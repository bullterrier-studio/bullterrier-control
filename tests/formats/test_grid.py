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
