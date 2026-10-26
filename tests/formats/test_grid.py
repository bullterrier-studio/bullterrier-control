import pytest
from hypothesis import given
from hypothesis import strategies as st

from bullterrier_control import FormatError, Grid

STEADY = {"beats": [0.5, 1.0, 1.5, 2.0, 2.5], "meter": 4, "first_downbeat": 1}


def test_a_steady_grid_keeps_its_beats_meter_and_downbeat():
    grid = Grid.parse(STEADY)
    assert grid.beats == (0.5, 1.0, 1.5, 2.0, 2.5)
    assert grid.meter == 4
    assert grid.first_downbeat == 1


INVALID = {
    "one beat": ({**STEADY, "beats": [1.0]}, "beats"),
    "equal neighbours": ({**STEADY, "beats": [1.0, 1.0]}, "beats"),
    "falling beats": ({**STEADY, "beats": [2.0, 1.0]}, "beats"),
    "a fall in the middle": ({**STEADY, "beats": [0.5, 1.0, 0.9, 2.0]}, "beats"),
    "negative beat": ({**STEADY, "beats": [-0.5, 0.5]}, "beats"),
    "a nan beat": ({**STEADY, "beats": [0.5, float("nan")]}, "beats"),
    "meter zero": ({**STEADY, "meter": 0}, "meter"),
    "meter a float": ({**STEADY, "meter": 4.0}, "meter"),
    "downbeat equal to the meter": ({**STEADY, "first_downbeat": 4}, "first_downbeat"),
    "downbeat negative": ({**STEADY, "first_downbeat": -1}, "first_downbeat"),
    "downbeat past the last beat": ({"beats": [0.0, 1.0], "meter": 4, "first_downbeat": 3}, "first_downbeat"),
    "tempo is not stored": ({**STEADY, "bpm": 120}, "bpm"),
    "beats missing": ({"meter": 4, "first_downbeat": 0}, "beats"),
}


@pytest.mark.parametrize(("data", "named"), INVALID.values(), ids=INVALID.keys())
def test_an_invalid_grid_is_refused_naming_the_field(data, named, reports):
    with pytest.raises(FormatError) as refused:
        Grid.parse(data)
    reports(refused.value, named)


def test_the_error_names_the_first_beat_that_does_not_increase():
    with pytest.raises(FormatError, match=r"beats\[2\]"):
        Grid.parse({**STEADY, "beats": [0.5, 1.0, 0.9, 2.0]})


@given(
    beats=st.lists(st.floats(min_value=0, max_value=1e6), min_size=2, unique=True).map(sorted),
    meter=st.integers(min_value=1, max_value=12),
    data=st.data(),
)
def test_any_increasing_beat_times_are_a_valid_grid(beats, meter, data):
    first_downbeat = data.draw(st.integers(min_value=0, max_value=min(meter, len(beats)) - 1))
    grid = Grid.parse({"beats": beats, "meter": meter, "first_downbeat": first_downbeat})
    assert grid.beats == tuple(beats)
    assert (grid.meter, grid.first_downbeat) == (meter, first_downbeat)


STEADY_HALF_SECOND = Grid.parse({"beats": [1.0, 1.5, 2.0, 2.5], "meter": 4, "first_downbeat": 0})
# a beat of one second, then of half a second
SLOWING = Grid.parse({"beats": [0.0, 1.0, 1.5], "meter": 2, "first_downbeat": 0})


def test_the_downbeats_are_every_meter_th_beat_from_the_first_downbeat():
    grid = Grid.parse({"beats": [0, 1, 2, 3, 4, 5, 6, 7], "meter": 4, "first_downbeat": 1})
    assert grid.downbeats == (1.0, 5.0)


SECONDS_AT = {
    "on the beats": (STEADY_HALF_SECOND, [0, 1, 3], [1.0, 1.5, 2.5]),
    "between beats": (STEADY_HALF_SECOND, [0.5, 2.5], [1.25, 2.25]),
    "before the first beat": (STEADY_HALF_SECOND, [-2], [0.0]),
    "after the last beat": (STEADY_HALF_SECOND, [5], [3.5]),
    "inside a slowing grid": (SLOWING, [0.5, 1.5, 2.0], [0.5, 1.25, 1.5]),
    "past the end of a slowing grid keeps the last beat length": (SLOWING, [3], [2.0]),
    "before the start of a slowing grid keeps the first beat length": (SLOWING, [-1], [-1.0]),
}


@pytest.mark.parametrize(("grid", "positions", "expected"), SECONDS_AT.values(), ids=SECONDS_AT.keys())
def test_seconds_at_interpolates_between_beats_and_extends_the_first_and_last_beat_length(grid, positions, expected):
    assert grid.seconds_at(positions).tolist() == pytest.approx(expected)


BEAT_AT = {
    "on the beats": (STEADY_HALF_SECOND, [1.0, 1.5, 2.5], [0, 1, 3]),
    "between beats": (STEADY_HALF_SECOND, [1.25], [0.5]),
    "before the first beat": (STEADY_HALF_SECOND, [0.0], [-2]),
    "after the last beat": (STEADY_HALF_SECOND, [3.5], [5]),
    "inside a slowing grid": (SLOWING, [0.5, 1.25, 1.5], [0.5, 1.5, 2.0]),
    "past the end of a slowing grid": (SLOWING, [2.0], [3]),
    "before the start of a slowing grid": (SLOWING, [-1.0], [-1]),
}


@pytest.mark.parametrize(("grid", "seconds", "expected"), BEAT_AT.values(), ids=BEAT_AT.keys())
def test_beat_at_is_the_inverse_reading_of_seconds(grid, seconds, expected):
    assert grid.beat_at(seconds).tolist() == pytest.approx(expected)


def test_a_two_beat_grid_extends_its_single_beat_length_both_ways():
    grid = Grid.parse({"beats": [1.0, 3.0], "meter": 1, "first_downbeat": 0})
    assert grid.seconds_at([-1, 0, 1, 2]).tolist() == pytest.approx([-1.0, 1.0, 3.0, 5.0])
    assert grid.beat_at([-1.0, 1.0, 3.0, 5.0]).tolist() == pytest.approx([-1, 0, 1, 2])


SHAPES = {
    "a scalar": (3.0, ()),
    "an empty list": ([], (0,)),
    "a list": ([0, 1, 2], (3,)),
    "a matrix": ([[0, 1], [2, 3]], (2, 2)),
}


@pytest.mark.parametrize(("values", "shape"), SHAPES.values(), ids=SHAPES.keys())
def test_conversions_keep_the_shape_of_what_they_are_given(values, shape):
    assert STEADY_HALF_SECOND.seconds_at(values).shape == shape
    assert STEADY_HALF_SECOND.beat_at(values).shape == shape


@given(
    gaps=st.lists(st.floats(min_value=0.05, max_value=5), min_size=1, max_size=8),
    positions=st.lists(st.floats(min_value=-4, max_value=12), min_size=1, max_size=20),
)
def test_reading_seconds_and_back_returns_the_beat_position_and_time_only_moves_forward(gaps, positions):
    beats = [0.0]
    for gap in gaps:
        beats.append(beats[-1] + gap)
    grid = Grid.parse({"beats": beats, "meter": 1, "first_downbeat": 0})
    assert grid.beat_at(grid.seconds_at(positions)).tolist() == pytest.approx(positions, abs=1e-6)
    ordered = grid.seconds_at(sorted(positions))
    assert (ordered[1:] >= ordered[:-1]).all()
