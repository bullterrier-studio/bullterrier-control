// The promises of `Signal`, through its public constructors, operators and `sample`; every expected number is worked
// out by hand.

use std::error::Error as _;
use std::fmt;

use proptest::prelude::*;

use crate::{Error, Signal};

/// A signal that is its own time, written as a plugin author would write one.
fn echo() -> Signal {
    Signal::from_fn(|times, out| {
        out.copy_from_slice(times);
        Ok::<(), Unreachable>(())
    })
}

/// The error of a callback that never fails.
#[derive(Debug)]
struct Unreachable;

impl fmt::Display for Unreachable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("unreachable")
    }
}

impl std::error::Error for Unreachable {}

/// A signal with a bug: every value is `value`.
fn broken(value: f64) -> Signal {
    Signal::from_fn(move |_, out| {
        out.fill(value);
        Ok::<(), Unreachable>(())
    })
}

/// The message of a refusal that must be an `Error::Signal`, or why it is not one.
fn signal_error<T: fmt::Debug>(result: &Result<T, Error>) -> String {
    match result {
        Err(Error::Signal { message }) => message.clone(),
        other => format!("not a signal error: {other:?}"),
    }
}

#[test]
fn a_signal_can_be_sent_to_and_shared_between_threads() {
    fn shareable<T: Send + Sync + Clone + fmt::Debug>() {}
    fn error<T: Send + Sync + std::error::Error + 'static>() {}
    shareable::<Signal>();
    error::<Error>();
}

#[test]
fn a_time_before_zero_reads_as_time_zero() -> Result<(), Error> {
    assert_eq!(echo().sample(&[-5.0, -0.001, 0.0, 2.0])?, [0.0, 0.0, 0.0, 2.0]);
    assert_eq!(echo().at(-1e300)?, 0.0);
    Ok(())
}

#[test]
fn times_and_values_that_are_not_finite_are_refused_naming_the_time() {
    // (what is read, at which times, what the refusal says)
    let echoed = echo();
    let nan = broken(f64::NAN);
    let inf = broken(f64::INFINITY);
    let minus_inf = broken(f64::NEG_INFINITY);
    let first_bad = Signal::from_fn(|times, out| {
        for (value, time) in out.iter_mut().zip(times) {
            *value = if *time > 1.0 { 1e308 * 10.0 } else { *time };
        }
        Ok::<(), Unreachable>(())
    });
    for (signal, times, says, rule) in [
        (
            &echoed,
            vec![0.0, f64::NAN],
            "times must be finite (got NaN)",
            "a NaN time",
        ),
        (
            &echoed,
            vec![f64::INFINITY],
            "times must be finite (got inf)",
            "an infinite time",
        ),
        (
            &echoed,
            vec![1.0, f64::NEG_INFINITY],
            "times must be finite (got -inf)",
            "minus infinity",
        ),
        (
            &nan,
            vec![0.5, 1.0],
            "not finite at time 0.5",
            "a NaN value names the first time",
        ),
        (&inf, vec![2.0], "not finite at time 2.0", "an infinite value"),
        (
            &minus_inf,
            vec![-3.0],
            "not finite at time -3.0",
            "the time as asked, before it reads as 0",
        ),
        (
            &first_bad,
            vec![0.5, 3.0, 2.0],
            "not finite at time 3.0",
            "an overflow: 1e308 * 10",
        ),
    ] {
        let refused = signal.sample(&times);
        let message = signal_error(&refused);
        assert!(message.contains(says), "{rule}: {message}");
    }
}

#[test]
fn finite_times_and_values_are_given_back() -> Result<(), Error> {
    // the edges of the good side: the largest finite time, and the largest finite value
    assert_eq!(echo().at(f64::MAX)?, f64::MAX);
    assert_eq!(broken(-f64::MAX).at(0.0)?, -f64::MAX);
    assert_eq!(echo().sample(&[])?, Vec::<f64>::new());
    Ok(())
}

#[test]
fn sample_into_fills_a_buffer_as_long_as_the_times_and_refuses_another_length() -> Result<(), Error> {
    let mut values = [9.0; 3];
    echo().sample_into(&[0.25, -1.0, 4.0], &mut values)?;
    assert_eq!(values, [0.25, 0.0, 4.0]);
    for (times, room) in [(vec![0.0, 1.0], 3), (vec![0.0, 1.0, 2.0], 2), (vec![], 1)] {
        let mut values = vec![0.0; room];
        let refused = echo().sample_into(&times, &mut values);
        let message = signal_error(&refused);
        assert!(message.contains(&format!("{} times", times.len())), "{message}");
    }
    Ok(())
}

#[test]
fn a_constant_is_its_value_at_every_time_and_must_be_finite() {
    // (value, what reading it at 0, 3.5 and 1e9 gives)
    for (value, expected) in [
        (0.25, Ok(vec![0.25; 3])),
        (-7.0, Ok(vec![-7.0; 3])),
        (f64::MAX, Ok(vec![f64::MAX; 3])),
        (f64::NAN, Err("a constant must be finite (got NaN)")),
        (f64::INFINITY, Err("a constant must be finite (got inf)")),
        (f64::NEG_INFINITY, Err("a constant must be finite (got -inf)")),
    ] {
        let read = Signal::constant(value).and_then(|constant| constant.sample(&[0.0, 3.5, 1e9]));
        match expected {
            Ok(values) => assert_eq!(read.ok(), Some(values), "{value}"),
            Err(says) => assert_eq!(signal_error(&read), says, "{value}"),
        }
    }
}

/// The error a plugin's callback returns.
#[derive(Debug, PartialEq)]
struct OutOfTune(&'static str);

impl fmt::Display for OutOfTune {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "out of tune: {}", self.0)
    }
}

impl std::error::Error for OutOfTune {}

#[test]
fn a_callbacks_error_is_kept_as_the_source_of_the_error() {
    let failing = Signal::from_fn(|_, _| Err(OutOfTune("A4 at 443 Hz")));
    let refused = failing.sample(&[0.0, 1.0]);
    let Err(error) = &refused else {
        assert!(refused.is_err(), "a failing callback gave values");
        return;
    };
    assert!(matches!(error, Error::Node { .. }), "{error:?}");
    let source = error.source().and_then(|source| source.downcast_ref::<OutOfTune>());
    assert_eq!(source, Some(&OutOfTune("A4 at 443 Hz")));
    assert_eq!(error.to_string(), "out of tune: A4 at 443 Hz");
}

#[test]
fn a_callback_sees_finite_times_from_zero_and_a_zeroed_buffer() -> Result<(), Error> {
    let lazy = Signal::from_fn(|times, out| {
        // writes only the first value, adds the times it saw to what it was given
        if let Some(first) = out.first_mut() {
            *first += 1.0;
        }
        for (value, time) in out.iter_mut().zip(times).skip(1) {
            *value += time;
        }
        Ok::<(), Unreachable>(())
    });
    let mut values = [5.0, 5.0, 5.0];
    lazy.sample_into(&[2.0, -4.0, 0.5], &mut values)?;
    assert_eq!(values, [1.0, 0.0, 0.5]);
    Ok(())
}

proptest! {
    #[test]
    fn a_read_gives_the_value_at_each_time_whatever_the_batch_or_order(
        times in prop::collection::vec(-10.0..1e3_f64, 0..40),
    ) {
        let signal = echo();
        let together = signal.sample(&times);
        let alone: Result<Vec<f64>, Error> = times.iter().map(|&time| signal.at(time)).collect();
        let mut reversed = times.clone();
        reversed.reverse();
        let mut backwards = signal.sample(&reversed).map_err(|error| TestCaseError::fail(error.to_string()))?;
        backwards.reverse();
        let together = together.map_err(|error| TestCaseError::fail(error.to_string()))?;
        prop_assert_eq!(&together, &alone.map_err(|error| TestCaseError::fail(error.to_string()))?);
        prop_assert_eq!(&together, &backwards);
    }
}
