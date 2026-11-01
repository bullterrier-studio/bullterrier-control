//! How a graph is walked: a read and a drop each loop over a stack of their own on the heap, so a graph of any depth
//! is read and dropped on any thread's stack, by the same code on every target. A node's own code runs one frame
//! deep. Only a callback (user code: a `from_fn`, a recurrence, a Python `compute`) can read a signal inside a read:
//! that read is a walk of its own on the thread's stack, so [`callback`] counts the callbacks running on a thread and
//! refuses one nested more than [`NESTED`] deep.

use std::cell::{Cell, RefCell};
use std::sync::{Arc, LazyLock};

use super::Signal;
use super::node::{Constant, Next, Node, Read};
use crate::Error;

/// How many callbacks may run one inside another on a thread (a `from_fn` reading a signal whose `from_fn` reads
/// another, ...). Each level is a read on the thread's stack: measured, release, about 0.5 KiB a Rust callback and
/// 2.9 KiB a Python `compute`.
pub(crate) const NESTED: usize = 64;

thread_local! {
    /// How many callbacks are running on this thread, one inside another.
    static CALLBACKS: Cell<usize> = const { Cell::new(0) };
}

/// What `call` gives, counted as one more callback running on this thread; refused with [`Error::Depth`] if
/// [`NESTED`] are running.
pub(super) fn callback<T>(call: impl FnOnce() -> Result<T, Error>) -> Result<T, Box<Error>> {
    let running = CALLBACKS.get();
    if running >= NESTED {
        return Err(Box::new(Error::Depth {
            message: format!(
                "callbacks that read signals nest more than {NESTED} deep (a callback reading a signal whose \
                 callback reads another, ...)"
            ),
        }));
    }
    CALLBACKS.set(running + 1);
    let _done = Counted;
    call().map_err(Box::new)
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
                grids.pop();
                buffers.pop().unwrap_or_default()
            }
        };
        node.finish(times_of(times, &grids, at), &inputs, out_of(out, &mut buffers, into))?;
    }
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

/// A node no signal reads: what a dropped handle holds while its node drops.
static VACANT: LazyLock<Arc<dyn Node + Send + Sync>> = LazyLock::new(|| Arc::new(Constant(0.0)));

thread_local! {
    /// The nodes waiting to drop on this thread, and whether a loop is dropping them.
    static DROPPING: (Cell<bool>, RefCell<Vec<Arc<dyn Node + Send + Sync>>>) =
        const { (Cell::new(false), RefCell::new(Vec::new())) };
}

/// Drops `node`'s handle (leaving [`VACANT`] in its place). The last handle of a node with inputs drops it in a loop:
/// a node dropping drops its inputs' handles, and those only queue their nodes for the loop, so a drop of any depth
/// runs one node's frame deep. A node that holds no signal (a constant, a series) drops at once, and so does any node
/// on a thread whose locals are gone.
// `strong_count` is 1 only for the last handle (a signal has no weak handles). Two handles dropped at once on two
// threads may both see 2: the node then drops in the frame of whichever counts down last, its inputs' handles each
// checking again.
pub(super) fn release(node: &mut Arc<dyn Node + Send + Sync>) {
    if Arc::strong_count(node) != 1 || !node.holds_signals() {
        return;
    }
    let last = std::mem::replace(node, Arc::clone(&VACANT));
    let first = DROPPING.try_with(move |(dropping, pending)| {
        if dropping.get() {
            pending.borrow_mut().push(last);
            return None;
        }
        dropping.set(true);
        Some(last)
    });
    if let Ok(Some(first)) = first {
        let _done = Draining;
        drop(first);
        while let Ok(Some(next)) = DROPPING.try_with(|(_, pending)| pending.borrow_mut().pop()) {
            drop(next);
        }
    }
}

/// The loop of [`release`] running on this thread; marked done when it ends or unwinds.
struct Draining;

impl Drop for Draining {
    fn drop(&mut self) {
        let _ = DROPPING.try_with(|(dropping, _)| dropping.set(false));
    }
}
