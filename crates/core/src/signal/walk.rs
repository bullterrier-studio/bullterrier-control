//! How a graph is walked: a read and a drop each loop over a stack of their own on the heap, so a graph of any depth
//! is read and dropped on any thread's stack, by the same code on every target. A node's own code runs one frame
//! deep. Only a callback (user code: a `from_fn`, a recurrence, a thing's body, so a Python `compute`, `run` or body)
//! can read a signal or call a thing inside a read or a call: that is a walk of its own on the thread's stack, so
//! [`callback`] counts the callbacks running on a thread and refuses one nested more than [`NESTED`] deep.

use std::cell::{Cell, RefCell};
use std::mem::ManuallyDrop;
use std::sync::Arc;

use super::Signal;
use super::node::{Constant, Next, Node, Read};
use crate::Error;

/// How many callbacks may run one inside another on a thread, of every kind together: a `from_fn`, a recurrence
/// (`Signal::rendered`), a thing's body (`Thing::custom`), and so a Python `compute`, `run` or thing body. Each
/// level is a walk or a call on the thread's stack, which no loop can take over (user code is between two levels).
///
/// The cap is a count standing for bytes: the smallest thread stack it must fit is 256 KiB (a Python thread on macOS
/// gets 512 KiB). Measured in a release build on macOS arm64, a level takes about 815 B for a Rust `from_fn`,
/// 3,150 B for a Python `compute` and 4,000 B for a Python thing body (the largest), and a thread needs about 8 KiB
/// before the first: 8 KiB + 48 x 4,000 B = 196 KiB, leaving about 60 KiB of the 256 for the caller's own frames
/// (64 levels would leave 6 KiB). A callback that recurses through C (a deep `repr`, numpy internals) takes more a
/// level and is not covered.
pub(crate) const NESTED: usize = 48;

/// The most samples one read may hold at once across its graph: the grids and buffers of every node waiting on the
/// walk's stacks, 2^26 of them, 512 MiB of `f64`. A rendering is bounded by its own `limit`, but a chain of them each
/// holds a grid and a buffer for the same long read, so `MAX_DEPTH` of them would hold 1000 times that; a chain of
/// retimings each holds its mapped times and a buffer, two samples per time read. This is the bound on the read. A
/// node's finished history is kept apart (never more than half of what its read held). A read needs about two samples
/// per rendering for each sample of its longest time: 100 stacked renderings fit 5 minutes at 1000 Hz, 2 fit 4.6
/// hours. The read's own `times` and `out` belong to its caller and do not count.
pub(crate) const MOST_HELD: usize = 1 << 26;

thread_local! {
    /// How many callbacks are running on this thread, one inside another.
    static CALLBACKS: Cell<usize> = const { Cell::new(0) };
}

/// What `call` (user code) gives, counted as one more callback running on this thread; refused with
/// [`Error::Depth`] if [`NESTED`] are running. Every callback the crate calls goes through here.
pub(crate) fn callback<T>(call: impl FnOnce() -> Result<T, Error>) -> Result<T, Error> {
    let running = CALLBACKS.get();
    if running >= NESTED {
        return Err(Error::Depth {
            message: format!(
                "callbacks nest more than {NESTED} deep on one thread (a compute reading a signal whose compute \
                 reads another, a body calling a thing whose body calls another, ...: one reading its own signal or \
                 calling its own thing?)"
            ),
        });
    }
    CALLBACKS.set(running + 1);
    let _done = Counted;
    call()
}

/// One running callback, counted down when it returns or unwinds.
struct Counted;

impl Drop for Counted {
    fn drop(&mut self) {
        CALLBACKS.set(CALLBACKS.get().saturating_sub(1));
    }
}

/// Where a step's times are: the read's own, or the `index`-th grid on the walk's stack of grids.
#[derive(Clone, Copy)]
enum Times {
    Read,
    Grid(usize),
}

/// Where a step writes its values: the reader's buffer, or the `index`-th on the walk's stack of buffers.
#[derive(Clone, Copy)]
enum Out {
    Reader,
    Buffer(usize),
}

/// What a started node waits for.
#[derive(Clone, Copy)]
enum Waiting<'g> {
    /// Its first input is read into its `out`: next its second, into a buffer of its own.
    Second(&'g Signal),
    /// Its input is read into its `out`.
    InPlace,
    /// Its second input is read, into the top buffer.
    Buffer,
    /// Its input is read at the top grid, into the top buffer.
    Grid,
}

/// A node started and waiting for its inputs.
struct Frame<'g> {
    node: &'g (dyn Node + Send + Sync),
    times: Times,
    out: Out,
    waiting: Waiting<'g>,
}

/// `signal`'s values at `times` into `out`: every node's [`Node::start`], the inputs it names read depth first
/// (first before second, as a recursive read would), then its [`Node::finish`]. Grids and buffers are pushed and
/// popped with the frames that asked for them, so the read holds what a recursive read holds on its way down, on
/// the heap.
pub(super) fn read(signal: &Signal, times: &[f64], out: &mut [f64]) -> Read {
    let mut frames: Vec<Frame<'_>> = Vec::new();
    let mut grids: Vec<Vec<f64>> = Vec::new();
    let mut buffers: Vec<Vec<f64>> = Vec::new();
    let mut held = 0_usize;
    let mut step: Option<(&(dyn Node + Send + Sync), Times, Out)> = Some((&*signal.0, Times::Read, Out::Reader));
    loop {
        if let Some((node, at, into)) = step.take() {
            let next = node.start(times_of(times, &grids, at), out_of(out, &mut buffers, into))?;
            step = match next {
                Next::Done => None,
                Next::As(input) => Some((&*input.0, at, into)),
                Next::InPlace(input) => {
                    push(
                        &mut frames,
                        signal,
                        Frame {
                            node,
                            times: at,
                            out: into,
                            waiting: Waiting::InPlace,
                        },
                    );
                    Some((&*input.0, at, into))
                }
                Next::Both(first, second) => {
                    push(
                        &mut frames,
                        signal,
                        Frame {
                            node,
                            times: at,
                            out: into,
                            waiting: Waiting::Second(second),
                        },
                    );
                    Some((&*first.0, at, into))
                }
                Next::At(input, grid) => {
                    hold(&mut held, 2 * grid.len())?;
                    buffers.push(vec![0.0; grid.len()]);
                    grids.push(grid);
                    push(
                        &mut frames,
                        signal,
                        Frame {
                            node,
                            times: at,
                            out: into,
                            waiting: Waiting::Grid,
                        },
                    );
                    Some((&*input.0, Times::Grid(grids.len() - 1), Out::Buffer(buffers.len() - 1)))
                }
            };
            continue;
        }
        let Some(frame) = frames.last_mut() else {
            return Ok(());
        };
        if let Waiting::Second(second) = frame.waiting {
            hold(&mut held, times_of(times, &grids, frame.times).len())?;
            buffers.push(vec![0.0; times_of(times, &grids, frame.times).len()]);
            frame.waiting = Waiting::Buffer;
            step = Some((&*second.0, frame.times, Out::Buffer(buffers.len() - 1)));
            continue;
        }
        let Some(Frame {
            node,
            times: at,
            out: into,
            waiting,
        }) = frames.pop()
        else {
            return Ok(());
        };
        let inputs = match waiting {
            Waiting::InPlace | Waiting::Second(_) => Vec::new(),
            Waiting::Buffer => buffers.pop().unwrap_or_default(),
            Waiting::Grid => {
                held -= grids.pop().map_or(0, |grid| grid.len());
                buffers.pop().unwrap_or_default()
            }
        };
        held -= inputs.len();
        node.finish(times_of(times, &grids, at), &inputs, out_of(out, &mut buffers, into))?;
    }
}

/// `count` more samples held by the read, which is refused if that is over [`MOST_HELD`].
fn hold(held: &mut usize, count: usize) -> Read {
    *held += count;
    if *held > MOST_HELD {
        return Err(Box::new(Error::Signal {
            message: format!(
                "the read would hold more than {MOST_HELD} samples at once across its stacked shapers: read fewer \
                 or earlier times at once, or stack fewer shapers"
            ),
        }));
    }
    Ok(())
}

/// `frame` on top of `frames`, which the first push sizes for the deepest read of `signal` (a frame a node, the
/// leaf's excepted): one allocation a read that has inputs, none for one that has not.
fn push<'g>(frames: &mut Vec<Frame<'g>>, signal: &Signal, frame: Frame<'g>) {
    if frames.capacity() == 0 {
        frames.reserve_exact(signal.0.depth());
    }
    frames.push(frame);
}

/// The times at `at`.
fn times_of<'a>(read: &'a [f64], grids: &'a [Vec<f64>], at: Times) -> &'a [f64] {
    match at {
        Times::Read => read,
        Times::Grid(index) => &grids[index],
    }
}

/// The buffer at `into`.
fn out_of<'a>(reader: &'a mut [f64], buffers: &'a mut [Vec<f64>], into: Out) -> &'a mut [f64] {
    match into {
        Out::Reader => reader,
        Out::Buffer(index) => &mut buffers[index],
    }
}

thread_local! {
    /// A node no signal reads: what a dropped handle holds while its node drops. One a thread: one for all would be
    /// one count every thread's drops write, and drops on many threads at once would queue on it (7 times slower on 8
    /// threads).
    static VACANT: Arc<dyn Node + Send + Sync> = Arc::new(Constant(0.0));
}

/// The drop loop of a thread: whether one runs, and the nodes waiting for it.
struct Dropping {
    running: Cell<bool>,
    /// Empty whenever no loop runs; its buffer is kept for the next loop, and freed by [`FREED`] when the thread ends.
    waiting: RefCell<Vec<Arc<dyn Node + Send + Sync>>>,
}

thread_local! {
    /// This thread's drop loop. It has no destructor (`ManuallyDrop`), so it is never torn down: a signal another
    /// thread local holds is dropped by the loop too when the thread ends, whichever of them goes first.
    static DROPPING: ManuallyDrop<Dropping> = const {
        ManuallyDrop::new(Dropping {
            running: Cell::new(false),
            waiting: RefCell::new(Vec::new()),
        })
    };

    /// Frees the buffer of this thread's queue when the thread ends (a loop that runs later, inside another thread
    /// local's destructor, frees it as it ends).
    static FREED: Freed = const { Freed };
}

/// What [`FREED`] holds.
struct Freed;

impl Drop for Freed {
    fn drop(&mut self) {
        let waiting = DROPPING.with(|dropping| std::mem::take(&mut *dropping.waiting.borrow_mut()));
        drop(waiting);
    }
}

/// Drops `node`'s handle (leaving [`VACANT`] in its place). The last handle of a node drops it in a loop: a node
/// dropping drops its inputs' handles, and those only queue their nodes for the loop, so a drop of any depth runs
/// one node's frame deep. Every node takes this path, whatever it holds: none declares what it holds.
// `strong_count` is 1 only for the last handle (a signal has no weak handles). Two handles dropped at once on two
// threads may both see 2: the node then drops in the frame of whichever counts down last, its inputs' handles each
// checking again.
pub(super) fn release(node: &mut Arc<dyn Node + Send + Sync>) {
    if Arc::strong_count(node) != 1 {
        return;
    }
    // once the thread's own is gone (a signal dropped by a thread local's destructor after it), one for this drop
    let vacant = VACANT.try_with(Arc::clone).unwrap_or_else(|_| Arc::new(Constant(0.0)));
    let last = std::mem::replace(node, vacant);
    DROPPING.with(|dropping| {
        if dropping.running.replace(true) {
            dropping.waiting.borrow_mut().push(last);
            return;
        }
        let _done = Draining(dropping);
        drop(last);
        loop {
            let next = dropping.waiting.borrow_mut().pop();
            let Some(next) = next else { break };
            drop(next);
        }
    });
}

/// The loop of [`release`] running; marked done when it ends or unwinds (a panic in a node's drop: the nodes still
/// waiting are dropped by the thread's next loop).
struct Draining<'a>(&'a Dropping);

impl Drop for Draining<'_> {
    fn drop(&mut self) {
        self.0.running.set(false);
        // the first loop on a thread sets up `FREED`; once it is gone (the thread is ending), the loop frees the buffer
        if FREED.try_with(|_| ()).is_err() && self.0.waiting.borrow().is_empty() {
            drop(std::mem::take(&mut *self.0.waiting.borrow_mut()));
        }
    }
}
