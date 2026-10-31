"""The extension's signals (`SignalBase`, `Constant`, `Rendered`, `from_series`, `from_values`, `minimum`, `maximum`,
the operators, `shape`, `make_signals_as`): the boundary the package's `Signal` is built on.

An architecture test of the extension's own interface: the package's classes inherit these, and only these tests
reach them directly. Every expected number is worked out by hand. Python code the core calls back (a subclass's
`compute`, a `Rendered`'s `run`) is hostile here: raising, returning the wrong thing, reading signals again, from
many threads at once.
"""

import gc
import subprocess
import sys
import threading
from decimal import Decimal
from fractions import Fraction

import numpy as np
import pytest

from bullterrier_control import Series
from bullterrier_control._native import _core  # noqa: PLC2701  architecture: the boundary the package's Signal is
from bullterrier_control.errors import FormatError, ParameterError, SignalError

# a ramp that is its own time from 0 s to 16 s, held at 16 after (every number here is exact in floats)
RAMP = _core.from_values([0.0, 16.0], 0.0625)
TIMEOUT = 60.0  # seconds a thread test waits before it fails as a deadlock


class Echo(_core.SignalBase):
    """A signal written in Python, as a plugin writes one: `factor * t + offset`. Its `__init__` takes arguments and
    does not call `super().__init__()`."""

    def __init__(self, factor, *, offset=0.0):
        self.factor, self.offset, self.seen = factor, offset, []

    def compute(self, times):
        self.seen.append(times)
        return self.factor * times + self.offset


class Computing(_core.SignalBase):
    """A signal whose `compute` is `body(times)`."""

    def __init__(self, body):
        self.body = body

    def compute(self, times):
        return self.body(times)


class Boom(Exception):
    """A plugin's own exception."""


def boom(_):
    raise Boom("plugin failed on purpose")


# ---- sample: what it takes and what it gives ----


@pytest.mark.parametrize(
    ("times", "expected"),
    [
        pytest.param(2, np.array(2.0), id="an int gives a 0-d array"),
        pytest.param(2.5, np.array(2.5), id="a float gives a 0-d array"),
        pytest.param(np.float16(4.0), np.array(4.0), id="a numpy scalar gives a 0-d array"),
        pytest.param([-1, 3], np.array([0.0, 3.0]), id="a time before 0 reads as 0"),
        pytest.param([[1, 2], [3, 4]], np.array([[1.0, 2.0], [3.0, 4.0]]), id="a nested list keeps its shape"),
        pytest.param(np.arange(6, dtype=np.float32).reshape(2, 3), np.arange(6.0).reshape(2, 3), id="float32"),
        pytest.param(np.array([[1, 2], [3, 4]]).T, np.array([[1.0, 3.0], [2.0, 4.0]]), id="a transposed array"),
        pytest.param(np.arange(10.0)[::3], np.array([0.0, 3.0, 6.0, 9.0]), id="a strided view"),
        pytest.param(np.array([1, 9], dtype=np.uint64), np.array([1.0, 9.0]), id="unsigned integers"),
        pytest.param([], np.zeros(0), id="no times give no values"),
        pytest.param(np.zeros((0, 3)), np.zeros((0, 3)), id="an empty 2-D array keeps its shape"),
        pytest.param([20.0, 1e300], np.array([16.0, 16.0]), id="the last value holds after the end"),
    ],
)
def test_sample_gives_float64_values_in_the_shape_of_the_times(times, expected):
    values = RAMP.sample(times)
    assert values.dtype == np.float64
    assert values.shape == expected.shape
    assert values.tolist() == expected.tolist()


@pytest.mark.parametrize(
    ("times", "named"),
    [
        pytest.param("1.0", "numbers", id="text"),
        pytest.param(True, "numbers", id="a bool"),
        pytest.param([True, False], "numbers", id="an array of bools"),
        pytest.param(None, "numbers", id="nothing"),
        pytest.param([2**70], "numbers", id="an integer too large for an array of numbers"),
        pytest.param(np.array([1 + 2j]), "numbers", id="a complex number"),
        pytest.param(np.datetime64("2026-01-01"), "numbers", id="a date"),
        pytest.param(float("nan"), "finite", id="NaN"),
        pytest.param([0.0, float("inf")], "finite", id="an infinity"),
        pytest.param(np.array([[0.0], [-np.inf]]), "finite", id="minus infinity in a 2-D array"),
    ],
)
def test_sample_refuses_a_time_that_is_not_a_finite_number(times, named):
    with pytest.raises(SignalError, match=named):
        RAMP.sample(times)


def test_a_value_that_is_not_finite_is_refused_naming_its_time():
    huge = _core.from_values([1e308, 1e308], 1.0) * 10
    with pytest.raises(SignalError, match=r"not finite at time 2\.5"):
        huge.sample([2.5])


def test_compute_of_a_signal_made_in_rust_gives_a_value_that_is_not_finite_for_the_read_that_called_it():
    huge = _core.Constant(1e308) * 10
    assert huge.compute(np.array([1.0, 2.0])).tolist() == [np.inf, np.inf]
    assert _core.Constant(0.5).compute(np.zeros((2, 2))).tolist() == [[0.5, 0.5], [0.5, 0.5]]
    with pytest.raises(SignalError, match="not finite"):
        huge.sample(1.0)


# ---- a signal written in Python ----


def test_a_python_signal_is_read_through_its_compute_with_finite_times_from_zero_as_a_1d_float64_array():
    echo = Echo(2.0, offset=0.5)
    assert echo.sample([[-1.0, 2.0], [0.25, 0.0]]).tolist() == [[0.5, 4.5], [1.0, 0.5]]
    [seen] = echo.seen
    assert seen.dtype == np.float64
    assert seen.shape == (4,)
    assert seen.tolist() == [0.0, 2.0, 0.25, 0.0]


def test_a_python_signal_whose_init_calls_super_works_too():
    class Polite(_core.SignalBase):
        def __init__(self, value):
            super().__init__()
            self.value = value

        def compute(self, times):
            return np.full(times.shape, self.value)

    assert Polite(0.75).sample([0.0, 3.0]).tolist() == [0.75, 0.75]


def test_a_signal_made_from_a_python_signal_keeps_it_alive():
    echo = Echo(3.0)
    shifted = echo + 1
    del echo
    gc.collect()
    assert shifted.sample(2.0).tolist() == 7.0


def test_a_subclass_without_compute_is_refused_when_read():
    class Empty(_core.SignalBase):
        pass

    with pytest.raises(TypeError, match="Empty must implement compute"):
        Empty().sample(1.0)


class Chained(_core.SignalBase):
    """A signal class with methods of its own, as the package's `Signal` is."""

    def doubled(self):
        return self * 2


MIXED = [
    pytest.param(_core.Constant, (3.0,), 3.0, id="Constant"),
    pytest.param(_core.Rendered, (RAMP,), 2.5, id="Rendered"),
]
MIXED_ARGUMENTS = {"rate": 2.0, "run": lambda inputs: inputs}


@pytest.mark.parametrize(("made", "arguments", "value"), MIXED)
def test_constant_or_rendered_listed_before_a_signal_class_makes_a_signal_with_its_methods(made, arguments, value):
    mixed = type("Mixed", (made, Chained), {})
    keywords = MIXED_ARGUMENTS if made is _core.Rendered else {}
    signal = mixed(*arguments, **keywords)
    assert signal.sample(2.5).tolist() == value
    assert signal.doubled().sample(2.5).tolist() == 2 * value


@pytest.mark.parametrize(
    ("made", "arguments", "keywords"),
    [
        pytest.param(_core.Constant, (3.0,), {}, id="Constant"),
        pytest.param(_core.Rendered, (RAMP,), MIXED_ARGUMENTS, id="Rendered"),
    ],
)
def test_constant_or_rendered_listed_after_a_signal_class_is_refused_when_made(made, arguments, keywords):
    # Python makes such a class with the first base's `__new__`, which would never see the arguments
    mixed = type("Mixed", (Chained, made), {})
    with pytest.raises(TypeError, match=f"list {made.__name__} first"):
        mixed(*arguments, **keywords)


@pytest.mark.parametrize(
    ("body", "expected"),
    [
        pytest.param(lambda t: (2 * t).astype(np.float32), [0.0, 2.0, 4.0], id="float32 is made float64"),
        pytest.param(lambda t: np.repeat(t, 2)[::2], [0.0, 1.0, 2.0], id="a strided view is copied"),
        pytest.param(lambda t: [7.0] * len(t), [7.0, 7.0, 7.0], id="a list of numbers"),
        pytest.param(lambda t: t.astype(np.int64) + 1, [1.0, 2.0, 3.0], id="integers"),
        pytest.param(lambda t: t > 0.5, [0.0, 1.0, 1.0], id="bools are 0 and 1 as numpy makes them"),
    ],
)
def test_a_compute_may_return_anything_numpy_makes_float64_of_in_the_shape_of_its_times(body, expected):
    assert Computing(body).sample([0.0, 1.0, 2.0]).tolist() == expected


@pytest.mark.parametrize(
    ("body", "error", "named"),
    [
        pytest.param(lambda t: np.zeros(len(t) + 1), ValueError, r"shape \(4,\) for times of shape \(3,\)", id="long"),
        pytest.param(lambda t: t.reshape(-1, 1), ValueError, r"shape \(3, 1\)", id="2-D for 1-D times"),
        pytest.param(lambda t: 0.5, ValueError, r"shape \(\)", id="a number for an array"),
        pytest.param(lambda t: None, ValueError, r"shape \(\)", id="nothing"),
        pytest.param(lambda t: "abc", ValueError, "could not convert", id="text: numpy's own error"),
        pytest.param(lambda t: object(), TypeError, "float", id="an object: numpy's own error"),
        pytest.param(lambda t: t * np.nan, SignalError, "not finite at time", id="NaN is refused by the read"),
        pytest.param(boom, Boom, "plugin failed on purpose", id="its own exception, unchanged"),
    ],
)
def test_a_compute_that_returns_what_is_not_its_values_is_an_error_of_the_read(body, error, named):
    with pytest.raises(error, match=named):
        Computing(body).sample([0.0, 1.0, 2.0])


def test_an_array_a_compute_keeps_and_changes_later_changes_nothing_read():
    kept = []

    def keep(times):
        kept.append(np.full(times.shape, 5.0))
        return kept[-1]

    keeper = Computing(keep)
    first = keeper.sample([0.0, 1.0])
    rendered = _core.Rendered(keeper, rate=1.0, run=lambda inputs: inputs)
    before = rendered.sample([0.0, 1.0, 1500.0])
    for array in kept:
        array[:] = 99.0
    assert first.tolist() == [5.0, 5.0]
    assert rendered.sample([0.0, 1.0, 1500.0]).tolist() == before.tolist() == [5.0, 5.0, 5.0]


def test_a_compute_may_read_other_signals_and_one_reading_itself_is_refused_without_crashing():
    reader = Computing(lambda t: RAMP.sample(t) + Echo(1.0).sample(t))
    assert reader.sample([1.0, 2.0]).tolist() == [2.0, 4.0]
    selfish: _core.SignalBase = Computing(lambda t: selfish.sample(t))
    with pytest.raises(SignalError, match="deep"):
        selfish.sample(1.0)


# ---- exceptions reach the caller unchanged ----


@pytest.mark.parametrize(
    "read",
    [
        pytest.param(lambda s: s, id="the signal"),
        pytest.param(lambda s: -(s * 2 + 1), id="through operators"),
        pytest.param(lambda s: _core.minimum(s, 1.0), id="through minimum"),
        pytest.param(lambda s: _core.Rendered(s, rate=10.0, run=lambda x: x), id="as the input of a Rendered"),
        pytest.param(lambda s: _core.shape("slew", s, {"rise": 0.1}), id="as the input of a Rust shaper"),
        pytest.param(lambda s: Computing(lambda t: s.sample(t)), id="read by another Python signal"),
        pytest.param(lambda s: _core.Rendered(RAMP, rate=10.0, run=boom), id="raised by a run"),
    ],
)
def test_an_exception_raised_by_python_code_reaches_the_caller_of_sample_unchanged(read):
    with pytest.raises(Boom, match="plugin failed on purpose") as raised:
        read(Computing(boom)).sample([0.5, 1.0])
    assert type(raised.value) is Boom


def test_the_exception_object_raised_is_the_one_python_code_raised():
    raised = Boom("this one")

    def fail(_):
        raise raised

    with pytest.raises(Boom) as caught:
        (Computing(fail) + 1).sample(1.0)
    assert caught.value is raised


# ---- operators, minimum, maximum ----

TWO = _core.Constant(2.0)


@pytest.mark.parametrize(
    ("made", "expected"),
    [
        pytest.param(lambda: RAMP + 1, 4.0, id="signal + int"),
        pytest.param(lambda: 1.5 + RAMP, 4.5, id="float + signal"),
        pytest.param(lambda: RAMP - np.float64(0.25), 2.75, id="signal - numpy float"),
        pytest.param(lambda: 10 - RAMP, 7.0, id="int - signal"),
        pytest.param(lambda: np.int64(2) * RAMP, 6.0, id="numpy int * signal"),  # type: ignore[operator]  # numpy defers
        pytest.param(lambda: np.float32(0.5) * RAMP, 1.5, id="numpy float32 * signal"),  # type: ignore[operator]  # as above
        pytest.param(lambda: RAMP * Fraction(1, 4), 0.75, id="signal * fraction"),  # type: ignore[operator]  # a Real
        pytest.param(lambda: RAMP * TWO - TWO, 4.0, id="signals with signals"),
        pytest.param(lambda: -RAMP, -3.0, id="negation"),
        pytest.param(lambda: Echo(2.0) * RAMP, 18.0, id="a python signal with a rust one"),
        pytest.param(lambda: _core.minimum(RAMP, 2), 2.0, id="minimum of a signal and a number"),
        pytest.param(lambda: _core.minimum(1, RAMP), 1.0, id="minimum with the number first"),
        pytest.param(lambda: _core.maximum(RAMP, TWO), 3.0, id="maximum of two signals"),
        pytest.param(lambda: _core.maximum(5.0, 4), 5.0, id="maximum of two numbers"),
    ],
)
def test_signals_combine_with_signals_and_real_numbers(made, expected):
    assert made().sample(3.0).tolist() == expected


@pytest.mark.parametrize(
    "other",
    [
        pytest.param(True, id="a bool"),
        pytest.param(np.bool_(True), id="a numpy bool"),
        pytest.param("1", id="text"),
        pytest.param(None, id="nothing"),
        pytest.param([1.0], id="a list"),
        pytest.param(np.array([1.0]), id="a numpy array"),
        pytest.param(Decimal("1.5"), id="a decimal, not a real number to Python"),
        pytest.param(1j, id="a complex number"),
    ],
)
def test_a_signal_does_not_combine_with_what_is_not_a_signal_or_a_real_number(other):
    for combine in (lambda: RAMP + other, lambda: other - RAMP, lambda: RAMP * other, lambda: other * RAMP):
        with pytest.raises(TypeError):
            combine()
    with pytest.raises(TypeError, match="signals and numbers only"):
        _core.maximum(RAMP, other)


@pytest.mark.parametrize(
    "number",
    [
        pytest.param(float("nan"), id="NaN"),
        pytest.param(float("-inf"), id="minus infinity"),
        pytest.param(np.float64("inf"), id="a numpy infinity"),
        pytest.param(10**400, id="an integer beyond the floats"),
    ],
)
def test_a_number_that_is_not_finite_is_refused_when_it_is_combined(number):
    for combine in (lambda: RAMP + number, lambda: number * RAMP, lambda: _core.minimum(number, RAMP)):
        with pytest.raises(SignalError, match="finite"):
            combine()


# ---- Constant, from_series, from_values ----


@pytest.mark.parametrize(
    ("value", "expected"),
    [
        pytest.param(0.25, 0.25, id="a float"),
        pytest.param(-3, -3.0, id="an int"),
        pytest.param(np.float32(0.5), 0.5, id="a numpy float"),
        pytest.param("1.5", 1.5, id="text float() reads"),
    ],
)
def test_a_constant_is_its_value_at_every_time(value, expected):
    assert _core.Constant(value).sample([0.0, 7.5, 1e9]).tolist() == [expected] * 3


@pytest.mark.parametrize(
    ("value", "error", "named"),
    [
        pytest.param(float("nan"), SignalError, "finite", id="NaN"),
        pytest.param(2**2000, SignalError, "finite", id="an integer beyond the floats"),
        pytest.param("many", ValueError, "many", id="text float() cannot read"),
        pytest.param(None, TypeError, "float", id="nothing"),
    ],
)
def test_a_constant_must_be_a_finite_number(value, error, named):
    with pytest.raises(error, match=named):
        _core.Constant(value)


def test_from_values_reads_values_from_start_at_rate_interpolated_or_held():
    linear = _core.from_values([0.0, 1.0, 0.5], 2.0, start=1.0)
    held = _core.from_values([0.0, 1.0, 0.5], 2.0, start=1.0, interpolation="hold")
    times = [0.0, 1.25, 1.5, 1.75, 9.0]
    assert linear.sample(times).tolist() == [0.0, 0.5, 1.0, 0.75, 0.5]
    assert held.sample(times).tolist() == [0.0, 0.0, 1.0, 1.0, 0.5]


def test_from_series_reads_a_series_as_from_values_reads_its_values():
    series = Series(rate=4.0, values=[1.0, 3.0], unit="ratio", start=0.5)
    assert _core.from_series(series).sample([0.5, 0.625, 2.0]).tolist() == [1.0, 2.0, 3.0]
    assert _core.from_series(series, interpolation="hold").sample([0.625]).tolist() == [1.0]


@pytest.mark.parametrize(
    ("made", "error", "named"),
    [
        pytest.param(lambda: _core.from_values([], 1.0), FormatError, "values", id="no values"),
        pytest.param(lambda: _core.from_values([1.0], 0.0), FormatError, "rate", id="a rate of 0"),
        pytest.param(lambda: _core.from_values([1.0], True), FormatError, "rate", id="a bool rate"),
        pytest.param(lambda: _core.from_values([1.0], 1.0, start=-1), FormatError, "start", id="a start before 0"),
        pytest.param(lambda: _core.from_values([np.nan], 1.0), FormatError, "values", id="a value that is NaN"),
        pytest.param(lambda: _core.from_values([1.0], 1.0, interpolation="cubic"), ValueError, "cubic", id="cubic"),  # type: ignore[arg-type]  # the wrong value is the case
    ],
)
def test_values_that_make_no_series_are_refused_naming_the_field(made, error, named):
    with pytest.raises(error, match=named):
        made()


# ---- Rendered ----


def running_sum(inputs):
    return np.cumsum(inputs)


def test_a_rendered_signal_holds_the_latest_sample_of_its_run_at_or_before_each_time():
    # the ramp at 10 samples a second: 0, 0.1, 0.2, ...; summed: 0, 0.1, 0.3, 0.6
    summed = _core.Rendered(RAMP, rate=10.0, run=running_sum)
    assert summed.sample([0.0, 0.1, 0.25, 0.3, -1.0]) == pytest.approx([0.0, 0.1, 0.3, 0.6, 0.0], abs=1e-12)


def test_run_gets_a_fresh_1d_c_contiguous_float64_array_of_its_own_each_time():
    given = []

    def keep(inputs):
        given.append(inputs)
        inputs *= 2  # its own: changing it changes nothing else
        return inputs

    doubled = _core.Rendered(RAMP, rate=1.0, run=keep, limit=5000)
    assert doubled.sample([3.0]).tolist() == [6.0]
    assert doubled.sample([4000.0]).tolist() == [32.0]
    first, second = given
    assert first is not second
    for inputs in given:
        assert (inputs.dtype, inputs.ndim, inputs.flags.c_contiguous, inputs.flags.owndata) == (
            np.float64,
            1,
            True,
            True,
        )


@pytest.mark.parametrize(
    ("run", "error", "named"),
    [
        pytest.param(lambda x: x[:-1], ValueError, r"recurrence returned an array of shape \(1023,\)", id="short"),
        pytest.param(lambda x: x.reshape(1, -1), ValueError, r"shape \(1, 1024\) for an input", id="2-D"),
        pytest.param(lambda x: x.astype(np.float32), None, "", id="float32 is made float64"),
        pytest.param(lambda x: list(x), None, "", id="a list"),
        pytest.param(lambda x: x + np.inf, SignalError, "not finite", id="an infinity is refused by the read"),
        pytest.param(boom, Boom, "on purpose", id="its own exception, unchanged"),
        pytest.param(42, TypeError, "callable", id="a run that is not callable"),
    ],
)
def test_a_run_that_returns_what_is_not_its_outputs_is_an_error_of_the_read(run, error, named):
    rendered = _core.Rendered(RAMP, rate=100.0, run=run)
    if error is None:
        assert rendered.sample([0.5]).tolist() == [0.5]
        return
    with pytest.raises(error, match=named):
        rendered.sample([0.5])


@pytest.mark.parametrize(
    ("rate", "limit", "named"),
    [
        pytest.param(0.0, 10, "rate", id="a rate of 0"),
        pytest.param(-1.0, 10, "rate", id="a negative rate"),
        pytest.param(float("inf"), 10, "rate", id="an infinite rate"),
        pytest.param(True, 10, "rate", id="a bool rate"),
        pytest.param(10.0, 0, "limit", id="a limit of 0"),
        pytest.param(10.0, -5, "limit", id="a negative limit"),
        pytest.param(10.0, 2.5, "limit", id="a fractional limit"),
        pytest.param(10.0, True, "limit", id="a bool limit"),
    ],
)
def test_a_wrong_argument_to_rendered_is_a_value_error(rate, limit, named):
    with pytest.raises(ValueError, match=named) as raised:
        _core.Rendered(RAMP, rate=rate, run=running_sum, limit=limit)
    assert not isinstance(raised.value, SignalError)


def test_a_read_needing_more_samples_than_the_limit_is_refused_before_rendering():
    rendered = []

    def counted(inputs):
        rendered.append(len(inputs))
        return inputs

    limited = _core.Rendered(RAMP, rate=10.0, run=counted, limit=100)
    assert limited.sample(9.9).tolist() == pytest.approx(9.9)
    with pytest.raises(SignalError, match="limit"):
        limited.sample(10.0)
    assert rendered == [100]


# ---- shape: a shaper of the Rust core ----


def test_shape_shapes_a_signal_with_a_shaper_of_the_core():
    step = _core.from_values([0.0, 1.0], 10.0, interpolation="hold")
    slewed = _core.shape("slew", step, {"rise": 0.5, "fall": 0.5, "rate": 10.0})
    # 0.2 a sample up from 0.1 s: 0.2 at 0.1 s, 0.4 at 0.25 s, 1 from 0.5 s
    assert slewed.sample([0.0, 0.1, 0.25, 0.5, 9.0]) == pytest.approx([0.0, 0.2, 0.4, 1.0, 1.0], abs=1e-12)
    assert _core.shape("follow", Echo(1.0), {"attack": 0, "rate": 10.0}).sample([0.5]).tolist() == [0.5]


@pytest.mark.parametrize(
    ("name", "params", "error", "named"),
    [
        pytest.param("slew", {"rise": "slow"}, ParameterError, "rise", id="a parameter of the wrong type"),
        pytest.param("slew", {"depth": 1.0}, ParameterError, "depth", id="an unknown parameter"),
        pytest.param("slew", {"rise": float("nan")}, ValueError, "finite", id="NaN has no JSON form"),
        pytest.param("wobble", {}, ValueError, "wobble", id="an unknown shaper"),
    ],
)
def test_shape_refuses_parameters_or_a_name_the_core_does_not_have(name, params, error, named):
    with pytest.raises(error, match=named):
        _core.shape(name, RAMP, params)


# ---- threads and the GIL ----


def isolated(code):
    """What `code` prints, run in a Python of its own that is killed after `TIMEOUT` seconds: a deadlock that holds
    the GIL stops every thread of its process, so only another process can time it out."""
    done = subprocess.run(
        [sys.executable, "-c", code], capture_output=True, text=True, encoding="utf-8", check=False, timeout=TIMEOUT
    )
    assert done.returncode == 0, done.stderr
    return done.stdout.strip()


SIXTEEN_READERS = """
import threading
import numpy as np
from bullterrier_control._native import _core
class Echo(_core.SignalBase):
    def compute(self, times):
        return times
rate = 100.0
rendered = _core.Rendered(Echo(), rate=rate, run=np.cumsum)
barrier = threading.Barrier(16)
# each thread reads a later time, so renders race to grow the history
reads = [np.linspace(0.0, 5.0 + 20.0 * index, 7) for index in range(16)]
results = {}
def read(index):
    barrier.wait()
    results[index] = rendered.sample(reads[index])
threads = [threading.Thread(target=read, args=(index,)) for index in range(16)]
for thread in threads:
    thread.start()
for thread in threads:
    thread.join()
# the oracle: the running sum of the sample times i / rate, up to the sample at or before each time
for index, times in enumerate(reads):
    samples = np.floor(times * rate + 1e-9)
    print(index, np.allclose(results[index], samples * (samples + 1) / 2 / rate, rtol=1e-12, atol=0))
"""


def test_sixteen_threads_reading_one_rendered_python_signal_at_once_get_equal_values_without_deadlock():
    assert isolated(SIXTEEN_READERS).splitlines() == [f"{index} True" for index in range(16)]


SELF_READER = """
import numpy as np
from bullterrier_control._native import _core
from bullterrier_control.errors import SignalError
selfish = _core.Rendered(
    _core.Constant(1.0), rate=10.0, run=lambda inputs: selfish.sample(np.arange(len(inputs)) / 10.0)
)
try:
    selfish.sample(1.0)
except SignalError as error:
    print(str(error))
"""


def test_a_run_that_reads_its_own_signal_is_refused_without_hanging():
    assert "deep" in isolated(SELF_READER)


def test_a_long_read_of_a_rust_signal_lets_another_python_thread_run():
    # one time, so numpy (which lets go of the GIL to copy a large array) is out of it: 4.8 million samples of a
    # hundred sums, slewed, a read of about a third of a second in Rust alone
    deep = RAMP
    for _ in range(100):
        deep = deep + 1.0
    slewed = _core.shape("slew", deep, {"rise": 0.0, "fall": 0.0, "rate": 48000.0})
    entered, seen = threading.Event(), {}

    def long_read():
        entered.set()
        seen["values"] = slewed.sample(100.0)

    interval = sys.getswitchinterval()
    # no thread gives up the GIL on a timer: the main thread runs while the reader reads only if the read lets go of it
    sys.setswitchinterval(1000.0)
    try:
        reader = threading.Thread(target=long_read, daemon=True)
        reader.start()
        assert entered.wait(TIMEOUT)
        ran_during_the_read = "values" not in seen
        reader.join(TIMEOUT)
    finally:
        sys.setswitchinterval(interval)
    assert ran_during_the_read
    assert seen["values"].tolist() == 116.0


# ---- deep graphs ----


def test_a_hundred_thousand_chained_sums_are_refused_when_read_and_dropped_without_crashing():
    total: _core.SignalBase = _core.Constant(0.0)
    for _ in range(100_000):
        total = total + 1.0
    with pytest.raises(SignalError, match="deep"):
        total.sample(0.0)
    del total
    gc.collect()


def chain(depth):
    """`depth` Python signals, each reading the one before through `sample`, over the ramp."""

    class Link(_core.SignalBase):
        def __init__(self, before):
            self.before = before

        def compute(self, times):
            return self.before.sample(times)

    signal = RAMP
    for _ in range(depth):
        signal = Link(signal)
    return signal


def test_a_chain_of_python_signals_reads_while_shallow_and_a_deep_one_is_refused_without_crashing():
    assert chain(50).sample(2.0).tolist() == 2.0
    with pytest.raises(SignalError, match="deep"):
        chain(5000).sample(2.0)


def test_a_deep_chain_of_python_signals_is_refused_on_a_small_thread_stack_without_crashing():
    code = """
import threading, sys
sys.setrecursionlimit(100_000)
from bullterrier_control._native import _core
RAMP = _core.from_values([0.0, 16.0], 0.0625)
class Link(_core.SignalBase):
    def __init__(self, before):
        self.before = before
    def compute(self, times):
        return self.before.sample(times)
signal = RAMP
for _ in range(5000):
    signal = Link(signal)
said = []
def read():
    try:
        signal.sample(2.0)
    except Exception as error:
        said.append(type(error).__name__)
threading.stack_size(512 * 1024)
reader = threading.Thread(target=read)
reader.start()
reader.join()
print(said)
"""
    assert isolated(code) == "['SignalError']"


# ---- make_signals_as ----

MADE_AS = """
from bullterrier_control._native import _core
class Signal(_core.SignalBase):
    pass
class Other(_core.SignalBase):
    pass
class Written(_core.SignalBase):
    def compute(self, times):
        return times
before = type(_core.Constant(1.0) + 1).__name__
_core.make_signals_as(Signal)
_core.make_signals_as(Signal)  # the same class again: nothing changes
made = [_core.Constant(1.0) + 1, _core.from_values([1.0], 1.0), _core.maximum(1, 2), -Written()]
print(before, {type(signal).__name__ for signal in made}, made[0].sample(5.0), made[3].sample(5.0))
for wrong in (Other, Written, int):
    try:
        _core.make_signals_as(wrong)
    except (TypeError, ValueError) as error:
        print(type(error).__name__, str(error).split(" (")[0])
"""


def test_the_signals_the_extension_makes_are_of_the_class_make_signals_as_names_once():
    assert isolated(MADE_AS).splitlines() == [
        "SignalBase {'Signal'} 2.0 -5.0",
        "ValueError signals are already made as Signal",
        "TypeError signals are made as a subclass of SignalBase that does not implement compute",
        "TypeError signals are made as a subclass of SignalBase that does not implement compute",
    ]
