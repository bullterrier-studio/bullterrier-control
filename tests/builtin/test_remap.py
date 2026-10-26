import numpy as np
import pytest

from bullterrier_control import ParameterError, lib
from bullterrier_control.plugin import curve


def remap_of(value: float, **parameters) -> float:
    """The remapped value of a signal that holds `value`."""
    return float(lib.remap(lib.constant(value=value), **parameters).sample(0.0))


@pytest.mark.parametrize(
    ("value", "expected"),
    [
        pytest.param(0.0, 40.0, id="0 is low"),
        pytest.param(0.5, 80.0, id="the middle: 40 + 80 * 0.5"),
        pytest.param(1.0, 120.0, id="1 is high"),
        pytest.param(1.7, 120.0, id="above 1 is clipped to 1 first"),
        pytest.param(-0.3, 40.0, id="below 0 is clipped to 0 first"),
    ],
)
def test_it_puts_0_to_1_onto_low_to_high_and_clips_what_is_outside(value, expected):
    assert remap_of(value, low=40, high=120) == pytest.approx(expected, abs=1e-12)


def test_the_defaults_leave_0_to_1_alone():
    assert [remap_of(value) for value in (0.0, 0.25, 1.0)] == [0.0, 0.25, 1.0]


def test_an_ease_reshapes_before_the_range_is_applied():
    # in-out is smoothstep p^2 (3 - 2p): at 0.25 it is 0.0625 * 2.5 = 0.15625, so 40 + 80 * 0.15625 = 52.5
    assert remap_of(0.25, low=40, high=120, ease="in-out") == pytest.approx(52.5, abs=1e-12)


@pytest.mark.parametrize("value", [0.0, 0.1, 0.25, 0.5, 0.9, 1.0])
def test_a_bezier_with_its_handles_on_the_diagonal_is_linear(value):
    # x(t) = y(t) = 3t^2 - 2t^3 for handles (0, 0) and (1, 1), so the curve maps p to p
    assert remap_of(value, ease="cubic-bezier(0, 0, 1, 1)") == pytest.approx(value, abs=1e-9)


def test_a_low_above_high_inverts():
    assert remap_of(0.25, low=1, high=0) == 0.75  # 1 + (0 - 1) * 0.25


@pytest.mark.parametrize(
    ("value", "expected"),
    [pytest.param(0.0, 0.3, id="0 is exactly low"), pytest.param(1.0, 0.9, id="1 is exactly high")],
)
def test_the_ends_are_exactly_the_range_not_an_ulp_beyond(value, expected):
    assert remap_of(value, low=0.3, high=0.9) == expected  # 0.3 + (0.9 - 0.3) * 1 is 0.9000000000000001


@pytest.mark.parametrize(
    ("low", "high"),
    [
        pytest.param(40.0, 120.0, id="a range"),
        pytest.param(0.3, 0.9, id="fractions"),
        pytest.param(1.0, 0.0, id="inverted"),
    ],
)
def test_an_ease_that_overshoots_still_stays_inside_the_range(low, high):
    ramp = lib.keyframes(points=[{"t": 0, "value": -0.5}, {"t": 2, "value": 1.5}])  # the input is t - 0.5
    times = np.random.default_rng(3).uniform(0, 2, 1000)
    out = lib.remap(ramp, low=low, high=high, ease="overshoot").sample(times)
    assert out.min() >= min(low, high)
    assert out.max() <= max(low, high)
    # the clip is at work: the back-out passes 1 on its way (1.0465 at 0.8), and the range ends are reached exactly
    assert out.max() == max(low, high)
    assert out.min() == min(low, high)


@pytest.mark.parametrize(
    "ease",
    [
        pytest.param("wobbly", id="an unknown name"),
        pytest.param("cubic-bezier(1.5, 0, 1, 1)", id="a bezier with x1 outside 0..1"),
        pytest.param("cubic-bezier(0, 0, 1)", id="a bezier with three numbers"),
        pytest.param("", id="empty"),
    ],
)
def test_an_ease_that_is_not_one_is_refused_naming_the_field(ease, reports):
    with pytest.raises(ParameterError, match="ease") as error:
        lib.remap(lib.constant(value=0.5), ease=ease)
    reports(error.value, "ease")


@pytest.mark.parametrize(
    ("ease", "expected"),
    [
        # each by hand from the formulas in easing.py (Penner) at p = 0.25
        pytest.param("linear", 0.25, id="linear: p"),
        pytest.param("hold", 0.0, id="hold: 0 until the end"),
        pytest.param("in", 0.0625, id="in: p^2"),
        pytest.param("out", 0.4375, id="out: 1 - (1 - p)^2 = 1 - 0.5625"),
        pytest.param("in-out", 0.15625, id="in-out: p^2 (3 - 2p) = 0.0625 * 2.5"),
        pytest.param("cubic-in", 0.015625, id="cubic-in: p^3"),
        pytest.param("cubic-out", 0.578125, id="cubic-out: 1 - 0.75^3 = 1 - 0.421875"),
        pytest.param("cubic-in-out", 0.0625, id="cubic-in-out below the middle: 4 p^3"),
        # 1 + 2.70158 * (-0.75)^3 + 1.70158 * (-0.75)^2 = 1 - 1.1397290625 + 0.95713875
        pytest.param("overshoot", 0.8174096875, id="overshoot: Penner back-out, 1.70158"),
        pytest.param(" cubic-bezier( 0 ,0, 1 ,1 ) ", 0.25, id="a bezier on the diagonal, whitespace free"),
    ],
)
def test_each_ease_is_applied_to_an_input_inside_0_to_1(ease, expected):
    assert remap_of(0.25, ease=ease) == pytest.approx(expected, abs=1e-9)


@pytest.mark.parametrize("ease", ["cubic-bezier(0.42, 0, 0.58, 1)", "cubic-bezier(0.1, 0.2, 0.3, 1.4)"])
def test_a_cubic_bezier_is_accepted_and_keeps_the_ends(ease):
    assert [remap_of(value, low=5, high=6, ease=ease) for value in (0.0, 1.0)] == [5.0, 6.0]


@pytest.mark.parametrize(
    ("low", "high"),
    [
        pytest.param(0.0, 1.0, id="0 to 1"),
        pytest.param(40.0, 120.0, id="a range"),
        pytest.param(1.0, 0.0, id="inverted"),
    ],
)
def test_a_bezier_whose_handles_leave_0_to_1_still_stays_inside_the_range(low, high):
    ease = "cubic-bezier(0.3, -2, 0.7, 3)"
    # the curve leaves 0..1 both ways, by hand at Bezier parameters 0.2 and 0.8: progress x = 0.1904 and 0.8096, value
    # y = 3(-2)(0.8)^2(0.2) + 3(3)(0.8)(0.2)^2 + 0.2^3 = -0.472 and 1.472
    assert curve(ease)(np.array([0.1904, 0.8096])).tolist() == pytest.approx([-0.472, 1.472], abs=1e-9)
    ramp = lib.keyframes(points=[{"t": 0, "value": -0.5}, {"t": 2, "value": 1.5}])
    out = lib.remap(ramp, low=low, high=high, ease=ease).sample(np.linspace(0, 2, 2001))
    assert out.min() == min(low, high)
    assert out.max() == max(low, high)
