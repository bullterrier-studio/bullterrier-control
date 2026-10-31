// The promises of `Signal::rendered`, through the public API; every expected number is worked out by hand.

use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Barrier, Condvar, Mutex, OnceLock, PoisonError};
use std::thread;
use std::time::Duration;

use proptest::prelude::*;

use crate::{Error, FromValues, Interpolation, MAX_DEPTH, Rendering, Signal};

/// How long a thread test waits for an answer before it calls the read hung.
const HUNG: Duration = Duration::from_secs(20);

/// A callback's own error, naming what went wrong.
#[derive(Debug)]
struct Failed(&'static str);

impl fmt::Display for Failed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for Failed {}

/// A causal recurrence: the running sum of the input, in seconds at 10 samples a second (a constant 1 integrates to the
/// time plus one step).
#[expect(
    clippy::unnecessary_wraps,
    reason = "a recurrence's form, as `Signal::rendered` takes it"
)]
fn integrate(inputs: &[f64]) -> Result<Vec<f64>, Failed> {
    let mut sum = 0.0;
    Ok(inputs
        .iter()
        .map(|input| {
            sum += input;
            sum / 10.0
        })
        .collect())
}

/// The running sum of the input.
#[expect(
    clippy::unnecessary_wraps,
    reason = "a recurrence's form, as `Signal::rendered` takes it"
)]
fn cumulate(inputs: &[f64]) -> Result<Vec<f64>, Failed> {
    let mut sum = 0.0;
    Ok(inputs
        .iter()
        .map(|input| {
            sum += input;
            sum
        })
        .collect())
}

fn at_rate(rate: f64) -> Rendering {
    Rendering {
        rate,
        ..Rendering::default()
    }
}

/// A signal that is its own time.
fn echo() -> Signal {
    Signal::from_fn(|times, out| {
        out.copy_from_slice(times);
        Ok::<(), Failed>(())
    })
}

/// The constant `value`, which is finite.
fn constant(value: f64) -> Result<Signal, Error> {
    Signal::constant(value)
}

/// The message of a refusal that must be an `Error::Signal`, or why it is not one.
fn signal_error<T: fmt::Debug>(result: &Result<T, Error>) -> String {
    match result {
        Err(Error::Signal { message }) => message.clone(),
        other => format!("not a signal error: {other:?}"),
    }
}

/// The length of every input a recurrence was called with, in order.
type Lengths = Arc<Mutex<Vec<usize>>>;

/// `integrate` over a constant 1 at `rendering`, and the length of every input `run` was called with.
fn counted(rendering: Rendering) -> Result<(Signal, Lengths), Error> {
    let lengths = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&lengths);
    let signal = Signal::rendered(&constant(1.0)?, rendering, move |inputs| {
        seen.lock().unwrap_or_else(PoisonError::into_inner).push(inputs.len());
        integrate(inputs)
    })?;
    Ok((signal, lengths))
}

fn lengths(seen: &Mutex<Vec<usize>>) -> Vec<usize> {
    seen.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

#[test]
fn a_constant_one_integrates_to_the_time_plus_one_step_and_a_time_between_samples_reads_the_earlier()
-> Result<(), Error> {
    let signal = Signal::rendered(&constant(1.0)?, at_rate(10.0), integrate)?;
    // the sum at sample k includes sample k: (k + 1) / 10
    assert_eq!(
        signal.sample(&[0.0, 0.05, 0.1, 0.15, 0.3, 2.55])?,
        [0.1, 0.1, 0.2, 0.2, 0.4, 2.6]
    );
    assert_eq!(signal.at(-7.0)?, 0.1, "a time before 0 reads as 0");
    Ok(())
}

#[test]
fn nothing_is_rendered_for_no_times() -> Result<(), Error> {
    let (signal, seen) = counted(at_rate(10.0))?;
    assert_eq!(signal.sample(&[])?, Vec::<f64>::new());
    assert_eq!(lengths(&seen), Vec::<usize>::new());
    Ok(())
}

#[test]
fn the_history_grows_by_doubling_from_1024_samples_capped_at_the_limit() -> Result<(), Error> {
    let (signal, seen) = counted(at_rate(10.0))?;
    signal.at(0.0)?; // 1 sample needed: the first render is 1024
    signal.at(102.3)?; // sample 1023: already rendered
    signal.at(102.4)?; // sample 1024 needs 1025: twice 1024
    signal.at(204.8)?; // sample 2048 needs 2049: twice 2048
    signal.at(1.0)?;
    assert_eq!(lengths(&seen), [1024, 2048, 4096]);

    let (capped, seen) = counted(Rendering {
        rate: 10.0,
        limit: 1500,
    })?;
    capped.at(0.0)?;
    assert_eq!(capped.at(120.0)?, 120.1); // sample 1200 needs 1201: twice 1024 is 2048, over the limit of 1500
    assert_eq!(lengths(&seen), [1024, 1500]);

    let (wide, seen) = counted(at_rate(10.0))?;
    wide.at(1000.0)?; // sample 10000 needs 10001, more than 1024
    assert_eq!(lengths(&seen), [10001]);
    Ok(())
}

#[test]
fn reading_later_and_later_renders_a_few_times_not_once_per_read() -> Result<(), Error> {
    let (signal, seen) = counted(at_rate(10.0))?;
    for second in 1..=300 {
        assert_eq!(signal.at(f64::from(second))?, f64::from(second * 10 + 1) / 10.0);
    }
    assert_eq!(lengths(&seen), [1024, 2048, 4096]);
    Ok(())
}

#[test]
fn the_limit_is_exact_a_read_needing_the_limit_works_and_one_more_is_refused_before_rendering() -> Result<(), Error> {
    let lengths = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&lengths);
    let signal = Signal::rendered(&constant(1.0)?, Rendering { rate: 1.0, limit: 100 }, move |inputs| {
        seen.lock().unwrap_or_else(PoisonError::into_inner).push(inputs.len());
        cumulate(inputs)
    })?;
    let refused = signal.at(100.0); // sample 100 needs 101
    assert!(signal_error(&refused).contains("over the limit"), "{refused:?}");
    assert_eq!(self::lengths(&lengths), Vec::<usize>::new(), "refused before rendering");
    assert_eq!(signal.sample(&[99.0, 99.5])?, [100.0, 100.0]); // samples 0..=99: 100 of them
    assert_eq!(self::lengths(&lengths), [100]);
    Ok(())
}

#[test]
fn a_read_far_over_the_limit_is_refused_before_the_input_is_read_or_anything_allocated() -> Result<(), Error> {
    let reads = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&reads);
    let input = Signal::from_fn(move |_, _| {
        counted.fetch_add(1, Ordering::SeqCst);
        Ok::<(), Failed>(())
    });
    let signal = Signal::rendered(&input, at_rate(10.0), integrate)?;
    // 1e12 s needs 1e13 samples (80 TB); 1.797e308 s at 10 Hz overflows to an infinite count
    for time in [1e12, 1e308, 1.797e308] {
        let refused = signal.at(time);
        let message = signal_error(&refused);
        assert!(
            message.contains("over the limit") && message.contains("20000000"),
            "{time}: {message}"
        );
    }
    assert_eq!(reads.load(Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn a_recurrence_that_changes_the_length_is_refused_naming_both_lengths() -> Result<(), Error> {
    let short = Signal::rendered(&constant(1.0)?, at_rate(10.0), |inputs| {
        Ok::<_, Failed>(inputs[1..].to_vec())
    })?;
    let message = signal_error(&short.at(1.0));
    assert!(message.contains("1023") && message.contains("1024"), "{message}");
    let long = Signal::rendered(&constant(1.0)?, at_rate(10.0), |inputs| {
        Ok::<_, Failed>([inputs, &[0.0]].concat())
    })?;
    let message = signal_error(&long.at(1.0));
    assert!(message.contains("1025") && message.contains("1024"), "{message}");
    Ok(())
}

#[test]
fn an_error_of_the_recurrence_or_of_the_input_comes_through_as_its_own() -> Result<(), Error> {
    let failing = Signal::rendered(&constant(1.0)?, at_rate(10.0), |_| {
        Err::<Vec<f64>, _>(Failed("diverged"))
    })?;
    let error = failing.at(0.5);
    assert!(
        matches!(&error, Err(Error::Node { source }) if source.to_string() == "diverged"),
        "{error:?}"
    );
    let broken = Signal::from_fn(|_, _| Err(Failed("unplugged")));
    let error = Signal::rendered(&broken, at_rate(10.0), integrate)?.at(0.5);
    assert!(
        matches!(&error, Err(Error::Node { source }) if source.to_string() == "unplugged"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_rate_or_a_limit_out_of_range_is_refused_naming_it() -> Result<(), Error> {
    // 1e-310: the last sample's time, 2e7 / rate, is not a number
    for rate in [0.0, -1.0, f64::INFINITY, f64::NAN, 1e-310] {
        let refused = Signal::rendered(&constant(1.0)?, at_rate(rate), integrate);
        let message = signal_error(&refused);
        assert!(
            message.starts_with("rate") && message.contains("rate"),
            "{rate}: {refused:?}"
        );
    }
    let refused = Signal::rendered(&constant(1.0)?, Rendering { rate: 10.0, limit: 0 }, integrate);
    assert!(signal_error(&refused).starts_with("limit"), "{refused:?}");
    let smallest = Signal::rendered(&constant(1.0)?, Rendering { rate: 10.0, limit: 1 }, integrate)?;
    assert_eq!(smallest.at(0.05)?, 0.1);
    Ok(())
}

#[test]
fn the_default_rendering_is_1000_samples_a_second_up_to_20_million() {
    assert_eq!(
        Rendering::default(),
        Rendering {
            rate: 1000.0,
            limit: 20_000_000
        }
    );
}

#[test]
fn a_read_never_depends_on_the_input_after_the_time_read() -> Result<(), Error> {
    let mut values = vec![0.0; 10];
    values.extend([1.0; 50]);
    let steps = Signal::from_values(&values, 10.0, FromValues::default(), Interpolation::Hold)?;
    let copy = Signal::rendered(&steps, at_rate(10.0), |inputs| Ok::<_, Failed>(inputs.to_vec()))?;
    // the input steps up at 1.0 s; a read at 0.95 or 0.99 still sees 0, however close the step is
    assert_eq!(copy.sample(&[0.0, 0.5, 0.95, 0.99])?, [0.0, 0.0, 0.0, 0.0]);
    assert_eq!(copy.sample(&[1.0, 1.04, 1.5])?, [1.0, 1.0, 1.0]);
    Ok(())
}

#[test]
fn a_time_that_is_a_samples_time_in_decimals_reads_that_sample_despite_float_rounding() -> Result<(), Error> {
    // 0.29 s at 100 samples a second is 28.999999999999996 samples in floats: still sample 29, whose value is 0.29
    let copy = Signal::rendered(&echo(), at_rate(100.0), |inputs| Ok::<_, Failed>(inputs.to_vec()))?;
    assert_eq!(copy.sample(&[0.29, 0.57])?, [0.29, 0.57]);
    Ok(())
}

#[test]
fn a_value_that_is_not_finite_in_the_input_comes_through_and_only_a_read_there_is_refused() -> Result<(), Error> {
    let broken_after_one = Signal::from_fn(|times, out| {
        for (value, time) in out.iter_mut().zip(times) {
            *value = if *time < 1.0 { 1.0 } else { f64::NAN };
        }
        Ok::<(), Failed>(())
    });
    let copy = Signal::rendered(&broken_after_one, at_rate(10.0), |inputs| {
        Ok::<_, Failed>(inputs.to_vec())
    })?;
    assert_eq!(copy.at(0.5)?, 1.0);
    assert!(signal_error(&copy.at(1.5)).contains("not finite at time 1.5"));
    Ok(())
}

#[test]
fn a_rendered_signal_over_the_deepest_input_is_refused() -> Result<(), Error> {
    let mut deep = echo(); // 1 deep
    for _ in 2..MAX_DEPTH {
        deep = deep + 1.0;
    }
    // MAX_DEPTH - 1 deep: the rendered signal over it is MAX_DEPTH deep
    let deepest = Signal::rendered(&deep, at_rate(10.0), integrate)?;
    assert_eq!(deepest.at(0.0)?, 99.8); // (0 + 998) / 10
    let refused = Signal::rendered(&(deep + 1.0), at_rate(10.0), integrate);
    assert!(signal_error(&refused).contains("deep"), "{refused:?}");
    Ok(())
}

/// The values `read` gives on another thread, or why there are none: an error, or no answer within [`HUNG`].
fn on_a_thread(read: impl FnOnce() -> Result<Vec<f64>, Error> + Send + 'static) -> Result<Vec<f64>, String> {
    let (send, receive) = mpsc::channel();
    thread::spawn(move || send.send(read()));
    match receive.recv_timeout(HUNG) {
        Ok(result) => result.map_err(|error| error.to_string()),
        Err(_) => Err(format!("no answer in {HUNG:?}: hung")),
    }
}

#[test]
fn two_threads_racing_a_first_render_both_run_it_outside_any_lock_and_read_the_same_values() -> Result<(), Error> {
    // each run waits until both threads are inside one: a lock held across `run` keeps the second out, and the first
    // gives up waiting
    let inside = Arc::new((Mutex::new(0_usize), Condvar::new()));
    let gate = Arc::clone(&inside);
    let signal = Signal::rendered(&constant(1.0)?, at_rate(10.0), move |inputs| {
        let (count, arrived) = &*gate;
        let mut count = count.lock().unwrap_or_else(PoisonError::into_inner);
        *count += 1;
        arrived.notify_all();
        let (count, waited) = arrived
            .wait_timeout_while(count, Duration::from_secs(5), |count| *count < 2)
            .unwrap_or_else(PoisonError::into_inner);
        drop(count);
        if waited.timed_out() {
            return Err(Failed("the other thread never came inside run"));
        }
        integrate(inputs)
    })?;
    let start = Arc::new(Barrier::new(2));
    let readers: Vec<_> = (0..2)
        .map(|_| {
            let (signal, start) = (signal.clone(), Arc::clone(&start));
            let (send, receive) = mpsc::channel();
            thread::spawn(move || {
                start.wait();
                send.send(signal.sample(&[40.0, 0.25]).map_err(|error| error.to_string()))
            });
            receive
        })
        .collect();
    for reader in readers {
        let values = reader.recv_timeout(HUNG).map_err(|_| "hung".to_owned());
        assert_eq!(values, Ok(Ok(vec![40.1, 0.3])));
    }
    assert_eq!(
        *inside.0.lock().unwrap_or_else(PoisonError::into_inner),
        2,
        "both rendered"
    );
    // the history kept is one of the two: a read after gives the same values and renders nothing more
    assert_eq!(
        on_a_thread(move || signal.sample(&[40.0, 0.25, 102.3])),
        Ok(vec![40.1, 0.3, 102.4])
    );
    assert_eq!(
        *inside.0.lock().unwrap_or_else(PoisonError::into_inner),
        2,
        "nothing more rendered"
    );
    Ok(())
}

#[test]
fn a_render_that_ends_after_a_longer_one_keeps_the_longer() -> Result<(), Error> {
    // a read at 0 s renders 1024 samples, and its `run` waits until a read at 1000 s (10001 samples) has finished
    let longer_read = Arc::new((Mutex::new(false), Condvar::new()));
    let lengths = Arc::new(Mutex::new(Vec::new()));
    let (done, seen) = (Arc::clone(&longer_read), Arc::clone(&lengths));
    let (inside, entered) = mpsc::channel();
    let signal = Signal::rendered(&constant(1.0)?, at_rate(10.0), move |inputs| {
        seen.lock().unwrap_or_else(PoisonError::into_inner).push(inputs.len());
        if inputs.len() == 1024 {
            let _ = inside.send(());
            let (finished, ended) = &*done;
            let finished = finished.lock().unwrap_or_else(PoisonError::into_inner);
            let (finished, waited) = ended
                .wait_timeout_while(finished, Duration::from_secs(5), |finished| !*finished)
                .unwrap_or_else(PoisonError::into_inner);
            drop(finished);
            if waited.timed_out() {
                return Err(Failed("the longer read never finished"));
            }
        }
        integrate(inputs)
    })?;
    let short = signal.clone();
    let (send, early) = mpsc::channel();
    thread::spawn(move || send.send(short.sample(&[0.0]).map_err(|error| error.to_string())));
    // the short read is inside its `run` before the long read starts
    assert_eq!(entered.recv_timeout(HUNG), Ok(()));
    let long = signal.clone();
    assert_eq!(on_a_thread(move || long.sample(&[1000.0])), Ok(vec![1000.1]));
    *longer_read.0.lock().unwrap_or_else(PoisonError::into_inner) = true;
    longer_read.1.notify_all();
    assert_eq!(
        early.recv_timeout(HUNG).map_err(|_| "hung".to_owned()),
        Ok(Ok(vec![0.1]))
    );
    // the 10001 samples are kept: a read at 500 s renders nothing
    assert_eq!(on_a_thread(move || signal.sample(&[500.0])), Ok(vec![500.1]));
    assert_eq!(self::lengths(&lengths), [1024, 10001]);
    Ok(())
}

#[test]
fn sixteen_threads_reading_one_rendered_signal_get_the_serial_values() -> Result<(), Error> {
    let times: Vec<f64> = (0..64).map(|step| f64::from(step) * 6.25).collect();
    let serial = Signal::rendered(&echo(), at_rate(10.0), integrate)?.sample(&times)?;
    let shared = Signal::rendered(&echo(), at_rate(10.0), integrate)?;
    let start = Arc::new(Barrier::new(16));
    let readers: Vec<_> = times
        .chunks(4)
        .rev() // the far chunks first
        .map(|chunk| {
            let (shared, start, chunk) = (shared.clone(), Arc::clone(&start), chunk.to_vec());
            let (send, receive) = mpsc::channel();
            thread::spawn(move || {
                start.wait();
                send.send(shared.sample(&chunk).map_err(|error| error.to_string()))
            });
            receive
        })
        .collect();
    let mut parts = Vec::new();
    for reader in readers {
        parts.push(reader.recv_timeout(HUNG).map_err(|_| "hung".to_owned()));
    }
    let expected: Vec<_> = serial.chunks(4).rev().map(|chunk| Ok(Ok(chunk.to_vec()))).collect();
    assert_eq!(parts, expected);
    Ok(())
}

#[test]
fn an_input_that_reads_the_same_rendered_signal_while_it_renders_does_not_deadlock() -> Result<(), Error> {
    let rendered: Arc<OnceLock<Signal>> = Arc::new(OnceLock::new());
    let nested: Arc<Mutex<Option<Result<f64, String>>>> = Arc::new(Mutex::new(None));
    let entered = Arc::new(AtomicBool::new(false));
    let (inner, seen) = (Arc::clone(&rendered), Arc::clone(&nested));
    // its own time; the first time it is read, it also reads the signal rendered from it (once: a read inside reads it
    // again, which must not read the signal yet again)
    let input = Signal::from_fn(move |times, out| {
        out.copy_from_slice(times);
        if let Some(signal) = inner.get()
            && !entered.swap(true, Ordering::SeqCst)
        {
            let value = signal.at(0.25).map_err(|error| error.to_string());
            *seen.lock().unwrap_or_else(PoisonError::into_inner) = Some(value);
        }
        Ok::<(), Failed>(())
    });
    let signal = Signal::rendered(&input, at_rate(10.0), |inputs| Ok::<_, Failed>(inputs.to_vec()))?;
    let _ = rendered.set(signal.clone());
    // sample k of the copy is k / 10: 0.25 reads sample 2, 3.0 sample 30
    assert_eq!(on_a_thread(move || signal.sample(&[0.25, 3.0])), Ok(vec![0.2, 3.0]));
    assert_eq!(*nested.lock().unwrap_or_else(PoisonError::into_inner), Some(Ok(0.2)));
    Ok(())
}

fn hold(values: &[f64]) -> Result<Signal, Error> {
    Signal::from_values(values, 3.0, FromValues::default(), Interpolation::Hold)
}

proptest! {
    #[test]
    fn a_rendered_signal_gives_the_same_values_whatever_the_order_and_batches_of_the_reads(
        values in prop::collection::vec(-5.0_f64..=5.0, 1..8),
        times in prop::collection::vec(0.0_f64..=300.0, 1..12),
        split in 0_usize..12,
    ) {
        let fresh = || Signal::rendered(&hold(&values)?, at_rate(10.0), integrate);
        let all_at_once = fresh().and_then(|signal| signal.sample(&times));
        let reader = fresh().map_err(|error| TestCaseError::fail(error.to_string()))?;
        let mut backwards = Vec::new();
        for time in times.iter().rev() {
            backwards.push(reader.at(*time).map_err(|error| TestCaseError::fail(error.to_string()))?);
        }
        backwards.reverse();
        let halves = fresh().and_then(|signal| {
            let (early, late) = times.split_at(split.min(times.len()));
            Ok([signal.sample(late)?, signal.sample(early)?])
        });
        let all_at_once = all_at_once.map_err(|error| TestCaseError::fail(error.to_string()))?;
        prop_assert_eq!(&backwards, &all_at_once);
        let [late, early] = halves.map_err(|error| TestCaseError::fail(error.to_string()))?;
        prop_assert_eq!([early, late].concat(), all_at_once);
    }
}
