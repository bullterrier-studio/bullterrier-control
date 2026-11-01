//! The core's [`Signal`] for Python: `SignalBase`, the class a Python signal inherits, and what makes and combines
//! signals (`Constant`, `Rendered`, `from_series`, `from_values`, `minimum`, `maximum`, the operators, `shape`).
//!
//! A `SignalBase` either holds a Rust signal (one the extension made) or is a Python subclass implementing
//! `compute(times)`, read through [`callback::computed`]. A signal the extension makes is an instance of the class
//! `make_signals_as` named (the package's `Signal`, so it has the package's methods), or of `SignalBase` until then;
//! `Constant` and `Rendered` are classes of their own, which the package's classes of those names inherit.
//!
//! A subclass holds no Rust signal of itself: one is made for each use, so a signal made from it (`self + 1`) keeps
//! it alive without it keeping that signal alive in turn.

use std::sync::OnceLock;

use bullterrier_control::{Error, Interpolation, Rendering, Signal};
use pyo3::exceptions::{PyOverflowError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::pyclass_init::PyClassInitializer;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyBool, PyDict, PyFloat, PyInt, PyTuple, PyType};

use crate::callback;
use crate::data::{float64s, shaped, shown};
use crate::error;
use crate::formats::PySeries;
use crate::params;

/// A value that changes over time, readable at any moment in seconds: the base of every signal. A subclass implements
/// `compute(times)`; its `__init__` may take any arguments and need not call `super().__init__()`. A class inheriting
/// `Constant` or `Rendered` lists it first in its bases (else making one is a `TypeError`) and takes its own arguments
/// through `__new__`, which calls `super().__new__(cls, ...)`: their native `__new__` makes the signal, so an `__init__`
/// with other arguments is never reached.
///
/// `sample(times)` takes a number or an array of any shape and returns float64 values of its shape (0-d for a
/// number), reading a time before 0 as 0. A time that is not a finite number (text, a bool, NaN, an infinity, an
/// integer too large for an array of numbers) and a value that is not finite are refused with `SignalError`. The
/// same times give the same values, whatever was read before. A read of a signal made in Rust runs without the GIL.
///
/// A read calls a subclass's `compute(times)` with a 1-D float64 array of finite times from 0 up, of its own, and
/// takes as many values (anything numpy makes float64 of: another shape is a `ValueError`); an exception `compute`
/// raises reaches the caller of `sample` unchanged. A signal made in Rust (an operator's result, a `Constant`, a
/// `Rendered`, ...) is read in Rust, a `compute` of a subclass of its class aside; its own `compute` gives the values
/// at times of any shape without refusing one that is not finite (the read that called it does).
///
/// Signals combine with `+`, `-`, `*` and negation, a number (a real one, numpy's too, not a bool) standing for a
/// constant; a number that is not finite is refused at once with `SignalError`. A graph deeper than the core allows
/// (1000 nodes) is refused when read.
#[pyclass(subclass, frozen, module = "bullterrier_control._native._core", name = "SignalBase")]
pub(crate) struct SignalBase {
    /// The Rust signal of one the extension made; empty for a Python subclass, which is read through its `compute`.
    handle: OnceLock<Signal>,
}

impl SignalBase {
    /// The base holding `signal`.
    fn holding(signal: Signal) -> Self {
        Self {
            handle: OnceLock::from(signal),
        }
    }
}

/// The class `make_signals_as` named: what the extension makes its signals as.
static MADE: PyOnceLock<Py<PyType>> = PyOnceLock::new();

/// `numbers.Real`, imported once: the numbers that stand for a constant signal.
static REAL: PyOnceLock<Py<PyType>> = PyOnceLock::new();

#[pymethods]
impl SignalBase {
    // A class listing `Constant` or `Rendered` after another subclass of this one is made with this `__new__` (the
    // first base's), never theirs: it would get no signal and swallow their arguments, so it is refused.
    #[new]
    #[classmethod]
    #[pyo3(signature = (*_args, **_kwargs), text_signature = "(*args, **kwargs)")]
    fn new(cls: &Bound<'_, PyType>, _args: &Bound<'_, PyTuple>, _kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        for (skipped, name) in [
            (cls.is_subclass_of::<Constant>()?, "Constant"),
            (cls.is_subclass_of::<Rendered>()?, "Rendered"),
        ] {
            if skipped {
                return Err(PyTypeError::new_err(format!(
                    "{} inherits {name} after another base, which makes it without {name}'s arguments: list {name} \
                     first in its bases",
                    cls.name()?
                )));
            }
        }
        Ok(Self {
            handle: OnceLock::new(),
        })
    }

    /// The values at `times` (a number, or an array of any shape; the result has its shape).
    fn sample<'py>(slf: &Bound<'py, Self>, times: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        read(&signal(slf), times, Signal::sample_into)
    }

    /// The values at `times`: finite, from 0 up, float64. A subclass implements it.
    fn compute<'py>(slf: &Bound<'py, Self>, times: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        match slf.get().handle.get() {
            Some(signal) => read(signal, times, Signal::read_into),
            None => Err(PyTypeError::new_err(format!(
                "{} must implement compute(times)",
                slf.get_type().name()?
            ))),
        }
    }

    // numpy defers to the operators below: an array times a signal is an error, not an array of signals
    #[classattr]
    fn __array_ufunc__(py: Python<'_>) -> Py<PyAny> {
        py.None()
    }

    fn __add__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        combined(slf.as_any(), other, |first, second| first + second)
    }

    fn __radd__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        combined(other, slf.as_any(), |first, second| first + second)
    }

    fn __sub__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        combined(slf.as_any(), other, |first, second| first - second)
    }

    fn __rsub__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        combined(other, slf.as_any(), |first, second| first - second)
    }

    fn __mul__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        combined(slf.as_any(), other, |first, second| first * second)
    }

    fn __rmul__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        combined(other, slf.as_any(), |first, second| first * second)
    }

    fn __neg__<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        made(slf.py(), -signal(slf))
    }
}

/// The same finite value at every time.
#[pyclass(extends = SignalBase, subclass, frozen, module = "bullterrier_control._native._core", name = "Constant")]
pub(crate) struct Constant;

#[pymethods]
impl Constant {
    /// A signal that is `value` everywhere: anything `float()` takes, giving a finite number (else `SignalError`).
    #[new]
    fn new(value: &Bound<'_, PyAny>) -> PyResult<PyClassInitializer<Self>> {
        let signal = constant(value)?;
        Ok(PyClassInitializer::from(SignalBase::holding(signal)).add_subclass(Self))
    }
}

/// A signal that needs history (a smoother, a slew limiter): a causal recurrence `run` over `signal` sampled at `rate`
/// samples per second from time 0, rendered as far as the reads need and kept, read by holding the latest sample at
/// or before each time.
///
/// `run` gets a fresh 1-D, C-contiguous float64 array of its own (whatever the input's `compute` returned) and returns
/// as many outputs (another shape is a `ValueError`). It must be causal (output *i* depends on inputs 0 to *i* only):
/// then the same time gives the same value whatever was read before, and a read at time t depends on the input only
/// up to t (a float rounding of a billionth of a sample aside). The signal renders at least 1024 samples and twice what
/// it holds, at most `limit`; a read needing more than `limit` samples is a `SignalError` before anything is rendered.
/// Several threads may read it at once.
///
/// A `rate` that is not a positive finite number and a `limit` that is not a whole number from 1 are `ValueError`s.
#[pyclass(extends = SignalBase, subclass, frozen, module = "bullterrier_control._native._core", name = "Rendered")]
pub(crate) struct Rendered;

#[pymethods]
impl Rendered {
    #[new]
    #[pyo3(signature = (signal, *, rate, run, limit = None), text_signature = "(signal, *, rate, run, limit=20000000)")]
    fn new(
        signal: &Bound<'_, SignalBase>,
        rate: &Bound<'_, PyAny>,
        run: Py<PyAny>,
        limit: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let py = signal.py();
        let rate = match rate.cast::<PyBool>() {
            Ok(_) => Err(PyValueError::new_err(format!(
                "rate must be a positive finite number of samples per second (got {})",
                shown(rate)
            ))),
            Err(_) => rate.extract::<f64>(),
        }?;
        let limit = match limit {
            None => Rendering::default().limit,
            Some(limit) => whole(limit).ok_or_else(|| {
                PyValueError::new_err(format!(
                    "limit must be a whole number of samples, at least 1 (got {})",
                    shown(limit)
                ))
            })?,
        };
        let rendered = callback::rendered(&self::signal(signal), Rendering { rate, limit }, run).map_err(|error| {
            match error {
                // a wrong argument to `Rendered` is a bug of its caller, not a signal that cannot be read
                Error::Signal { message } => PyValueError::new_err(message),
                other => error::raised(py, other),
            }
        })?;
        Ok(PyClassInitializer::from(SignalBase::holding(rendered)).add_subclass(Self))
    }
}

/// `limit` as a number of samples: a Python `int` (not a `bool`) that fits; 0 is the core's to refuse.
fn whole(limit: &Bound<'_, PyAny>) -> Option<usize> {
    if limit.cast::<PyBool>().is_ok() {
        return None;
    }
    limit.cast::<PyInt>().ok()?.extract::<usize>().ok()
}

/// The signal that reads `series`: sample *i* is at `start + i / rate` seconds, the first and last values hold beyond
/// the ends, and between samples the value is interpolated `linear`ly or `hold`s the earlier sample's.
#[pyfunction]
#[pyo3(signature = (series, *, interpolation = "linear"))]
pub(crate) fn from_series<'py>(series: &Bound<'py, PySeries>, interpolation: &str) -> PyResult<Bound<'py, PyAny>> {
    made(
        series.py(),
        Signal::from_series(series.get().series(), interpolated(interpolation)?),
    )
}

/// The signal that reads `values` as plain ratios, sample *i* at `start + i / rate` seconds: `from_series` of the
/// `Series(rate=rate, values=values, unit="ratio", start=start)`, whose `FormatError` names each rule broken.
#[pyfunction]
#[pyo3(signature = (values, rate, *, start = None, interpolation = "linear"), text_signature = "(values, rate, *, start=0.0, interpolation='linear')")]
pub(crate) fn from_values<'py>(
    values: &Bound<'py, PyAny>,
    rate: &Bound<'py, PyAny>,
    start: Option<&Bound<'py, PyAny>>,
    interpolation: &str,
) -> PyResult<Bound<'py, PyAny>> {
    let py = values.py();
    let fields = PyDict::new(py);
    fields.set_item("rate", rate)?;
    fields.set_item("values", values)?;
    fields.set_item("unit", "ratio")?;
    if let Some(start) = start {
        fields.set_item("start", start)?;
    }
    let series = py.get_type::<PySeries>().call((), Some(&fields))?;
    from_series(series.cast::<PySeries>()?, interpolation)
}

/// The larger of two signals (or numbers) at every time.
#[pyfunction]
pub(crate) fn maximum<'py>(first: &Bound<'py, PyAny>, second: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    pair(first, second, bullterrier_control::maximum)
}

/// The smaller of two signals (or numbers) at every time.
#[pyfunction]
pub(crate) fn minimum<'py>(first: &Bound<'py, PyAny>, second: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    pair(first, second, bullterrier_control::minimum)
}

/// The signal the shaper of the Rust core called `name` makes of `signal` with `params` (its parameters by name; one
/// left out takes its default). A `ParameterError` lists every rule `params` breaks; an unknown name is a
/// `ValueError`.
#[pyfunction]
pub(crate) fn shape<'py>(
    name: &str,
    signal: &Bound<'py, SignalBase>,
    params: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let py = signal.py();
    let params = params::json(params).map_err(PyValueError::new_err)?;
    let shaped =
        bullterrier_control::shape(name, &self::signal(signal), &params).map_err(|error| error::raised(py, error))?;
    made(py, shaped)
}

/// Make every signal the extension makes from now on (an operator's result, `from_series`, `shape`, ...) an instance
/// of `cls`: a subclass of `SignalBase` that does not implement `compute` and can be made by `cls.__new__(cls)` alone
/// (so not one with a `__new__` needing arguments, nor a subclass of `Constant` or `Rendered`: `TypeError`). Once per
/// process: naming another class later is a `ValueError`.
#[pyfunction]
pub(crate) fn make_signals_as(cls: &Bound<'_, PyType>) -> PyResult<()> {
    let py = cls.py();
    let base = py.get_type::<SignalBase>();
    if !cls.is_subclass(&base)? || !cls.getattr("compute")?.is(&base.getattr("compute")?) {
        return Err(PyTypeError::new_err(format!(
            "signals are made as a subclass of SignalBase that does not implement compute (got {})",
            cls.name()?
        )));
    }
    if !cls
        .call_method1("__new__", (cls,))
        .is_ok_and(|instance| instance.is_instance_of::<SignalBase>())
    {
        return Err(PyTypeError::new_err(format!(
            "{} cannot be made without arguments, as signals are made",
            cls.name()?
        )));
    }
    let named = MADE.get_or_init(py, || cls.clone().unbind());
    if named.bind(py).is(cls) {
        Ok(())
    } else {
        Err(PyValueError::new_err(format!(
            "signals are already made as {}",
            named.bind(py).name()?
        )))
    }
}

/// The Rust signal `signal` is: the one it holds, or a callback node reading its `compute`.
pub(crate) fn signal(signal: &Bound<'_, SignalBase>) -> Signal {
    match signal.get().handle.get() {
        Some(held) => held.clone(),
        None => callback::computed(signal.clone().into_any().unbind()),
    }
}

/// `signal` as a Python object: an instance of the class `make_signals_as` named, or of `SignalBase`.
pub(crate) fn made(py: Python<'_>, signal: Signal) -> PyResult<Bound<'_, PyAny>> {
    let Some(class) = MADE.get(py) else {
        return Ok(Bound::new(py, SignalBase::holding(signal))?.into_any());
    };
    let class = class.bind(py);
    let instance = class.call_method1("__new__", (class,))?;
    instance.cast::<SignalBase>()?.get().handle.set(signal).map_err(|_| {
        PyTypeError::new_err(format!(
            "{}.__new__ gave a signal it had made",
            class.name().map_or_else(|_| "?".to_owned(), |name| name.to_string())
        ))
    })?;
    Ok(instance)
}

/// A way to read a signal into a buffer: [`Signal::sample_into`] or [`Signal::read_into`].
type Read = fn(&Signal, &[f64], &mut [f64]) -> Result<(), Error>;

/// What `how` reads of `signal` at `times`, in the shape of `times`, read without the GIL.
fn read<'py>(signal: &Signal, times: &Bound<'py, PyAny>, how: Read) -> PyResult<Bound<'py, PyAny>> {
    let py = times.py();
    let numpy = py.import("numpy")?;
    let raw = numpy.call_method1("asarray", (times,))?;
    let kind: String = raw.getattr("dtype")?.getattr("kind")?.extract()?;
    // not text, a bool, nothing, or an integer too large for a float (numpy holds that as an object)
    if !matches!(kind.as_str(), "i" | "u" | "f") {
        return Err(error::raised(
            py,
            Error::Signal {
                message: format!("times must be numbers (got {})", shown(times)),
            },
        ));
    }
    let (moments, shape) = float64s(&raw)?;
    let mut values = vec![0.0; moments.len()];
    py.detach(|| how(signal, &moments, &mut values))
        .map_err(|error| error::raised(py, error))?;
    shaped(py, values, &shape)
}

/// `first` and `second` combined by `operation`, or `NotImplemented` (for Python to raise its `TypeError`) if either
/// is neither a signal nor a number.
fn combined(
    first: &Bound<'_, PyAny>,
    second: &Bound<'_, PyAny>,
    operation: impl FnOnce(&Signal, &Signal) -> Signal,
) -> PyResult<Py<PyAny>> {
    let py = first.py();
    match (operand(first)?, operand(second)?) {
        (Some(first), Some(second)) => Ok(made(py, operation(&first, &second))?.unbind()),
        _ => Ok(py.NotImplemented()),
    }
}

/// `first` and `second` combined by `operation`, or a `TypeError` if either is neither a signal nor a number.
fn pair<'py>(
    first: &Bound<'py, PyAny>,
    second: &Bound<'py, PyAny>,
    operation: fn(&Signal, &Signal) -> Signal,
) -> PyResult<Bound<'py, PyAny>> {
    match (operand(first)?, operand(second)?) {
        (Some(one), Some(other)) => made(first.py(), operation(&one, &other)),
        _ => Err(PyTypeError::new_err("a signal combines with signals and numbers only")),
    }
}

/// `value` as a signal: a signal is itself, a real number (numpy's included, a bool not) a constant (`SignalError` if
/// it is not finite); anything else is not an operand.
fn operand(value: &Bound<'_, PyAny>) -> PyResult<Option<Signal>> {
    if let Ok(signal) = value.cast::<SignalBase>() {
        return Ok(Some(self::signal(signal)));
    }
    let real = REAL.import(value.py(), "numbers", "Real")?;
    if value.cast::<PyBool>().is_err() && value.is_instance(real)? {
        return Ok(Some(constant(value)?));
    }
    Ok(None)
}

/// `float(value)`, an integer too large for a float being an infinity.
fn float(value: &Bound<'_, PyAny>) -> PyResult<f64> {
    let py = value.py();
    match py.get_type::<PyFloat>().call1((value,)) {
        Ok(number) => number.extract(),
        Err(error) if error.is_instance_of::<PyOverflowError>(py) => Ok(f64::INFINITY),
        Err(error) => Err(error),
    }
}

/// The constant signal `value` is (as `float()` reads it), or a `SignalError` if that is not finite.
fn constant(value: &Bound<'_, PyAny>) -> PyResult<Signal> {
    Signal::constant(float(value)?).map_err(|error| error::raised(value.py(), error))
}

/// The interpolation called `name`.
fn interpolated(name: &str) -> PyResult<Interpolation> {
    match name {
        "linear" => Ok(Interpolation::Linear),
        "hold" => Ok(Interpolation::Hold),
        other => Err(PyValueError::new_err(format!(
            "interpolation must be 'linear' or 'hold' (got {other:?})"
        ))),
    }
}
