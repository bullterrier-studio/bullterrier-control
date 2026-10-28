import numpy as np
import pytest
from hypothesis import given
from hypothesis import strategies as st

from bullterrier_control import FormatError, Series, SignalError, Unit
from bullterrier_control.signal import Constant, Signal, from_series, from_values, maximum, minimum


def a_series(values, *, start=0.0, rate=1.0, unit=Unit.RATIO) -> Series:
    return Series(start=start, rate=rate, values=tuple(values), unit=unit)


class Echo(Signal):
    """A signal that is its own time, written as a plugin author would write one."""

    def compute(self, times):
        return times


RAMP = from_series(a_series([0.0, 1.0, 0.0], start=1.0, rate=2.0))  # up from 1 s to 1.5 s, down by 2 s
STEPS = from_series(a_series([0.0, 1.0, 0.0], start=1.0, rate=2.0), interpolation="hold")


@pytest.mark.parametrize(
    ("given", "shape"),
    [
        (3.0, ()),
        ([], (0,)),
        ([0.0, 1.0, 2.0], (3,)),
        ([[0.0, 1.0], [2.0, 3.0]], (2, 2)),
        (np.arange(6.0).reshape(1, 2, 3), (1, 2, 3)),
    ],
    ids=["a scalar", "an empty list", "a list", "a matrix", "a 3-d array"],
)
def test_sampling_keeps_the_shape_of_the_times_and_returns_floats(given, shape):
    values = Echo().sample(given)
    assert values.shape == shape
    assert values.dtype == np.float64


def test_a_time_before_zero_reads_as_time_zero():
    assert Echo().sample([-5.0, -0.001, 0.0, 2.0]).tolist() == [0.0, 0.0, 0.0, 2.0]


def test_whole_numbers_as_times_are_fine():
    assert Echo().sample([1, 2, 3]).tolist() == [1.0, 2.0, 3.0]


BAD_TIMES = {
    "nan": ([0.0, float("nan")], "finite"),
    "infinity": ([float("inf")], "finite"),
    "minus infinity": (float("-inf"), "finite"),
    "text in a list": (["soon"], "numbers"),
    "a numeric string, which is not a number": ("0.15", "numbers"),
    "bytes": (b"1", "numbers"),
    "a bool": (True, "numbers"),
    "nothing": (None, "numbers"),
    "an integer too large for a float": (10**400, "numbers"),
}


@pytest.mark.parametrize(("times", "named"), BAD_TIMES.values(), ids=BAD_TIMES.keys())
def test_times_that_are_not_finite_numbers_are_refused(times, named):
    with pytest.raises(SignalError, match=named):
        Echo().sample(times)


def test_the_same_times_give_the_same_values():
    times = np.linspace(0, 3, 17)
    assert RAMP.sample(times).tolist() == RAMP.sample(times).tolist()


LINEAR = {
    "before the series, which holds its first value": ([0.0, 0.5], [0.0, 0.0]),
    "on the samples": ([1.0, 1.5, 2.0], [0.0, 1.0, 0.0]),
    "between the samples": ([1.25, 1.75], [0.5, 0.5]),
    "after the series, which holds its last value": ([2.5, 9.0], [0.0, 0.0]),
}


@pytest.mark.parametrize(("times", "expected"), LINEAR.values(), ids=LINEAR.keys())
def test_a_linear_series_signal_interpolates_and_holds_its_ends(times, expected):
    assert RAMP.sample(times).tolist() == pytest.approx(expected)


HOLD = {
    "before the series": ([0.0, 0.9], [0.0, 0.0]),
    "on a sample": ([1.0, 1.5, 2.0], [0.0, 1.0, 0.0]),
    "just before a sample the previous value holds": ([1.4, 1.9], [0.0, 1.0]),
    "after the series": ([2.4, 100.0], [0.0, 0.0]),
}


@pytest.mark.parametrize(("times", "expected"), HOLD.values(), ids=HOLD.keys())
def test_a_held_series_signal_keeps_each_value_until_the_next_sample(times, expected):
    assert STEPS.sample(times).tolist() == pytest.approx(expected)


def test_a_series_of_one_sample_is_that_value_at_every_time():
    for interpolation in ("linear", "hold"):
        signal = from_series(a_series([0.7], start=3.0, rate=10.0), interpolation=interpolation)
        assert signal.sample([0.0, 3.0, 3.05, 1000.0]).tolist() == [0.7, 0.7, 0.7, 0.7]


@given(
    start=st.floats(min_value=0, max_value=10),
    rate=st.floats(min_value=0.5, max_value=1000),
    values=st.lists(st.floats(min_value=-1e3, max_value=1e3), min_size=1, max_size=40),
)
def test_a_series_signal_gives_back_each_sample_at_its_own_time(start, rate, values):
    times = [start + index / rate for index in range(len(values))]
    for interpolation in ("linear", "hold"):
        signal = from_series(a_series(values, start=start, rate=rate), interpolation=interpolation)
        assert signal.sample(times).tolist() == pytest.approx(values, abs=1e-6)


def test_an_interpolation_that_is_not_offered_is_refused():
    with pytest.raises(ValueError, match="cubic"):
        from_series(a_series([0.0, 1.0]), interpolation="cubic")  # type: ignore[arg-type]


FROM_VALUES = {
    "linear between samples, the value at a sample time": (
        {"values": [0.0, 1.0, 0.2, 0.0], "rate": 10.0},
        [0.0, 0.05, 0.1, 0.3],
        [0.0, 0.5, 1.0, 0.0],
    ),
    "hold keeps the earlier sample": (
        {"values": [0.0, 1.0, 0.2, 0.0], "rate": 10.0, "interpolation": "hold"},
        [0.05, 0.1, 0.15],
        [0.0, 1.0, 1.0],
    ),
    "the first value holds before start": (
        {"values": [0.4, 0.8], "rate": 10.0, "start": 1.0},
        [0.0, 0.5, 1.0, 1.05, 1.1, 5.0],
        [0.4, 0.4, 0.4, 0.6, 0.8, 0.8],
    ),
    "an array of values": ({"values": np.array([0.0, 1.0]), "rate": 2.0}, [0.0, 0.25, 0.5], [0.0, 0.5, 1.0]),
}


@pytest.mark.parametrize(("arguments", "times", "expected"), FROM_VALUES.values(), ids=FROM_VALUES.keys())
def test_values_make_a_signal_in_one_call(arguments, times, expected):
    assert from_values(**arguments).sample(times).tolist() == pytest.approx(expected)


FROM_VALUES_REFUSED = {
    "no values": ({"values": [], "rate": 10.0}, "values"),
    "a nan value": ({"values": [0.0, float("nan")], "rate": 10.0}, "values[1]"),
    "a nan in an array": ({"values": np.array([0.0, np.nan]), "rate": 10.0}, "values[1]"),
    "rate zero": ({"values": [0.0, 1.0], "rate": 0.0}, "rate"),
    "negative start": ({"values": [0.0, 1.0], "rate": 10.0, "start": -1.0}, "start"),
}


@pytest.mark.parametrize(("arguments", "named"), FROM_VALUES_REFUSED.values(), ids=FROM_VALUES_REFUSED.keys())
def test_values_that_break_the_series_rules_are_refused_naming_the_field(arguments, named, reports):
    with pytest.raises(FormatError) as refused:
        from_values(**arguments)
    reports(refused.value, named)


def test_values_with_an_interpolation_that_is_not_offered_are_refused():
    with pytest.raises(ValueError, match="cubic"):
        from_values([0.0, 1.0], rate=1.0, interpolation="cubic")  # type: ignore[arg-type]


TIMES = [0.0, 1.25, 1.5, 2.0, 9.0]
RAMP_AT_TIMES = [0.0, 0.5, 1.0, 0.0, 0.0]


def values_of(signal: Signal) -> list[float]:
    return signal.sample(TIMES).tolist()


ARITHMETIC = {
    "a signal plus a number": (RAMP + 1, [1.0, 1.5, 2.0, 1.0, 1.0]),
    "a number plus a signal": (1 + RAMP, [1.0, 1.5, 2.0, 1.0, 1.0]),
    "a signal minus a number": (RAMP - 0.5, [-0.5, 0.0, 0.5, -0.5, -0.5]),
    "a number minus a signal": (2 - RAMP, [2.0, 1.5, 1.0, 2.0, 2.0]),
    "a signal times a number": (RAMP * 3, [0.0, 1.5, 3.0, 0.0, 0.0]),
    "a number times a signal": (3 * RAMP, [0.0, 1.5, 3.0, 0.0, 0.0]),
    "a negated signal": (-RAMP, [-0.0, -0.5, -1.0, -0.0, -0.0]),
    "two signals added": (RAMP + RAMP, [0.0, 1.0, 2.0, 0.0, 0.0]),
    "two signals multiplied": (RAMP * RAMP, [0.0, 0.25, 1.0, 0.0, 0.0]),
    "two signals subtracted": (RAMP - STEPS, [0.0, 0.5, 0.0, 0.0, 0.0]),
    "the smaller of two": (minimum(RAMP, 0.4), [0.0, 0.4, 0.4, 0.0, 0.0]),
    "the larger of a signal and a number": (maximum(0.4, RAMP), [0.4, 0.5, 1.0, 0.4, 0.4]),
    "a crossfade built from arithmetic": (
        RAMP * (1 - Constant(0.25)) + STEPS * Constant(0.25),
        [0.0, 0.375, 1.0, 0.0, 0.0],
    ),
}


@pytest.mark.parametrize(("signal", "expected"), ARITHMETIC.values(), ids=ARITHMETIC.keys())
def test_signals_combine_with_arithmetic_pointwise(signal, expected):
    assert values_of(signal) == pytest.approx(expected)


def test_a_combination_of_signals_is_a_signal_that_can_be_combined_again():
    combined = (RAMP + 1) * 2 - RAMP
    assert isinstance(combined, Signal)
    assert values_of(combined) == pytest.approx([2.0, 2.5, 3.0, 2.0, 2.0])


def test_a_combination_reads_times_before_zero_as_zero_like_its_parts():
    assert (RAMP + Echo()).sample([-3.0]).tolist() == [0.0]


@pytest.mark.parametrize("operand", [True, "1", None, [1.0]], ids=["a bool", "a string", "nothing", "a list"])
def test_a_signal_cannot_be_combined_with_something_that_is_not_a_signal_or_a_number(operand):
    with pytest.raises(TypeError):
        RAMP + operand


def test_a_number_that_is_not_finite_is_refused_when_it_is_combined():
    with pytest.raises(SignalError, match="finite"):
        RAMP + float("nan")


@given(
    first=st.lists(st.floats(min_value=-100, max_value=100), min_size=1, max_size=10),
    second=st.lists(st.floats(min_value=-100, max_value=100), min_size=1, max_size=10),
    times=st.lists(st.floats(min_value=0, max_value=20), max_size=10),
)
def test_arithmetic_on_signals_equals_arithmetic_on_their_values(first, second, times):
    a = from_series(a_series(first, rate=2.0))
    b = from_series(a_series(second, rate=3.0))
    expected = a.sample(times) * b.sample(times) - a.sample(times)
    assert (a * b - a).sample(times).tolist() == pytest.approx(expected.tolist())


class Broken(Signal):
    """A signal with a bug: it returns what it was told to."""

    def __init__(self, values):
        self._values = values

    def compute(self, times):
        return np.full(times.shape, self._values)


@pytest.mark.parametrize("bad", [float("nan"), float("inf"), float("-inf")], ids=["nan", "inf", "-inf"])
def test_a_value_that_is_not_finite_is_refused_naming_the_time_not_returned(bad):
    with pytest.raises(SignalError, match="finite"):
        Broken(bad).sample([0.0, 1.0])


def test_arithmetic_that_overflows_is_refused_not_returned_as_infinity():
    with pytest.raises(SignalError, match="finite"):
        (Constant(1e300) * 1e300).sample(0.0)


def test_a_series_of_huge_values_that_overflows_between_samples_is_refused():
    series = from_series(a_series([-1.7e308, 1.7e308], rate=1.0))
    with pytest.raises(SignalError, match="finite"):
        series.sample(0.5)


def test_a_held_series_reads_its_last_value_at_any_time_without_warnings():
    held = from_series(a_series([1.0, 2.0, 3.0], rate=1.0), interpolation="hold")
    assert held.sample([1e19, 1e300, 1.7e308]).tolist() == [3.0, 3.0, 3.0]
    fast = from_series(a_series([1.0, 2.0], rate=1e300), interpolation="hold")
    assert fast.sample([1.0]).tolist() == [2.0]


@pytest.mark.parametrize(
    "number", [np.float32(2), np.int64(2), np.float64(2), np.int8(2)], ids=lambda n: type(n).__name__
)
def test_numpy_numbers_combine_with_signals_on_either_side(number):
    assert (number * RAMP).sample([1.5]).tolist() == [2.0]
    assert (RAMP * number).sample([1.5]).tolist() == [2.0]
    assert (RAMP + number).sample([1.5]).tolist() == [3.0]
    assert (number - RAMP).sample([1.5]).tolist() == [1.0]


@pytest.mark.parametrize(
    "combine",
    [lambda s: np.array([1.0, 2.0]) * s, lambda s: s + np.array([1, 2]), lambda s: np.array([1.0]) - s],
    ids=["array times signal", "signal plus array", "array minus signal"],
)
def test_an_array_is_not_an_operand_and_never_makes_an_array_of_signals(combine):
    with pytest.raises(TypeError):
        combine(RAMP)


def test_a_number_too_large_to_be_a_float_is_refused_when_combined():
    with pytest.raises(SignalError, match="finite"):
        RAMP * 10**400
