//! The core's formats as the package's classes: `Unit`, `Event`, `Series`, `Grid` and `Bundle`, with `bundle_schema`
//! and `dumps`. Each class wraps the core's type and adds what Python needs: building from keywords or a mapping
//! (`parse`) through the core's reader, so every rule is the core's; values for `==`, `hash`, `repr`, `copy` and
//! `pickle`; and no way to change one once made (frozen, a read-only `values` array, read-only mappings).
//!
//! Concurrency: the classes are frozen and hold no lock and nothing mutable; the one section that runs without the GIL
//! (a grid's conversion) reads only data the grid owns and a copy of its input.

use std::collections::BTreeMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use numpy::{PyArray1, PyArrayDyn, PyArrayMethods};
use pyo3::IntoPyObjectExt;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyBytes, PyDict, PyList, PyMapping, PyString, PyTuple, PyType};
use serde::Serialize;

use bullterrier_control::formats::{
    BUNDLE_FORMAT, BUNDLE_VERSION, Bundle, BundleOptions, Event, Grid, Problem, Problems, Series, Unit,
};

use crate::data::{Data, NOT_UTF8, Numbers, numbers};
use crate::error::{format_error, raise};

/// `text` (JSON) as Python data: dicts, lists, numbers and text.
fn loads<'py>(py: Python<'py>, text: &str) -> PyResult<Bound<'py, PyAny>> {
    static LOADS: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    LOADS.import(py, "json", "loads")?.call1((text,))
}

/// `value` as the plain data a file holds for it (`{"t": 0.5, "strength": 1.0}`).
fn plain<'py, T: Serialize>(py: Python<'py>, value: &T) -> PyResult<Bound<'py, PyAny>> {
    // every number in a format is finite and every key text, so writing it cannot fail
    let text = serde_json::to_string(value).map_err(|error| PyValueError::new_err(error.to_string()))?;
    loads(py, &text)
}

/// A read-only view of `dict`.
fn read_only(dict: Bound<'_, PyDict>) -> PyResult<Bound<'_, PyAny>> {
    static PROXY: PyOnceLock<Py<PyType>> = PyOnceLock::new();
    PROXY.import(dict.py(), "types", "MappingProxyType")?.call1((dict,))
}

/// `value` as Python shows it.
fn shown<'py>(py: Python<'py>, value: impl IntoPyObject<'py>) -> PyResult<String> {
    Ok(value.into_bound_py_any(py)?.repr()?.to_string())
}

/// `values` as a list a repr can show: all of a few, else the first three and the last two around `...`.
fn listed(py: Python<'_>, values: &[f64]) -> PyResult<String> {
    let shown_all = values.len() <= 6;
    let mut parts = Vec::new();
    for (index, &value) in values.iter().enumerate() {
        if shown_all || index < 3 || index + 2 >= values.len() {
            parts.push(shown(py, value)?);
        } else if index == 3 {
            parts.push("...".to_owned());
        }
    }
    Ok(format!("[{}]", parts.join(", ")))
}

/// The one-dimensional numbers of the array-like `value`, for the field `field` of `about`, as a Python list.
fn number_list<'py>(about: &str, field: &str, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyList>> {
    let py = value.py();
    let Numbers { values, shape } = numbers(field, value).map_err(|problem| raise(py, about, &[problem]))?;
    if shape.len() != 1 {
        let rule = if shape.is_empty() {
            "must be a list of numbers, not a single number".to_owned()
        } else {
            format!("must be a list of numbers, not an array of {} dimensions", shape.len())
        };
        let problem = Problem {
            path: field.to_owned(),
            rule,
        };
        return Err(raise(py, about, &[problem]));
    }
    PyList::new(py, values)
}

/// A dict of the keyword arguments given (`None` is left out, as a file leaves out an optional field).
fn fields<'py>(py: Python<'py>, given: &[(&str, Option<Bound<'py, PyAny>>)]) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    for (name, value) in given {
        if let Some(value) = value {
            dict.set_item(*name, value)?;
        }
    }
    Ok(dict)
}

// ---- hashing: equal values hash equally (0.0 and -0.0 are equal); every number of a format is finite ----

/// Feed `value` to `hasher` so that `0.0` and `-0.0` feed the same.
fn feed(hasher: &mut DefaultHasher, value: f64) {
    // adding 0.0 turns -0.0 into 0.0 and leaves every other finite number as it is
    (value + 0.0).to_bits().hash(hasher);
}

fn feed_option(hasher: &mut DefaultHasher, value: Option<f64>) {
    value.is_some().hash(hasher);
    if let Some(value) = value {
        feed(hasher, value);
    }
}

fn feed_event(hasher: &mut DefaultHasher, event: &Event) {
    feed(hasher, event.t());
    feed(hasher, event.strength());
    feed_option(hasher, event.duration());
    event.label().hash(hasher);
    feed_option(hasher, event.pitch());
}

fn feed_series(hasher: &mut DefaultHasher, series: &Series) {
    feed(hasher, series.start());
    feed(hasher, series.rate());
    series.values().len().hash(hasher);
    for &value in series.values() {
        feed(hasher, value);
    }
    series.unit().hash(hasher);
    series.scaling().hash(hasher);
}

fn feed_grid(hasher: &mut DefaultHasher, grid: &Grid) {
    grid.beats().len().hash(hasher);
    for &beat in grid.beats() {
        feed(hasher, beat);
    }
    grid.meter().hash(hasher);
    grid.first_downbeat().hash(hasher);
}

fn feed_bundle(hasher: &mut DefaultHasher, bundle: &Bundle) {
    bundle.source().hash(hasher);
    feed_option(hasher, bundle.duration());
    bundle.grid().is_some().hash(hasher);
    if let Some(grid) = bundle.grid() {
        feed_grid(hasher, grid);
    }
    bundle.series().len().hash(hasher);
    for (name, series) in bundle.series() {
        name.hash(hasher);
        feed_series(hasher, series);
    }
    bundle.events().len().hash(hasher);
    for (name, events) in bundle.events() {
        name.hash(hasher);
        events.len().hash(hasher);
        for event in events {
            feed_event(hasher, event);
        }
    }
}

/// The hash of `value` as `feed` makes it.
fn hashed<T: ?Sized>(value: &T, feed: fn(&mut DefaultHasher, &T)) -> u64 {
    let mut hasher = DefaultHasher::new();
    feed(&mut hasher, value);
    hasher.finish()
}

// ---- Unit ----

/// What a Series' values measure: `Unit.FRACTION` (values in 0..1; the series says how that was reached),
/// `Unit.DECIBELS`, `Unit.HERTZ`, `Unit.MIDI`, `Unit.RATIO`, `Unit.SECONDS` or `Unit.BEATS`. Its `value` and `str()` are
/// the name a file spells it with (`"ratio"`); a `Series` takes a unit or that name.
#[pyclass(frozen, eq, hash, module = "bullterrier_control", name = "Unit")]
#[derive(PartialEq, Hash)]
pub(crate) struct PyUnit(pub(crate) Unit);

#[pymethods]
impl PyUnit {
    /// The name a file spells the unit with: `"ratio"`.
    #[getter]
    const fn value(&self) -> &'static str {
        self.0.as_str()
    }

    /// The member's name: `"RATIO"`.
    #[getter]
    fn name(&self) -> String {
        self.0.as_str().to_uppercase()
    }

    fn __str__(&self) -> &'static str {
        self.0.as_str()
    }

    fn __repr__(&self) -> String {
        format!("Unit.{}", self.name())
    }

    /// A copy or an unpickled unit is the member itself.
    fn __reduce__<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let getattr = py.import("builtins")?.getattr("getattr")?;
        (getattr, (slf.get_type(), slf.get().name())).into_bound_py_any(py)
    }
}

/// Make every unit of the core a member of the class: `Unit.FRACTION`, ... (the core's list is the only one).
pub(crate) fn add_units(py: Python<'_>) -> PyResult<()> {
    let class = py.get_type::<PyUnit>();
    for unit in Unit::ALL {
        class.setattr(unit.as_str().to_uppercase(), PyUnit(unit))?;
    }
    Ok(())
}

/// The member of `Unit` for `unit`.
fn unit_member(py: Python<'_>, unit: Unit) -> PyResult<Bound<'_, PyAny>> {
    py.get_type::<PyUnit>().getattr(unit.as_str().to_uppercase())
}

// ---- Event ----

/// Something at `t` seconds: an onset, a beat, a note (with `duration` and `pitch`), or a section start.
///
/// `t >= 0`; `strength` in [0, 1] (default 1); `duration > 0` (absent: a moment); `label` not empty; `pitch` a MIDI
/// note number in [0, 127], fractions allowed. A broken rule is a `FormatError` listing every one.
#[pyclass(frozen, module = "bullterrier_control", name = "Event")]
pub(crate) struct PyEvent(pub(crate) Event);

impl PyEvent {
    fn read(data: Bound<'_, PyAny>) -> PyResult<Self> {
        let py = data.py();
        Event::from_deserializer(Data(data))
            .map(Self)
            .map_err(|error| format_error(py, "Event", &error))
    }
}

#[pymethods]
impl PyEvent {
    #[new]
    #[pyo3(
        signature = (t, *, strength = None, duration = None, label = None, pitch = None),
        text_signature = "(t, *, strength=1.0, duration=None, label=None, pitch=None)"
    )]
    fn new<'py>(
        py: Python<'py>,
        t: Bound<'py, PyAny>,
        strength: Option<Bound<'py, PyAny>>,
        duration: Option<Bound<'py, PyAny>>,
        label: Option<Bound<'py, PyAny>>,
        pitch: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Self> {
        let given = [
            ("t", Some(t)),
            ("strength", strength),
            ("duration", duration),
            ("label", label),
            ("pitch", pitch),
        ];
        Self::read(fields(py, &given)?.into_any())
    }

    /// The event in `data` (a decoded JSON object), held to the rules a file is: a `FormatError` lists every one
    /// broken.
    #[classmethod]
    fn parse(_class: &Bound<'_, PyType>, data: Bound<'_, PyAny>) -> PyResult<Self> {
        Self::read(data)
    }

    /// This event with the fields in `changes` set (`duration=None` removes the duration), checked again.
    #[pyo3(signature = (**changes))]
    fn replace(&self, py: Python<'_>, changes: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        let data = plain(py, &self.0)?.cast_into::<PyDict>()?;
        if let Some(changes) = changes {
            data.update(changes.as_mapping())?;
        }
        Self::read(data.into_any())
    }

    /// The event as a file holds it: `{"t": 0.5, "strength": 1.0}`, with each optional field only when set.
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        plain(py, &self.0)
    }

    /// When it happens, in seconds from the start of the material.
    #[getter]
    const fn t(&self) -> f64 {
        self.0.t()
    }

    /// How strong it is, in [0, 1].
    #[getter]
    const fn strength(&self) -> f64 {
        self.0.strength()
    }

    /// The length of the span in seconds, or None for a moment.
    #[getter]
    const fn duration(&self) -> Option<f64> {
        self.0.duration()
    }

    /// Its name, if it has one.
    #[getter]
    fn label(&self) -> Option<&str> {
        self.0.label()
    }

    /// Its MIDI note number, if it has one.
    #[getter]
    const fn pitch(&self) -> Option<f64> {
        self.0.pitch()
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.0 == other.0
    }

    fn __hash__(&self) -> u64 {
        hashed(&self.0, feed_event)
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let event = &self.0;
        let mut parts = vec![
            format!("t={}", shown(py, event.t())?),
            format!("strength={}", shown(py, event.strength())?),
        ];
        if let Some(duration) = event.duration() {
            parts.push(format!("duration={}", shown(py, duration)?));
        }
        if let Some(label) = event.label() {
            parts.push(format!("label={}", shown(py, label)?));
        }
        if let Some(pitch) = event.pitch() {
            parts.push(format!("pitch={}", shown(py, pitch)?));
        }
        Ok(format!("Event({})", parts.join(", ")))
    }

    fn __reduce__<'py>(slf: &Bound<'py, Self>) -> PyResult<(Bound<'py, PyAny>, (Bound<'py, PyAny>,))> {
        Ok((slf.get_type().getattr("parse")?, (slf.get().to_dict(slf.py())?,)))
    }
}

// ---- Series ----

/// Values at a fixed rate: sample *i* is at `start + i / rate` seconds.
///
/// `start >= 0` (default 0); `rate > 0`; at least one value, each finite; `unit` a `Unit` or its name. A `fraction`
/// series states its `scaling` (how 0..1 was reached) and keeps every value in [0, 1]. `values` is a read-only float64
/// array of its own. A broken rule is a `FormatError` listing every one.
#[pyclass(frozen, module = "bullterrier_control", name = "Series")]
pub(crate) struct PySeries {
    inner: Series,
    /// `inner`'s values as a read-only numpy array, made once; `values` hands out views of it.
    values: Py<PyArray1<f64>>,
}

impl PySeries {
    fn wrap(py: Python<'_>, inner: Series) -> PyResult<Self> {
        // over an immutable `bytes` copy: numpy cannot make such an array writeable, nor reach the memory another way
        let bytes: Vec<u8> = inner.values().iter().flat_map(|value| value.to_ne_bytes()).collect();
        let values = py
            .import("numpy")?
            .call_method1("frombuffer", (PyBytes::new(py, &bytes), "float64"))?
            .cast_into::<PyArray1<f64>>()?;
        Ok(Self {
            inner,
            values: values.unbind(),
        })
    }

    fn read(data: Bound<'_, PyAny>) -> PyResult<Self> {
        let py = data.py();
        let inner = Series::from_deserializer(Data(data)).map_err(|error| format_error(py, "Series", &error))?;
        Self::wrap(py, inner)
    }
}

#[pymethods]
impl PySeries {
    #[new]
    #[pyo3(
        signature = (*, rate, values, unit, start = None, scaling = None),
        text_signature = "(*, rate, values, unit, start=0.0, scaling=None)"
    )]
    fn new<'py>(
        py: Python<'py>,
        rate: Bound<'py, PyAny>,
        values: &Bound<'py, PyAny>,
        unit: Bound<'py, PyAny>,
        start: Option<Bound<'py, PyAny>>,
        scaling: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Self> {
        let values = number_list("Series", "values", values)?.into_any();
        let given = [
            ("start", start),
            ("rate", Some(rate)),
            ("values", Some(values)),
            ("unit", Some(unit)),
            ("scaling", scaling),
        ];
        Self::read(fields(py, &given)?.into_any())
    }

    /// The series in `data` (a decoded JSON object), held to the rules a file is: a `FormatError` lists every one
    /// broken.
    #[classmethod]
    fn parse(_class: &Bound<'_, PyType>, data: Bound<'_, PyAny>) -> PyResult<Self> {
        Self::read(data)
    }

    /// The series as a file holds it: `{"start": 0.0, "rate": 10.0, "values": [...], "unit": "ratio"}`, with
    /// `scaling` only when set.
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        plain(py, &self.inner)
    }

    /// When the first sample is, in seconds.
    #[getter]
    const fn start(&self) -> f64 {
        self.inner.start()
    }

    /// Samples per second.
    #[getter]
    const fn rate(&self) -> f64 {
        self.inner.rate()
    }

    /// The samples: a read-only float64 array over the series' own data. Each call returns a new array object over
    /// the same immutable bytes, so reassigning the shape or dtype of one changes nothing for any other.
    #[getter]
    fn values<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyArray1<f64>>> {
        Ok(self
            .values
            .bind(py)
            .call_method0("view")?
            .cast_into::<PyArray1<f64>>()?)
    }

    /// What the values measure.
    #[getter]
    fn unit<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        unit_member(py, self.inner.unit())
    }

    /// How 0..1 was reached, if said.
    #[getter]
    fn scaling(&self) -> Option<&str> {
        self.inner.scaling()
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }

    fn __hash__(&self) -> u64 {
        hashed(&self.inner, feed_series)
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let series = &self.inner;
        let mut parts = vec![
            format!("start={}", shown(py, series.start())?),
            format!("rate={}", shown(py, series.rate())?),
            format!("values={}", listed(py, series.values())?),
            format!("unit={}", PyUnit(series.unit()).__repr__()),
        ];
        if let Some(scaling) = series.scaling() {
            parts.push(format!("scaling={}", shown(py, scaling)?));
        }
        Ok(format!("Series({})", parts.join(", ")))
    }

    fn __reduce__<'py>(slf: &Bound<'py, Self>) -> PyResult<(Bound<'py, PyAny>, (Bound<'py, PyAny>,))> {
        Ok((slf.get_type().getattr("parse")?, (slf.get().to_dict(slf.py())?,)))
    }
}

// ---- Grid ----

/// The musical clock: the time of every beat in seconds (at least two, each `>= 0`, strictly increasing), `meter`
/// beats per bar (`>= 1`) and `first_downbeat`, the index of the beat that starts the first bar (below `meter` and the
/// number of beats). Tempo is derived from the beats, never stored beside them. A broken rule is a `FormatError`
/// listing every one.
///
/// `grid.seconds_at(2.5)` is the time of beat position 2.5 (halfway between the third and fourth beat);
/// `grid.beat_at(12.4)` the beat position at 12.4 s.
#[pyclass(frozen, module = "bullterrier_control", name = "Grid")]
pub(crate) struct PyGrid(pub(crate) Grid);

impl PyGrid {
    fn read(data: Bound<'_, PyAny>) -> PyResult<Self> {
        let py = data.py();
        Grid::from_deserializer(Data(data))
            .map(Self)
            .map_err(|error| format_error(py, "Grid", &error))
    }

    /// `convert` each number of the array-like `value` (named `field` in a message), into an array of its shape.
    fn convert<'py>(
        &self,
        field: &str,
        value: &Bound<'py, PyAny>,
        convert: fn(&Grid, &[f64]) -> Result<Vec<f64>, bullterrier_control::formats::FormatError>,
    ) -> PyResult<Bound<'py, PyArrayDyn<f64>>> {
        let py = value.py();
        let Numbers { values, shape } = numbers(field, value).map_err(|problem| raise(py, "", &[problem]))?;
        let grid = &self.0;
        let converted = py
            .detach(|| convert(grid, &values))
            .map_err(|error| format_error(py, "", &error))?;
        PyArray1::from_vec(py, converted).reshape(shape)
    }
}

#[pymethods]
impl PyGrid {
    #[new]
    #[pyo3(signature = (*, beats, meter, first_downbeat))]
    fn new<'py>(
        py: Python<'py>,
        beats: &Bound<'py, PyAny>,
        meter: Bound<'py, PyAny>,
        first_downbeat: Bound<'py, PyAny>,
    ) -> PyResult<Self> {
        let beats = number_list("Grid", "beats", beats)?.into_any();
        let given = [
            ("beats", Some(beats)),
            ("meter", Some(meter)),
            ("first_downbeat", Some(first_downbeat)),
        ];
        Self::read(fields(py, &given)?.into_any())
    }

    /// The grid in `data` (a decoded JSON object), held to the rules a file is: a `FormatError` lists every one broken.
    #[classmethod]
    fn parse(_class: &Bound<'_, PyType>, data: Bound<'_, PyAny>) -> PyResult<Self> {
        Self::read(data)
    }

    /// The grid as a file holds it: `{"beats": [...], "meter": 4, "first_downbeat": 0}`.
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        plain(py, &self.0)
    }

    /// The time of every beat, in seconds.
    #[getter]
    fn beats<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        PyTuple::new(py, self.0.beats())
    }

    /// Beats per bar.
    #[getter]
    const fn meter(&self) -> u32 {
        self.0.meter()
    }

    /// The index of the beat that starts the first bar.
    #[getter]
    const fn first_downbeat(&self) -> u32 {
        self.0.first_downbeat()
    }

    /// The time of the first beat of every bar: `beats[first_downbeat::meter]`.
    #[getter]
    fn downbeats<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        PyTuple::new(py, self.0.downbeats())
    }

    /// The time in seconds of each beat position in `positions` (a number or an array-like of any shape; the result,
    /// float64, has its shape).
    ///
    /// Position 0 is the first beat, 1 the second, and so on; a fraction is linear between two beats. Before the first
    /// beat and after the last, the first or last beat length continues, so every position has a time. A position that
    /// is not a finite number, or whose time is not (too far from the beats), is a `FormatError` naming it
    /// (`positions[2]`, counted row-major).
    fn seconds_at<'py>(&self, positions: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyArrayDyn<f64>>> {
        self.convert("positions", positions, Grid::seconds_at)
    }

    /// The beat position of each time in `seconds`: the inverse of `seconds_at`, with the same shape and refusals.
    fn beat_at<'py>(&self, seconds: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyArrayDyn<f64>>> {
        self.convert("seconds", seconds, Grid::beat_at)
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.0 == other.0
    }

    fn __hash__(&self) -> u64 {
        hashed(&self.0, feed_grid)
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        Ok(format!(
            "Grid(beats={}, meter={}, first_downbeat={})",
            listed(py, self.0.beats())?,
            self.0.meter(),
            self.0.first_downbeat()
        ))
    }

    fn __reduce__<'py>(slf: &Bound<'py, Self>) -> PyResult<(Bound<'py, PyAny>, (Bound<'py, PyAny>,))> {
        Ok((slf.get_type().getattr("parse")?, (slf.get().to_dict(slf.py())?,)))
    }
}

// ---- Bundle ----

/// Everything one producer measured of one piece of material: named Series, named Event lists, a Grid.
///
/// `format` is `"bullterrier-control/bundle"` and `version` 1; `duration > 0`, and with it every event is at or before
/// it and every series starts at or before it; names match `^[a-z0-9][a-z0-9_.-]*$`; each event list is sorted by `t`.
/// `series` and `events` are read-only mappings (events as tuples), in name order. A broken rule is a `FormatError`
/// listing every one, under its path (`series.bass.rate`, `events.kick[3].t`).
#[pyclass(frozen, module = "bullterrier_control", name = "Bundle")]
pub(crate) struct PyBundle {
    inner: Bundle,
    grid: Option<Py<PyGrid>>,
    /// name -> `Series`, read-only
    series: Py<PyAny>,
    /// name -> tuple of `Event`, read-only
    events: Py<PyAny>,
}

impl PyBundle {
    /// The bundle `inner` with its parts as Python objects: `grid`, and the series and events by name.
    fn assemble(
        py: Python<'_>,
        inner: Bundle,
        grid: Option<Py<PyGrid>>,
        series: BTreeMap<String, Py<PySeries>>,
        events: BTreeMap<String, Vec<Py<PyEvent>>>,
    ) -> PyResult<Self> {
        let series_dict = PyDict::new(py);
        for (name, found) in series {
            series_dict.set_item(name, found)?;
        }
        let events_dict = PyDict::new(py);
        for (name, found) in events {
            events_dict.set_item(name, PyTuple::new(py, found)?)?;
        }
        Ok(Self {
            inner,
            grid,
            series: read_only(series_dict)?.unbind(),
            events: read_only(events_dict)?.unbind(),
        })
    }

    /// The bundle `inner`, its parts made Python objects.
    fn wrap(py: Python<'_>, inner: Bundle) -> PyResult<Self> {
        let grid = inner.grid().map(|grid| Py::new(py, PyGrid(grid.clone()))).transpose()?;
        let series = inner
            .series()
            .iter()
            .map(|(name, found)| Ok((name.clone(), Py::new(py, PySeries::wrap(py, found.clone())?)?)))
            .collect::<PyResult<_>>()?;
        let events = inner
            .events()
            .iter()
            .map(|(name, found)| {
                let found = found
                    .iter()
                    .map(|event| Py::new(py, PyEvent(event.clone())))
                    .collect::<PyResult<_>>()?;
                Ok((name.clone(), found))
            })
            .collect::<PyResult<_>>()?;
        Self::assemble(py, inner, grid, series, events)
    }

    fn read(data: Bound<'_, PyAny>) -> PyResult<Self> {
        let py = data.py();
        let inner = Bundle::from_deserializer(Data(data)).map_err(|error| format_error(py, "Bundle", &error))?;
        Self::wrap(py, inner)
    }
}

/// `value`'s type, as a message names it: `dict`.
fn type_name(value: &Bound<'_, PyAny>) -> String {
    value
        .get_type()
        .name()
        .map_or_else(|_| "?".to_owned(), |name| name.to_string())
}

/// The (text key, value) pairs of the mapping `value` given for `field`, recording a problem for what is not.
fn named<'py>(
    field: &str,
    value: &Bound<'py, PyAny>,
    problems: &mut Problems,
) -> PyResult<Vec<(String, Bound<'py, PyAny>)>> {
    let Ok(mapping) = value.cast::<PyMapping>() else {
        problems.push(Problem {
            path: field.to_owned(),
            rule: format!("must be a mapping of names (got {})", type_name(value)),
        });
        return Ok(Vec::new());
    };
    let mut found = Vec::new();
    for pair in mapping.items()?.iter() {
        let (key, item) = pair.extract::<(Bound<'py, PyAny>, Bound<'py, PyAny>)>()?;
        match key.cast::<PyString>().map(|key| key.to_str()) {
            Ok(Ok(name)) => found.push((name.to_owned(), item)),
            Ok(Err(_)) => problems.push(Problem {
                path: field.to_owned(),
                rule: NOT_UTF8.to_owned(),
            }),
            Err(_) => problems.push(Problem {
                path: field.to_owned(),
                rule: format!("a name must be text (got {})", shown(value.py(), key)?),
            }),
        }
    }
    Ok(found)
}

/// `value` as the class `T` (given at `path`), or the problem that it is not one.
fn instance<T: pyo3::PyClass + pyo3::PyTypeInfo>(
    path: &str,
    value: &Bound<'_, PyAny>,
    problems: &mut Problems,
) -> Option<Py<T>> {
    if let Ok(found) = value.cast::<T>() {
        return Some(found.clone().unbind());
    }
    problems.push(Problem {
        path: path.to_owned(),
        rule: format!("must be a {} (got {})", T::NAME, type_name(value)),
    });
    None
}

#[pymethods]
impl PyBundle {
    #[new]
    #[pyo3(signature = (*, format, version, source = None, duration = None, grid = None, series = None, events = None))]
    #[expect(clippy::too_many_arguments, reason = "the fields of the format, as keywords")]
    fn new<'py>(
        py: Python<'py>,
        format: Bound<'py, PyAny>,
        version: Bound<'py, PyAny>,
        source: Option<Bound<'py, PyAny>>,
        duration: Option<Bound<'py, PyAny>>,
        grid: Option<Bound<'py, PyAny>>,
        series: Option<Bound<'py, PyAny>>,
        events: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Self> {
        let mut problems = Problems::new();
        // the fields of the document's own, read as a file's are; the parts below are already checked objects
        let head = [
            ("format", Some(format)),
            ("version", Some(version)),
            ("source", source),
            ("duration", duration),
        ];
        let (source, duration) = match Bundle::from_deserializer(Data(fields(py, &head)?.into_any())) {
            Ok(read) => (read.source().map(str::to_owned), read.duration()),
            Err(error) => {
                problems.absorb(&error);
                (None, None)
            }
        };
        let grid = grid.and_then(|grid| instance::<PyGrid>("grid", &grid, &mut problems));
        let mut given_series = BTreeMap::new();
        if let Some(series) = series {
            for (name, item) in named("series", &series, &mut problems)? {
                if let Some(found) = instance::<PySeries>(&format!("series.{name}"), &item, &mut problems) {
                    given_series.insert(name, found);
                }
            }
        }
        let mut given_events = BTreeMap::new();
        if let Some(events) = events {
            for (name, list) in named("events", &events, &mut problems)? {
                let path = format!("events.{name}");
                if list.cast::<PyList>().is_err() && list.cast::<PyTuple>().is_err() {
                    problems.push(Problem {
                        path,
                        rule: format!("must be a list or tuple of Event (got {})", type_name(&list)),
                    });
                    continue;
                }
                let mut found = Vec::new();
                for (index, item) in list.try_iter()?.enumerate() {
                    if let Some(event) = instance::<PyEvent>(&format!("{path}[{index}]"), &item?, &mut problems) {
                        found.push(event);
                    }
                }
                given_events.insert(name, found);
            }
        }
        let options = BundleOptions {
            source,
            duration,
            grid: grid.as_ref().map(|grid| grid.get().0.clone()),
            series: given_series
                .iter()
                .map(|(name, found)| (name.clone(), found.get().inner.clone()))
                .collect(),
            events: given_events
                .iter()
                .map(|(name, found)| (name.clone(), found.iter().map(|event| event.get().0.clone()).collect()))
                .collect(),
        };
        let built = Bundle::new(options);
        if let Err(error) = &built {
            problems.absorb(error);
        }
        match (built, problems.into_error()) {
            (Ok(inner), None) => Self::assemble(py, inner, grid, given_series, given_events),
            (_, Some(error)) | (Err(error), None) => Err(format_error(py, "Bundle", &error)),
        }
    }

    /// The bundle in `data` (a decoded JSON object), held to the rules a file is: a `FormatError` lists every one
    /// broken.
    #[classmethod]
    fn parse(_class: &Bound<'_, PyType>, data: Bound<'_, PyAny>) -> PyResult<Self> {
        Self::read(data)
    }

    /// The bundle in the JSON text `text`: one object, no key repeated, no `NaN` or infinity, every rule of the format
    /// met. A `FormatError` names the one problem that stopped the reading, or every rule broken.
    #[classmethod]
    fn from_json(_class: &Bound<'_, PyType>, text: &Bound<'_, PyString>) -> PyResult<Self> {
        let py = text.py();
        let text = text.to_str().map_err(|_| {
            let problem = Problem {
                path: String::new(),
                rule: NOT_UTF8.to_owned(),
            };
            raise(py, "Bundle", &[problem])
        })?;
        let inner = Bundle::from_json(text).map_err(|error| format_error(py, "Bundle", &error))?;
        Self::wrap(py, inner)
    }

    /// The bundle as a file holds it: `{"format": ..., "version": 1, "series": {...}, "events": {...}}`, with each
    /// optional field only when set.
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        loads(py, &self.inner.to_json(false))
    }

    /// What the file states as its format: `"bullterrier-control/bundle"`.
    #[getter]
    #[expect(clippy::unused_self, reason = "an attribute of every bundle, the same for all")]
    const fn format(&self) -> &'static str {
        BUNDLE_FORMAT
    }

    /// The version of the format: 1, the one there is.
    #[getter]
    #[expect(clippy::unused_self, reason = "an attribute of every bundle, the same for all")]
    const fn version(&self) -> u32 {
        BUNDLE_VERSION
    }

    /// What produced the data, if said.
    #[getter]
    fn source(&self) -> Option<&str> {
        self.inner.source()
    }

    /// The length of the material in seconds, if said.
    #[getter]
    const fn duration(&self) -> Option<f64> {
        self.inner.duration()
    }

    /// The beats, if there are any.
    #[getter]
    fn grid(&self, py: Python<'_>) -> Option<Py<PyGrid>> {
        self.grid.as_ref().map(|grid| grid.clone_ref(py))
    }

    /// The series by name: a read-only mapping.
    #[getter]
    fn series<'py>(&self, py: Python<'py>) -> Bound<'py, PyAny> {
        self.series.bind(py).clone()
    }

    /// The events by name: a read-only mapping of tuples, each sorted by `t`.
    #[getter]
    fn events<'py>(&self, py: Python<'py>) -> Bound<'py, PyAny> {
        self.events.bind(py).clone()
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }

    fn __hash__(&self) -> u64 {
        hashed(&self.inner, feed_bundle)
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let bundle = &self.inner;
        let mut parts = Vec::new();
        if let Some(source) = bundle.source() {
            parts.push(format!("source={}", shown(py, source)?));
        }
        if let Some(duration) = bundle.duration() {
            parts.push(format!("duration={}", shown(py, duration)?));
        }
        if let Some(grid) = &self.grid {
            parts.push(format!("grid={}", grid.bind(py).repr()?));
        }
        let names: Vec<&str> = bundle.series().keys().map(String::as_str).collect();
        parts.push(format!("series={}", shown(py, names)?));
        let counts = PyDict::new(py);
        for (name, found) in bundle.events() {
            counts.set_item(name, found.len())?;
        }
        parts.push(format!("events={}", counts.repr()?));
        Ok(format!("Bundle({})", parts.join(", ")))
    }

    fn __reduce__<'py>(slf: &Bound<'py, Self>) -> PyResult<(Bound<'py, PyAny>, (String,))> {
        Ok((slf.get_type().getattr("from_json")?, (slf.get().inner.to_json(false),)))
    }
}

/// The JSON Schema of a Bundle file, for tools in other languages. It states shapes, types, ranges and the name
/// pattern; the rules a schema cannot state (sorted events, duration bounds, increasing beats, repeated keys, an
/// integer written as `1.0`) are enforced by `load`.
#[pyfunction]
pub(crate) fn bundle_schema(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    loads(py, &bullterrier_control::formats::bundle_schema().to_string())
}

/// The JSON text of `bundle`, one object ending in a newline; absent optional fields are left out, names sorted. Text
/// is written as it is, or with `ascii_only` as escapes, for an output whose encoding is not known to be UTF-8.
#[pyfunction]
#[pyo3(signature = (bundle, *, ascii_only = false))]
pub(crate) fn dumps(bundle: &Bound<'_, PyBundle>, ascii_only: bool) -> String {
    bundle.get().inner.to_json(ascii_only)
}

/// Whether `name` may name a series or an event list in a bundle.
#[pyfunction]
pub(crate) fn is_bundle_name(name: &str) -> bool {
    bullterrier_control::formats::is_name(name)
}
