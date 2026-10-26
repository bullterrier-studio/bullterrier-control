import numpy as np
import pytest
from hypothesis import given
from hypothesis import strategies as st

from bullterrier_control import InvalidError
from bullterrier_control.easing import EASES, curve

THIRD = 1 / 3
BEZIERS = [
    "cubic-bezier(0.42, 0, 0.58, 1)",
    "cubic-bezier(0.25,0.1,0.25,1)",
    "cubic-bezier(0, 0, 0, 1)",
    "cubic-bezier(0.3, -0.5, 0.7, 1.6)",
]
# Penner's back-out rises past 1 and returns; every other ease, and a bezier whose y stays in 0..1, only rises
NEVER_FALLS = [name for name in EASES if name != "overshoot"] + BEZIERS[:3]
PROGRESS = st.floats(min_value=0.0, max_value=1.0)


def bezier_point(x1, y1, x2, y2, t):
    """The curve's own definition (CSS Easing Level 1): the point at parameter `t`, with no inversion."""
    x = 3 * x1 * (1 - t) ** 2 * t + 3 * x2 * (1 - t) * t**2 + t**3
    y = 3 * y1 * (1 - t) ** 2 * t + 3 * y2 * (1 - t) * t**2 + t**3
    return x, y


def test_the_names_are_these_nine():
    assert EASES == (
        "linear",
        "hold",
        "in",
        "out",
        "in-out",
        "cubic-in",
        "cubic-out",
        "cubic-in-out",
        "overshoot",
    )


# each value is worked out by hand from the formula in the module's docstring, at a progress with a non-symmetric result
@pytest.mark.parametrize(
    ("name", "progress", "expected"),
    [
        pytest.param("linear", 0.3, 0.3, id="linear is the identity"),
        pytest.param("in", 0.5, 0.25, id="in is p squared"),
        pytest.param("out", 0.5, 0.75, id="out is 1 - (1-p) squared"),
        pytest.param("in-out", 0.5, 0.5, id="in-out passes through the middle"),
        pytest.param("in-out", 0.2, 0.104, id="in-out is smoothstep: 0.04 * (3 - 0.4)"),
        pytest.param("cubic-in", 0.5, 0.125, id="cubic-in is p cubed"),
        pytest.param("cubic-out", 0.5, 0.875, id="cubic-out is 1 - (1-p) cubed"),
        pytest.param("cubic-in-out", 0.25, 0.0625, id="cubic-in-out below the middle is 4 p cubed"),
        pytest.param("cubic-in-out", 0.75, 0.9375, id="cubic-in-out above the middle is 1 - (2-2p) cubed / 2"),
        pytest.param("overshoot", 0.5, 1.0876975, id="overshoot: 1 + 2.70158 * -0.125 + 1.70158 * 0.25"),
        pytest.param("hold", 0.999, 0.0, id="hold stays at 0 until the end"),
        pytest.param("hold", 1.0, 1.0, id="hold is 1 at the end"),
    ],
)
def test_the_named_curves_take_their_published_values(name, progress, expected):
    assert curve(name)(progress) == pytest.approx(expected, abs=1e-12)


@pytest.mark.parametrize(
    ("name", "progress", "expected", "tolerance"),
    [
        # x(t) = t here, so the parameter is the progress: y(0.25) = 3 * 0.5625 * 0.25 + 0.25 ** 3
        pytest.param(
            f"cubic-bezier({THIRD}, 1, {2 * THIRD}, 0)", 0.25, 0.4375, 1e-9, id="x is linear, y is hand-summed"
        ),
        pytest.param(f"cubic-bezier({THIRD}, {THIRD}, {2 * THIRD}, {2 * THIRD})", 0.6, 0.6, 1e-9, id="a straight line"),
        # x(0.25) = 0.1771875 + 0.0815625 + 0.015625 = 0.274375 and y(0.25) = 0.140625 + 0.015625 = 0.15625
        pytest.param("cubic-bezier(0.42, 0, 0.58, 1)", 0.274375, 0.15625, 1e-9, id="inverts x at parameter 0.25"),
        # the same curve with x1 and x2 swapped has x(0.25) = 0.319375, so a swap lands elsewhere
        pytest.param("cubic-bezier(0.58, 0, 0.42, 1)", 0.319375, 0.15625, 1e-9, id="inverts x with x1, x2 swapped"),
        # CSS `ease`, whose value in the middle is quoted as about 0.8024 (CSS Easing Level 1 implementations)
        pytest.param("cubic-bezier(0.25, 0.1, 0.25, 1)", 0.5, 0.8024, 1e-3, id="the CSS ease"),
        # a handle at x = 1 is valid: at parameter 0.5, x = 0.375 + 0.1875 + 0.125 = 0.6875 and y = 0.75 + 0.125 = 0.875
        pytest.param("cubic-bezier(1, 2, 0.5, 0)", 0.6875, 0.875, 1e-9, id="x1 = 1 is allowed"),
        pytest.param(" cubic-bezier( 0.42 ,0 ,0.58, 1 ) ", 0.274375, 0.15625, 1e-9, id="whitespace is free"),
        pytest.param("cubic-bezier(0.42,0,0.58,1)", 0.274375, 0.15625, 1e-9, id="no whitespace"),
        pytest.param("cubic-bezier(4.2e-1, 0, .58, 1.)", 0.274375, 0.15625, 1e-9, id="exponent and bare-dot numbers"),
    ],
)
def test_a_bezier_inverts_x_and_reads_y(name, progress, expected, tolerance):
    assert curve(name)(progress) == pytest.approx(expected, abs=tolerance)


def test_a_bezier_with_both_handles_at_x_zero_is_finite_and_does_not_fall():
    ease = curve("cubic-bezier(0, 0, 0, 1)")
    values = ease(np.linspace(0.0, 1.0, 201))
    assert np.isfinite(values).all()
    assert (np.diff(values) >= -1e-12).all()
    # x = t cubed and y = 3t^2 - 2t^3, so progress 0.125 is the parameter 0.5, where y = 0.75 - 0.25
    assert ease(0.125) == pytest.approx(0.5, abs=1e-9)


@given(
    x1=st.floats(min_value=0.1, max_value=0.9),
    y1=st.floats(min_value=-2.0, max_value=3.0),
    x2=st.floats(min_value=0.1, max_value=0.9),
    y2=st.floats(min_value=-2.0, max_value=3.0),
    t=PROGRESS,
)
def test_a_bezier_goes_through_the_points_of_its_definition(x1, y1, x2, y2, t):
    x, y = bezier_point(x1, y1, x2, y2, t)
    ease = curve(f"cubic-bezier({x1!r}, {y1!r}, {x2!r}, {y2!r})")
    assert ease(x) == pytest.approx(y, abs=1e-9)


@given(progress=PROGRESS)
def test_a_bezier_with_mirrored_handles_is_symmetric_about_the_centre(progress):
    ease = curve("cubic-bezier(0.42, 0, 0.58, 1)")
    assert ease(progress) + ease(1 - progress) == pytest.approx(1.0, abs=1e-9)


@pytest.mark.parametrize("name", NEVER_FALLS)
@given(progress=st.lists(PROGRESS, min_size=2, max_size=40))
def test_an_ease_never_falls_as_progress_rises(name, progress):
    values = curve(name)(np.sort(progress))
    assert (np.diff(values) >= -1e-12).all()


def test_overshoot_goes_past_the_end_before_settling():
    values = curve("overshoot")(np.linspace(0.0, 1.0, 101)[1:-1])
    assert values.max() > 1.05  # the peak of Penner's back-out is about 1.1
    assert values.max() < 1.15


@pytest.mark.parametrize("name", [*EASES, *BEZIERS])
def test_the_ends_are_exact_and_progress_outside_0_to_1_is_clipped(name):
    ease = curve(name)
    assert ease(0.0) == 0.0
    assert ease(1.0) == 1.0
    assert ease(-3.0) == 0.0
    assert ease(7.0) == 1.0


@pytest.mark.parametrize("name", ["in-out", "overshoot", "cubic-bezier(0.42, 0, 0.58, 1)", "hold"])
@pytest.mark.parametrize(
    ("given", "shape"),
    [
        pytest.param(0.4, (), id="a scalar"),
        pytest.param([], (0,), id="an empty list"),
        pytest.param([0.1, 0.9, 1.0], (3,), id="a list"),
        pytest.param([[0.0, 0.5], [0.25, 2.0]], (2, 2), id="a matrix"),
        pytest.param(np.full((1, 2, 3), 0.5), (1, 2, 3), id="a 3-d array"),
    ],
)
def test_the_result_is_float64_in_the_shape_of_the_progress(name, given, shape):
    values = curve(name)(given)
    assert values.shape == shape
    assert values.dtype == np.float64


def test_a_curve_is_vectorised_to_the_same_values_as_one_at_a_time():
    ease = curve("cubic-bezier(0.3, -0.5, 0.7, 1.6)")
    progress = [0.0, 0.1, 0.35, 0.8, 1.0]
    assert [float(ease(p)) for p in progress] == pytest.approx(ease(progress).tolist(), abs=1e-15)


@pytest.mark.parametrize(
    "name",
    [
        pytest.param("wobble", id="an unknown name"),
        pytest.param("", id="an empty name"),
        pytest.param("Linear", id="names are case sensitive"),
        pytest.param("cubic-bezier(1.2, 0, 0, 1)", id="x1 above 1"),
        pytest.param("cubic-bezier(0, 0, -0.1, 1)", id="x2 below 0"),
        pytest.param("cubic-bezier(-0.1, 0, 0.5, 1)", id="x1 below 0"),
        pytest.param("cubic-bezier(0.5, 0, 1.5, 1)", id="x2 above 1"),
        pytest.param("cubic-bezier(0.1, 0.2, 0.3)", id="three numbers"),
        pytest.param("cubic-bezier(0.1, 0.2, 0.3, 0.4, 0.5)", id="five numbers"),
        pytest.param("cubic-bezier(0.1, nan, 0.3, 0.4)", id="nan"),
        pytest.param("cubic-bezier(0.1, inf, 0.3, 0.4)", id="an infinity"),
        pytest.param("cubic-bezier(0.1, 1e999, 0.3, 0.4)", id="a number too large for a float"),
        pytest.param("cubic-bezier(a,b,c,d)", id="text"),
        pytest.param("cubic-bezier(0.1, 0.2, 0.3, 0.4", id="no closing bracket"),
        pytest.param("cubic-bezier 0.1, 0.2, 0.3, 0.4", id="no brackets"),
    ],
)
def test_a_bad_name_is_refused_with_the_valid_ones_listed(name):
    with pytest.raises(InvalidError, match=r"in-out.*cubic-bezier\(x1, y1, x2, y2\)"):
        curve(name)


@pytest.mark.parametrize(
    "name",
    [
        pytest.param("cubic-bezier(1.2, 0, 0, 1)", id="x1 above 1"),
        pytest.param("cubic-bezier(-0.1, 0, 0.5, 1)", id="x1 below 0"),
        pytest.param("cubic-bezier(0, 0, -0.1, 1)", id="x2 below 0"),
        pytest.param("cubic-bezier(0.5, 0, 1.5, 1)", id="x2 above 1"),
    ],
)
def test_a_well_formed_bezier_with_x_outside_0_to_1_says_so(name):
    with pytest.raises(InvalidError, match=r"x1 and x2 must be in 0\.\.1.*in-out"):
        curve(name)


def test_a_bezier_that_is_not_well_formed_is_not_blamed_on_its_x():
    with pytest.raises(InvalidError, match="unknown ease") as refused:
        curve("cubic-bezier(0.1, 0.2, 0.3)")
    assert "must be in 0..1" not in str(refused.value)
