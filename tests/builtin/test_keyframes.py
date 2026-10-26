import numpy as np
import pytest
from hypothesis import given
from hypothesis import strategies as st

from bullterrier_control import Keyframe, ParameterError, lib

# Four points, three segments of different lengths and eases (2 s `in`, 4 s `out`, 1 s `cubic-in`); the last point's
# ease is ignored. Each expected value below is a + (b - a) * ease(p), with p = (t - a.t) / (b.t - a.t).
FOUR = [
    Keyframe(t=0, value=0, ease="in"),
    Keyframe(t=2, value=1, ease="out"),
    Keyframe(t=6, value=0.5, ease="cubic-in"),
    Keyframe(t=7, value=2, ease="in-out"),
]
FOUR_BY_HAND = {
    # first segment, p = t / 2 and `in` is p^2
    0.5: 0.0625,  # p = 0.25: 0.0625
    1.0: 0.25,  # p = 0.5: 0.25
    # second segment, p = (t - 2) / 4 and `out` is 1 - (1 - p)^2; the value falls from 1 to 0.5
    3.0: 0.78125,  # p = 0.25: 1 - 0.75^2 = 0.4375; 1 - 0.5 * 0.4375
    5.0: 0.53125,  # p = 0.75: 1 - 0.25^2 = 0.9375; 1 - 0.5 * 0.9375
    # third segment, p = t - 6 and `cubic-in` is p^3; the value rises from 0.5 to 2
    6.5: 0.6875,  # p = 0.5: 0.125; 0.5 + 1.5 * 0.125
    6.75: 0.5 + 1.5 * 0.421875,  # p = 0.75: 0.75^3 = 0.421875
    # the points themselves, and after the last
    0.0: 0.0,
    2.0: 1.0,
    6.0: 0.5,
    7.0: 2.0,
    7.5: 2.0,
}


@pytest.mark.parametrize(("time", "expected"), FOUR_BY_HAND.items(), ids=[f"t={time}" for time in FOUR_BY_HAND])
def test_each_segment_follows_the_ease_of_the_point_it_starts_at(time, expected):
    assert lib.keyframes(points=FOUR).sample(time).tolist() == pytest.approx(expected, abs=1e-12)


def test_a_read_in_one_call_gives_the_same_values_as_one_by_one():
    times = np.array(list(FOUR_BY_HAND))
    together = lib.keyframes(points=FOUR).sample(times)
    assert together.tolist() == pytest.approx(list(FOUR_BY_HAND.values()), abs=1e-12)


@pytest.mark.parametrize("time", [0.0, 3.0, 1e9])
def test_one_point_gives_its_value_at_every_time_whatever_its_ease(time):
    assert lib.keyframes(points=[Keyframe(t=0, value=0.7, ease="out")]).sample(time).tolist() == 0.7


def test_a_downward_ramp_is_the_straight_line_between_its_points():
    signal = lib.keyframes(points=[Keyframe(t=1, value=1), Keyframe(t=3, value=-1)])
    # before the first point it holds 1; then 1 - (t - 1); then it holds -1
    assert signal.sample([0.0, 1.0, 1.5, 2.0, 2.5, 3.0, 9.0]).tolist() == [1.0, 1.0, 0.5, 0.0, -0.5, -1.0, -1.0]


def test_hold_keeps_the_start_value_until_the_next_point_and_gives_its_value_there():
    signal = lib.keyframes(points=[Keyframe(t=1, value=3, ease="hold"), Keyframe(t=3, value=5)])
    assert signal.sample([1.0, 2.0, 3 - 1e-9, 3.0, 4.0]).tolist() == [3.0, 3.0, 3.0, 5.0, 5.0]


def test_before_a_first_point_that_is_not_at_zero_the_first_value_holds():
    signal = lib.keyframes(points=[Keyframe(t=2, value=0.8, ease="out"), Keyframe(t=4, value=0.2)])
    # at 3 s: p = 0.5, out = 1 - 0.5^2 = 0.75, and 0.8 - 0.6 * 0.75 = 0.35
    assert signal.sample([0.0, 1.99, 2.0, 3.0, 4.0]).tolist() == pytest.approx([0.8, 0.8, 0.8, 0.35, 0.2], abs=1e-12)


def test_points_may_be_given_as_dicts():
    signal = lib.keyframes(points=[{"t": 0, "value": 0}, {"t": 2, "value": 1, "ease": "in"}, {"t": 4, "value": 5}])
    # linear 0 to 1 over 2 s gives 0.25 at 0.5 s; at 3 s the `in` segment, p = 0.5, gives 1 + 4 * 0.25 = 2
    assert signal.sample([0.5, 3.0]).tolist() == pytest.approx([0.25, 2.0], abs=1e-12)


@pytest.mark.parametrize("time", [1e12, 1e300])
def test_a_huge_time_reads_the_last_value(time):
    assert lib.keyframes(points=FOUR).sample(time).tolist() == 2.0


def test_an_unknown_ease_is_refused_and_reported_at_its_point(reports):
    with pytest.raises(ParameterError, match="wobble") as caught:
        lib.keyframes(points=[Keyframe(t=0, value=0), {"t": 3, "value": 1, "ease": "wobble"}, Keyframe(t=5, value=0)])
    reports(caught.value, "points[1].ease")


@given(
    st.lists(
        st.tuples(st.floats(0.001, 100), st.floats(-1e3, 1e3)), min_size=1, max_size=8, unique_by=lambda step: step[0]
    ),
    st.lists(st.floats(0, 500), min_size=1, max_size=20),
)
def test_linear_keyframes_are_the_straight_lines_numpy_interpolates(steps, times):
    # times of the points are the running sums of the gaps: strictly increasing
    point_times = np.cumsum([gap for gap, _ in steps]) - steps[0][0]
    values = [value for _, value in steps]
    signal = lib.keyframes(points=[{"t": t, "value": v} for t, v in zip(point_times, values, strict=True)])
    assert signal.sample(times).tolist() == pytest.approx(np.interp(times, point_times, values).tolist(), abs=1e-9)


@given(
    st.lists(
        st.tuples(st.floats(0.001, 100), st.floats(-1e3, 1e3), st.sampled_from(["linear", "out", "in-out", "hold"])),
        min_size=1,
        max_size=8,
    )
)
def test_a_read_exactly_at_a_point_gives_exactly_its_value(steps):
    # exactly: arriving at a point from the segment before it would add (b - a) to a and be a rounding off
    point_times = np.cumsum([gap for gap, _, _ in steps])
    points = [Keyframe(t=t, value=value, ease=ease) for t, (_, value, ease) in zip(point_times, steps, strict=True)]
    assert lib.keyframes(points=points).sample(point_times).tolist() == [value for _, value, _ in steps]
