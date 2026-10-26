import warnings

import pytest
from hypothesis import given, settings
from hypothesis import strategies as st

from bullterrier_control import ParameterError, Series, Unit, lib

LARGEST = 1.7976931348623157e308


def ramp(*values, start=0.0, rate=10.0, unit=Unit.RATIO):
    return Series(start=start, rate=rate, values=values, unit=unit)


# The q-th percentile of n values is read at index q * (n - 1) of the sorted values, between the two values around it
# in proportion (Hyndman and Fan, type 7); the output is (v - low percentile) / (high - low) clipped to 0..1. Each
# expectation below is that, worked by hand.


def test_normalise_puts_the_percentiles_at_0_and_1_and_clips_what_lies_beyond():
    # 11 values 0..10: the 10th percentile is at index 1 (value 1) and the 90th at index 9 (value 9), so (v - 1) / 8
    out = lib.normalise(series=ramp(*range(11)), lower=0.1, upper=0.9)
    assert out.values == pytest.approx([0, 0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1, 1], abs=1e-12)


def test_normalise_says_how_it_scaled_and_keeps_the_start_and_rate():
    out = lib.normalise(series=ramp(*range(11), start=2.5, rate=4.0), lower=0.1, upper=0.9)
    assert out.scaling == "percentile 10 to 90 of the series, clipped to 0..1"
    assert (out.unit, out.start, out.rate) == (Unit.FRACTION, 2.5, 4.0)


@pytest.mark.parametrize(
    ("lower", "upper", "said"),
    [
        pytest.param(0.1, 0.9, "percentile 10 to 90", id="whole percentages"),
        pytest.param(0.1234567, 0.9, "percentile 12.34567 to 90", id="a seven digit fraction is not rounded"),
        pytest.param(0.29, 0.5, "percentile 29 to 50", id="0.29 * 100 is 28.999999999999996, which reads 29"),
        pytest.param(0.001, 0.999, "percentile 0.1 to 99.9", id="tenths of a percent"),
        pytest.param(0.5, 0.9876543, "percentile 50 to 98.76543", id="a seven digit upper fraction"),
    ],
)
def test_normalise_says_the_percentiles_as_they_were_given(lower, upper, said):
    scaling = lib.normalise(series=ramp(0.0, 1.0, 2.0), lower=lower, upper=upper).scaling
    assert scaling == f"{said} of the series, clipped to 0..1"


@pytest.mark.parametrize(
    ("values", "named"),
    [
        pytest.param({"lower": 0.995, "upper": 1.0}, "lower", id="a lower percentile just above 0.99"),
        pytest.param({"lower": 0.0, "upper": 0.005}, "upper", id="an upper percentile just below 0.01"),
    ],
)
def test_normalise_refuses_percentiles_in_the_last_hundredth_at_either_end(values, named, reports):
    with pytest.raises(ParameterError) as refused:
        lib.normalise(series=ramp(0.0, 1.0), **values)
    reports(refused.value, named)
    assert [path for path, _ in refused.value.problems] == [named]  # the range, not the order of the two


@pytest.mark.parametrize(
    "values",
    [
        pytest.param({"lower": 0.99, "upper": 1.0}, id="lower at 0.99"),
        pytest.param({"lower": 0.0, "upper": 0.01}, id="upper at 0.01"),
    ],
)
def test_normalise_accepts_the_percentiles_0_99_and_0_01(values):
    assert lib.normalise(series=ramp(0.0, 1.0), **values).unit is Unit.FRACTION


def test_normalise_by_the_defaults_reads_the_5th_and_98th_percentiles():
    # 101 values 0..100: the 5th percentile is value 5 (index 5) and the 98th value 98: (v - 5) / 93
    out = lib.normalise(series=ramp(*range(101)))
    assert out.scaling == "percentile 5 to 98 of the series, clipped to 0..1"
    assert [out.values[index] for index in (0, 5, 6, 50, 98, 100)] == pytest.approx(
        [0, 0, 1 / 93, 45 / 93, 1, 1], abs=1e-12
    )


@pytest.mark.parametrize(
    ("given", "values", "expected"),
    [
        pytest.param(
            (0, 10, 20, 30, 40),
            {"lower": 0.25, "upper": 0.875},
            [0, 0, 0.4, 0.8, 1],
            id="the 87.5th percentile is index 3.5, halfway between 30 and 40: 35; the 25th is 10; (v - 10) / 25",
        ),
        pytest.param(
            (0, 10, 20, 30, 40),
            {"lower": 0.125, "upper": 0.75},
            [0, 0.2, 0.6, 1, 1],
            id="the 12.5th percentile is index 0.5, between 0 and 10: 5; the 75th is 30; (v - 5) / 25",
        ),
        pytest.param(
            (40, 0, 30, 10, 20),
            {"lower": 0.25, "upper": 0.875},
            [1, 0, 0.8, 0, 0.4],
            id="the order of the values does not matter to the percentiles, only to where each output lands",
        ),
        pytest.param(
            (0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 10),
            {},
            [0, 0, 0, 0, 0, 0, 0, 0, 0, 5 / 9, 1],
            id="the 98th percentile is index 9.8, 5 + 0.8 * 5 = 9; the 5th is 0; 5 / 9, and 10 / 9 clipped",
        ),
        pytest.param(
            (-4, -2, 0, 2, 4),
            {"lower": 0.0, "upper": 1.0},
            [0, 0.25, 0.5, 0.75, 1],
            id="the 0th and 100th percentiles are the lowest and highest value",
        ),
    ],
)
def test_normalise_interpolates_a_percentile_between_the_values_around_it(given, values, expected):
    assert lib.normalise(series=ramp(*given), **values).values == pytest.approx(expected, abs=1e-12)


@pytest.mark.parametrize(
    ("given", "values"),
    [
        pytest.param((3.0, 3.0, 3.0), {}, id="a flat series"),
        pytest.param((3.0,), {}, id="one value"),
        pytest.param((0.0, 0.0), {}, id="a flat series of zeros"),
        pytest.param(
            (0.0,) * 10 + (10.0,), {"lower": 0.05, "upper": 0.9}, id="equal percentiles though one value differs"
        ),
        pytest.param((-2.5, -2.5), {"lower": 0.0, "upper": 1.0}, id="a flat negative series"),
    ],
)
def test_normalise_gives_zeros_when_the_percentiles_are_equal_and_warns_of_nothing(given, values):
    with warnings.catch_warnings():
        warnings.simplefilter("error")
        out = lib.normalise(series=ramp(*given), **values)
    assert out.values == (0.0,) * len(given)


def test_normalise_reaches_across_the_whole_range_of_floats():
    out = lib.normalise(series=ramp(-LARGEST, 0.0, LARGEST), lower=0.0, upper=1.0)
    assert out.values == (0.0, 0.5, 1.0)


def test_normalise_clips_a_value_far_above_a_tiny_span_to_1_without_overflowing():
    # 97 zeros, two values of 1e-320 (a denormal) and a 1.0: both percentiles lie among the tiny values, the span is
    # 1e-320 and 1.0 / 1e-320 is beyond the floats, so it is clipped to 1
    out = lib.normalise(series=ramp(*([0.0] * 97), 1e-320, 1e-320, 1.0), lower=0.05, upper=0.98)
    assert out.values == (0.0,) * 97 + (1.0, 1.0, 1.0)


def test_normalise_takes_any_unit_and_a_series_that_is_already_a_fraction():
    already = Series(rate=1.0, values=(0.2, 0.4, 0.6), unit=Unit.FRACTION, scaling="by hand")
    out = lib.normalise(series=already, lower=0.0, upper=1.0)
    assert out.values == pytest.approx([0, 0.5, 1], abs=1e-12)
    assert out.scaling == "percentile 0 to 100 of the series, clipped to 0..1"


def test_normalise_takes_a_series_given_as_a_mapping():
    out = lib.normalise(series={"rate": 2, "values": [1.0, 3.0], "unit": "decibels"}, lower=0.0, upper=1.0)
    assert (out.rate, out.values) == (2.0, (0.0, 1.0))


@settings(max_examples=200)
@given(
    values=st.lists(st.integers(min_value=-1000, max_value=1000), min_size=1, max_size=30),
    gain=st.sampled_from([0.5, 2, 3, 7, 1000]),
    shift=st.integers(min_value=-1000, max_value=1000),
)
def test_normalise_is_unchanged_by_scaling_and_shifting_the_series(values, gain, shift):
    # whole numbers, so that the scaled series is exact and no percentile sits a rounding error apart
    plain = lib.normalise(series=ramp(*map(float, values))).values
    moved = lib.normalise(series=ramp(*(gain * value + shift for value in values))).values
    assert moved == pytest.approx(plain, abs=1e-9)


@settings(max_examples=200)
@given(values=st.lists(st.floats(min_value=-1e6, max_value=1e6), min_size=1, max_size=30))
def test_normalise_gives_values_from_0_to_1_that_never_fall_where_the_input_rises(values):
    out = lib.normalise(series=ramp(*sorted(values))).values
    assert all(0.0 <= value <= 1.0 for value in out)
    assert list(out) == sorted(out)


@pytest.mark.parametrize(
    ("values", "named"),
    [
        pytest.param({"lower": 0.05, "upper": 0.05}, "upper", id="equal percentiles"),
        pytest.param({"lower": 0.5, "upper": 0.25}, "upper", id="the upper below the lower"),
        pytest.param({"lower": 1.5}, "lower", id="a lower percentile above the range"),
        pytest.param({"upper": -0.5}, "upper", id="an upper percentile below the range"),
    ],
)
def test_normalise_refuses_percentiles_that_do_not_rise_or_are_out_of_range(values, named, reports):
    with pytest.raises(ParameterError) as refused:
        lib.normalise(series=ramp(0.0, 1.0), **values)
    reports(refused.value, named)


def test_normalise_refuses_a_bad_series_naming_it(reports):
    with pytest.raises(ParameterError) as refused:
        lib.normalise(series={"rate": 0, "values": [1.0, 2.0], "unit": "ratio"})
    reports(refused.value, "series.rate")


def test_normalise_needs_a_series():
    with pytest.raises(TypeError, match="series"):
        lib.normalise()
