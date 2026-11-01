// The promises of `Signal`, through its public constructors, operators and `sample`; every expected number is worked
// out by hand.

use std::error::Error as _;
use std::fmt;

use proptest::prelude::*;

use crate::formats::{Series, SeriesOptions, Unit};
use crate::{Error, FromValues, Interpolation, MAX_DEPTH, Signal, maximum, minimum};

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

/// The message of a refusal that must be an `Error::Depth`, or why it is not one.
fn depth_error<T: fmt::Debug>(result: &Result<T, Error>) -> String {
    match result {
        Err(Error::Depth { message }) => message.clone(),
        other => format!("not a depth error: {other:?}"),
    }
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
fn read_into_passes_a_value_that_is_not_finite_and_refuses_what_sample_into_refuses_of_the_times() -> Result<(), Error>
{
    let mut values = [9.0; 3];
    (broken(f64::NAN) + echo()).read_into(&[0.0, -2.0, 1.0], &mut values)?;
    assert!(values.iter().all(|value| value.is_nan()), "{values:?}");
    (broken(f64::INFINITY) * -1.0).read_into(&[0.5, 1.0, 2.0], &mut values)?;
    assert_eq!(values, [f64::NEG_INFINITY; 3]);
    // a time before 0 reads as 0, as for sample_into
    echo().read_into(&[0.25, -1.0, 4.0], &mut values)?;
    assert_eq!(values, [0.25, 0.0, 4.0]);
    for (times, room, named) in [
        (vec![0.0, f64::NAN], 2, "NaN"),
        (vec![f64::NEG_INFINITY], 1, "inf"),
        (vec![0.0, 1.0], 3, "2 times"),
    ] {
        let mut values = vec![0.0; room];
        let message = signal_error(&echo().read_into(&times, &mut values));
        assert!(message.contains(named), "{message}");
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

/// Whether `got` is `expected` to within float rounding (a millionth of a millionth).
fn close(got: &[f64], expected: &[f64]) -> bool {
    got.len() == expected.len()
        && got
            .iter()
            .zip(expected)
            .all(|(got, expected)| (got - expected).abs() <= 1e-12)
}

/// A ratio series of `values` from `start` at `rate`.
fn series(values: &[f64], start: f64, rate: f64) -> Result<Series, Error> {
    let options = SeriesOptions {
        start,
        ..Default::default()
    };
    Series::new(rate, values.to_vec(), Unit::Ratio, options).map_err(|source| Error::Format { source })
}

#[test]
fn a_series_signal_interpolates_or_holds_between_samples_and_holds_its_ends() -> Result<(), Error> {
    // up from 1 s to 1.5 s, down by 2 s; (interpolation, times, values, rule)
    let ramp = series(&[0.0, 1.0, 0.0], 1.0, 2.0)?;
    // 0.0 at 0 s, 1.0 at 0.01 s ... 29.0 at 0.29 s
    let counted: Vec<f64> = (0..30).map(f64::from).collect();
    let hundred = series(&counted, 0.0, 100.0)?;
    for (data, interpolation, times, expected, rule) in [
        (
            &ramp,
            Interpolation::Linear,
            vec![0.0, 0.5, -2.0],
            vec![0.0, 0.0, 0.0],
            "before the series",
        ),
        (
            &ramp,
            Interpolation::Linear,
            vec![1.0, 1.5, 2.0],
            vec![0.0, 1.0, 0.0],
            "on the samples",
        ),
        (
            &ramp,
            Interpolation::Linear,
            vec![1.25, 1.75, 1.125],
            vec![0.5, 0.5, 0.25],
            "between",
        ),
        (
            &ramp,
            Interpolation::Linear,
            vec![2.5, 9.0, 1e300],
            vec![0.0, 0.0, 0.0],
            "after the series",
        ),
        (
            &ramp,
            Interpolation::Hold,
            vec![0.0, 0.9],
            vec![0.0, 0.0],
            "held before the series",
        ),
        (
            &ramp,
            Interpolation::Hold,
            vec![1.0, 1.5, 2.0],
            vec![0.0, 1.0, 0.0],
            "held on a sample",
        ),
        (
            &ramp,
            Interpolation::Hold,
            vec![1.4, 1.9],
            vec![0.0, 1.0],
            "just before a sample the previous holds",
        ),
        (
            &ramp,
            Interpolation::Hold,
            vec![2.4, 100.0],
            vec![0.0, 0.0],
            "held after the series",
        ),
        (
            &hundred,
            Interpolation::Hold,
            vec![0.29, 0.07, 0.295],
            vec![29.0, 7.0, 29.0],
            "a decimal sample time",
        ),
        (
            &hundred,
            Interpolation::Linear,
            vec![0.29, 0.005],
            vec![29.0, 0.5],
            "linear at a decimal time",
        ),
    ] {
        let values = Signal::from_series(data, interpolation).sample(&times)?;
        assert!(close(&values, &expected), "{rule}: {values:?}");
    }
    Ok(())
}

#[test]
fn a_series_of_one_sample_is_that_value_at_every_time() -> Result<(), Error> {
    let single = series(&[0.7], 3.0, 10.0)?;
    for interpolation in [Interpolation::Linear, Interpolation::Hold] {
        let values = Signal::from_series(&single, interpolation).sample(&[0.0, 3.0, 3.05, 1000.0])?;
        assert_eq!(values, [0.7; 4], "{interpolation:?}");
    }
    Ok(())
}

#[test]
fn a_series_read_at_the_edge_of_the_floats_holds_its_last_value_or_is_refused() -> Result<(), Error> {
    let held = Signal::from_series(&series(&[1.0, 2.0, 3.0], 0.0, 1.0)?, Interpolation::Hold);
    assert_eq!(held.sample(&[1e19, 1e300, f64::MAX])?, [3.0, 3.0, 3.0]);
    // (time - start) * rate overflows to infinity: past the end
    let fast = series(&[1.0, 2.0], 0.0, 1e300)?;
    for interpolation in [Interpolation::Linear, Interpolation::Hold] {
        assert_eq!(Signal::from_series(&fast, interpolation).sample(&[1.0])?, [2.0]);
    }
    // halfway between the two largest floats of opposite sign: the slope overflows
    let huge = Signal::from_series(&series(&[-1.7e308, 1.7e308], 0.0, 1.0)?, Interpolation::Linear);
    let message = signal_error(&huge.sample(&[0.0, 0.5]));
    assert!(message.contains("not finite at time 0.5"), "{message}");
    // on a sample between two that far apart the value is that sample's, as numpy's interp gives it, sign of zero too
    let apart = Signal::from_series(
        &series(&[0.0, -1.7e308, 1.7e308, -0.0, 1.0], 0.0, 1.0)?,
        Interpolation::Linear,
    );
    let values = apart.sample(&[1.0, 2.0, 3.0])?;
    assert_eq!(values, [-1.7e308, 1.7e308, 0.0]);
    assert!(values[2].is_sign_negative(), "{values:?}");
    Ok(())
}

#[test]
fn values_make_a_signal_in_one_call_or_are_refused_naming_the_field() {
    // (values, rate, start, interpolation, times, the values or the field refused)
    for (values, rate, start, interpolation, times, expected) in [
        (
            vec![0.0, 1.0, 0.2, 0.0],
            10.0,
            0.0,
            Interpolation::Linear,
            vec![0.0, 0.05, 0.1, 0.25, 0.3, 1.0],
            Ok(vec![0.0, 0.5, 1.0, 0.1, 0.0, 0.0]),
        ),
        (
            vec![0.0, 1.0, 0.2, 0.0],
            10.0,
            0.0,
            Interpolation::Hold,
            vec![0.05, 0.1, 0.15],
            Ok(vec![0.0, 1.0, 1.0]),
        ),
        (
            vec![0.4, 0.8],
            10.0,
            1.0,
            Interpolation::Linear,
            vec![0.0, 0.5, 1.0, 1.05, 1.1, 5.0],
            Ok(vec![0.4, 0.4, 0.4, 0.6, 0.8, 0.8]),
        ),
        (vec![], 10.0, 0.0, Interpolation::Linear, vec![0.0], Err("values")),
        (
            vec![0.0, f64::NAN],
            10.0,
            0.0,
            Interpolation::Linear,
            vec![0.0],
            Err("values[1]"),
        ),
        (vec![0.0, 1.0], 0.0, 0.0, Interpolation::Hold, vec![0.0], Err("rate")),
        (
            vec![0.0, 1.0],
            10.0,
            -1.0,
            Interpolation::Linear,
            vec![0.0],
            Err("start"),
        ),
    ] {
        let made = Signal::from_values(&values, rate, FromValues { start }, interpolation);
        match (made.and_then(|signal| signal.sample(&times)), expected) {
            (Ok(got), Ok(expected)) => assert!(close(&got, &expected), "{values:?} at {times:?}: {got:?}"),
            (Err(Error::Format { source }), Err(field)) => {
                let fields: Vec<&str> = source.problems.iter().map(|problem| problem.path.as_str()).collect();
                assert_eq!(fields, [field], "{values:?}");
            }
            (got, expected) => assert_eq!(format!("{got:?}"), format!("{expected:?}"), "{values:?}"),
        }
    }
}

proptest! {
    #[test]
    fn a_series_signal_gives_back_each_sample_at_its_own_time(
        start in 0.0..10.0_f64,
        rate in 0.5..1000.0_f64,
        values in prop::collection::vec(-1e3..1e3_f64, 1..40),
    ) {
        let data = series(&values, start, rate).map_err(|error| TestCaseError::fail(error.to_string()))?;
        let times: Vec<f64> = (0_u32..).zip(&values).map(|(index, _)| start + f64::from(index) / rate).collect();
        for interpolation in [Interpolation::Linear, Interpolation::Hold] {
            let got = Signal::from_series(&data, interpolation)
                .sample(&times)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            for (got, expected) in got.iter().zip(&values) {
                prop_assert!((got - expected).abs() <= 1e-6, "{:?}: {} for {}", interpolation, got, expected);
            }
        }
    }
}

/// Up from 1 s to 1.5 s, down by 2 s; at [`TIMES`]: 0, 0.5, 1, 0, 0.
fn ramp() -> Result<Signal, Error> {
    Ok(Signal::from_series(
        &series(&[0.0, 1.0, 0.0], 1.0, 2.0)?,
        Interpolation::Linear,
    ))
}

/// The same samples held; at [`TIMES`]: 0, 0, 1, 0, 0.
fn steps() -> Result<Signal, Error> {
    Ok(Signal::from_series(
        &series(&[0.0, 1.0, 0.0], 1.0, 2.0)?,
        Interpolation::Hold,
    ))
}

const TIMES: [f64; 5] = [0.0, 1.25, 1.5, 2.0, 9.0];

#[test]
fn signals_combine_with_arithmetic_point_by_point() -> Result<(), Error> {
    let (ramp, steps) = (ramp()?, steps()?);
    let (quarter, point_four) = (Signal::constant(0.25)?, Signal::constant(0.4)?);
    for (combined, expected, rule) in [
        (&ramp + 1.0, [1.0, 1.5, 2.0, 1.0, 1.0], "a signal plus a number"),
        (1.0 + &ramp, [1.0, 1.5, 2.0, 1.0, 1.0], "a number plus a signal"),
        (&ramp - 0.5, [-0.5, 0.0, 0.5, -0.5, -0.5], "a signal minus a number"),
        (2.0 - &ramp, [2.0, 1.5, 1.0, 2.0, 2.0], "a number minus a signal"),
        (&ramp * 3.0, [0.0, 1.5, 3.0, 0.0, 0.0], "a signal times a number"),
        (3.0 * &ramp, [0.0, 1.5, 3.0, 0.0, 0.0], "a number times a signal"),
        (-&ramp, [-0.0, -0.5, -1.0, -0.0, -0.0], "a negated signal"),
        (&ramp + &ramp, [0.0, 1.0, 2.0, 0.0, 0.0], "two signals added"),
        (&ramp * &ramp, [0.0, 0.25, 1.0, 0.0, 0.0], "two signals multiplied"),
        (&ramp - &steps, [0.0, 0.5, 0.0, 0.0, 0.0], "two signals subtracted"),
        (&steps - &ramp, [0.0, -0.5, 0.0, 0.0, 0.0], "subtracted the other way"),
        (
            minimum(&ramp, &point_four),
            [0.0, 0.4, 0.4, 0.0, 0.0],
            "the smaller of two",
        ),
        (
            maximum(&point_four, &ramp),
            [0.4, 0.5, 1.0, 0.4, 0.4],
            "the larger of two",
        ),
        (
            &ramp * (1.0 - &quarter) + &steps * &quarter,
            [0.0, 0.375, 1.0, 0.0, 0.0],
            "a crossfade built from arithmetic",
        ),
        (
            (ramp.clone() + 1.0) * 2.0 - ramp.clone(),
            [2.0, 2.5, 3.0, 2.0, 2.0],
            "owned signals combine again",
        ),
    ] {
        let values = combined.sample(&TIMES)?;
        assert!(close(&values, &expected), "{rule}: {values:?}");
    }
    // negation keeps the sign of zero, as numpy's does
    assert!((-&ramp).at(0.0)?.is_sign_negative());
    Ok(())
}

#[test]
fn a_combination_reads_times_before_zero_as_zero_like_its_parts() -> Result<(), Error> {
    assert_eq!((&ramp()? + &echo()).sample(&[-3.0, 1.25])?, [0.0, 1.75]);
    Ok(())
}

#[test]
fn a_combination_that_is_not_finite_is_refused_naming_the_time() -> Result<(), Error> {
    let ramp = ramp()?;
    let zero = Signal::constant(0.0)?;
    // (the combination, what its refusal says, rule)
    for (combined, says, rule) in [
        (Signal::constant(1e308)? * 10.0, "not finite at time 0.5", "an overflow"),
        (&ramp + f64::NAN, "a constant must be finite (got NaN)", "a NaN number"),
        (
            f64::INFINITY * &ramp,
            "a constant must be finite (got inf)",
            "an infinite number",
        ),
        (
            minimum(&(&ramp - f64::INFINITY), &zero),
            "a constant must be finite (got inf)",
            "not hidden by the smaller of two",
        ),
        (
            maximum(&broken(f64::NAN), &zero),
            "not finite at time 0.5",
            "a NaN is the larger",
        ),
        (
            minimum(&zero, &broken(f64::NAN)),
            "not finite at time 0.5",
            "a NaN is the smaller",
        ),
        (
            &broken(f64::INFINITY) - &broken(f64::INFINITY),
            "not finite at time 0.5",
            "inf - inf",
        ),
    ] {
        let message = signal_error(&combined.sample(&[0.5, 1.0]));
        assert!(message.contains(says), "{rule}: {message}");
    }
    Ok(())
}

#[test]
fn a_callbacks_error_comes_through_a_combination() {
    let failing = Signal::from_fn(|_, _| Err(OutOfTune("E2")));
    for combined in [
        &failing + 1.0,
        2.0 * &failing,
        &echo() - &failing,
        maximum(&failing, &echo()),
    ] {
        let refused = combined.sample(&[1.0]);
        let source = refused
            .as_ref()
            .err()
            .and_then(|error| error.source())
            .map(ToString::to_string);
        assert_eq!(source.as_deref(), Some("out of tune: E2"), "{refused:?}");
    }
}

/// A signal graph as data, read by [`Expression::value`] with plain arithmetic: the oracle of the operators.
#[derive(Clone, Debug)]
enum Expression {
    Number(f64),
    /// The time, from 0 up.
    Time,
    Add(Box<Expression>, Box<Expression>),
    Subtract(Box<Expression>, Box<Expression>),
    Multiply(Box<Expression>, Box<Expression>),
    Larger(Box<Expression>, Box<Expression>),
    Smaller(Box<Expression>, Box<Expression>),
    Negate(Box<Expression>),
    PlusNumber(Box<Expression>, f64),
    NumberMinus(f64, Box<Expression>),
    TimesNumber(Box<Expression>, f64),
}

impl Expression {
    fn signal(&self) -> Result<Signal, Error> {
        Ok(match self {
            Self::Number(number) => Signal::constant(*number)?,
            Self::Time => echo(),
            Self::Add(first, second) => first.signal()? + second.signal()?,
            Self::Subtract(first, second) => &first.signal()? - &second.signal()?,
            Self::Multiply(first, second) => first.signal()? * &second.signal()?,
            Self::Larger(first, second) => maximum(&first.signal()?, &second.signal()?),
            Self::Smaller(first, second) => minimum(&first.signal()?, &second.signal()?),
            Self::Negate(inner) => -inner.signal()?,
            Self::PlusNumber(inner, number) => inner.signal()? + *number,
            Self::NumberMinus(number, inner) => *number - &inner.signal()?,
            Self::TimesNumber(inner, number) => &inner.signal()? * *number,
        })
    }

    fn value(&self, time: f64) -> f64 {
        match self {
            Self::Number(number) => *number,
            Self::Time => time.max(0.0),
            Self::Add(first, second) => first.value(time) + second.value(time),
            Self::Subtract(first, second) => first.value(time) - second.value(time),
            Self::Multiply(first, second) => first.value(time) * second.value(time),
            Self::Larger(first, second) => first.value(time).max(second.value(time)),
            Self::Smaller(first, second) => first.value(time).min(second.value(time)),
            Self::Negate(inner) => -inner.value(time),
            Self::PlusNumber(inner, number) => inner.value(time) + number,
            Self::NumberMinus(number, inner) => number - inner.value(time),
            Self::TimesNumber(inner, number) => inner.value(time) * number,
        }
    }
}

fn expressions() -> impl Strategy<Value = Expression> {
    let leaf = prop_oneof![(-100.0..100.0_f64).prop_map(Expression::Number), Just(Expression::Time)];
    leaf.prop_recursive(4, 32, 2, |inner| {
        let number = -100.0..100.0_f64;
        prop_oneof![
            (inner.clone(), inner.clone()).prop_map(|(a, b)| Expression::Add(Box::new(a), Box::new(b))),
            (inner.clone(), inner.clone()).prop_map(|(a, b)| Expression::Subtract(Box::new(a), Box::new(b))),
            (inner.clone(), inner.clone()).prop_map(|(a, b)| Expression::Multiply(Box::new(a), Box::new(b))),
            (inner.clone(), inner.clone()).prop_map(|(a, b)| Expression::Larger(Box::new(a), Box::new(b))),
            (inner.clone(), inner.clone()).prop_map(|(a, b)| Expression::Smaller(Box::new(a), Box::new(b))),
            inner.clone().prop_map(|a| Expression::Negate(Box::new(a))),
            (inner.clone(), number.clone()).prop_map(|(a, n)| Expression::PlusNumber(Box::new(a), n)),
            (number.clone(), inner.clone()).prop_map(|(n, a)| Expression::NumberMinus(n, Box::new(a))),
            (inner, number).prop_map(|(a, n)| Expression::TimesNumber(Box::new(a), n)),
        ]
    })
}

/// The test case's failure for an error that should not have happened.
fn failed(error: &Error) -> TestCaseError {
    TestCaseError::fail(error.to_string())
}

proptest! {
    #[test]
    fn a_graph_of_operators_gives_what_arithmetic_on_its_values_gives(
        expression in expressions(),
        times in prop::collection::vec(-10.0..1e3_f64, 0..20),
    ) {
        let got = expression.signal().and_then(|signal| signal.sample(&times)).map_err(|error| failed(&error))?;
        let expected: Vec<f64> = times.iter().map(|&time| expression.value(time)).collect();
        prop_assert_eq!(got, expected);
    }

    #[test]
    fn a_graph_gives_each_time_its_value_whatever_the_batches_or_their_order(
        expression in expressions(),
        times in prop::collection::vec(-10.0..1e3_f64, 0..30),
        cut in 0..30_usize,
    ) {
        // the graph over a series too, so every kind of node is read in pieces
        let signal = expression.signal().map_err(|error| failed(&error))?
            * steps().map_err(|error| failed(&error))?
            + ramp().map_err(|error| failed(&error))?;
        let together = signal.sample(&times).map_err(|error| failed(&error))?;
        let alone: Vec<f64> = times.iter().map(|&time| signal.at(time)).collect::<Result<_, _>>().map_err(|error| failed(&error))?;
        let (front, back) = times.split_at(cut.min(times.len()));
        let mut halves = signal.sample(back).map_err(|error| failed(&error))?;
        halves.splice(0..0, signal.sample(front).map_err(|error| failed(&error))?);
        let mut reversed = times.clone();
        reversed.reverse();
        let mut backwards = signal.sample(&reversed).map_err(|error| failed(&error))?;
        backwards.reverse();
        prop_assert_eq!(&together, &alone);
        prop_assert_eq!(&together, &halves);
        prop_assert_eq!(&together, &backwards);
    }
}

/// `depth` nodes deep: a constant 0 with 1 added `depth - 1` times, `alternate`ly as a number and as a signal.
fn chain(depth: usize, alternate: bool) -> Result<Signal, Error> {
    let one = Signal::constant(1.0)?;
    let mut sum = Signal::constant(0.0)?;
    for step in 1..depth {
        sum = if alternate && step % 2 == 0 {
            &one + sum
        } else {
            sum + 1.0
        };
    }
    Ok(sum)
}

#[test]
fn a_graph_as_deep_as_the_limit_reads_and_one_deeper_is_refused() -> Result<(), Error> {
    let deepest = chain(MAX_DEPTH, false)?;
    // 0 plus 1 for each node above the constant
    assert_eq!(deepest.at(5.0)?, 999.0);
    assert_eq!(chain(MAX_DEPTH, true)?.sample(&[0.0, 1.0])?, [999.0, 999.0]);
    let too_deep = format!("at most {MAX_DEPTH} nodes deep (this one would be {})", MAX_DEPTH + 1);
    for (signal, rule) in [
        (&deepest + 1.0, "a number"),
        (1.0 - &deepest, "a number first"),
        (&deepest * &deepest, "two signals"),
        (-&deepest, "negation"),
        (maximum(&Signal::constant(0.0)?, &deepest), "the larger of two"),
    ] {
        let message = depth_error(&signal.sample(&[0.0]));
        assert!(message.contains(&too_deep), "{rule}: {message}");
    }
    // the refusal comes through whatever is built on it
    let message = depth_error(&((&deepest + 1.0) * 2.0 + &echo()).sample(&[0.0]));
    assert!(message.contains(&too_deep), "{message}");
    Ok(())
}

#[test]
fn a_sum_of_a_hundred_thousand_terms_is_refused_and_dropped_without_overflowing_the_stack() -> Result<(), Error> {
    let term = Signal::constant(0.5)?;
    let mut sum = term.clone();
    for _ in 1..100_000 {
        sum = sum + &term;
    }
    let message = depth_error(&sum.sample(&[0.0, 1.0]));
    assert!(message.contains("nodes deep"), "{message}");
    drop(sum);
    // the deepest graph there can be, dropped as its last handle goes
    drop(chain(MAX_DEPTH, true)?);
    Ok(())
}

#[test]
fn a_deep_graph_is_debugged_without_walking_it() -> Result<(), Error> {
    let text = format!("{:?}", chain(MAX_DEPTH, false)?);
    assert!(text.contains("Add") && text.contains("1000"), "{text}");
    Ok(())
}

#[test]
fn every_way_of_making_a_signal_says_how_it_was_made() -> Result<(), Error> {
    use crate::Origin;
    use crate::builtin::slew;

    let level = Signal::constant(0.5)?;
    let steps = Signal::from_values(&[0.0, 1.0], 10.0, FromValues::default(), Interpolation::Hold)?;
    let slewed = steps.slew(slew::Params::default())?;
    let rendered = Signal::rendered(&level, crate::Rendering::default(), |inputs| {
        Ok::<_, Unreachable>(inputs.to_vec())
    })?;
    // (how it was made, the signal, how it says it was made)
    let rows: [(&str, Signal, &str); 13] = [
        ("a constant", level.clone(), "values"),
        ("from values", steps.clone(), "values"),
        ("from a series", ramp()?, "values"),
        ("a callback", echo(), "callback"),
        ("a recurrence run over a signal", rendered, "rendered"),
        ("a sum", &level + &steps, "operator"),
        ("a difference with a number", 1.0 - &level, "operator"),
        ("a product", &level * 2.0, "operator"),
        ("a negation", -&level, "operator"),
        ("the larger of two", maximum(&level, &steps), "operator"),
        ("the smaller of two", minimum(&level, &steps), "operator"),
        ("an operator that cannot be read", &level + f64::NAN, "operator"),
        ("a thing of the registry", slewed.clone(), "thing"),
    ];
    for (row, signal, expected) in rows {
        let made = match signal.origin() {
            Origin::Thing(_) => "thing",
            Origin::Operator => "operator",
            Origin::Values => "values",
            Origin::Callback => "callback",
            Origin::Rendered => "rendered",
        };
        assert_eq!(made, expected, "{row}");
    }
    // a thing's origin holds its recipe, the one `recipe` gives
    let recipe = match slewed.origin() {
        Origin::Thing(recipe) => Some(recipe.name.as_str()),
        _ => None,
    };
    assert_eq!((recipe, slewed.recipe().map(|recipe| recipe.name.as_str())), (Some("slew"), Some("slew")));
    Ok(())
}
