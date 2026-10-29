import pytest
from hypothesis import given, settings
from hypothesis import strategies as st

from bullterrier_control import Grid, ParameterError, lib

# a beat every half second; the first downbeat is the second beat (index 1) and a bar is 4 beats
STEADY = Grid(beats=(0.0, 0.5, 1.0, 1.5, 2.0), meter=4, first_downbeat=1)
# the first beat is half a second long, the second one second
SLOWING = Grid(beats=(0.0, 0.5, 1.5), meter=4, first_downbeat=0)


def times(events):
    return [event.t for event in events]


def labels(events):
    return [event.label for event in events]


def test_beats_gives_an_event_on_each_beat_labelled_downbeat_or_beat_each_at_full_strength():
    events = lib.beats(grid=STEADY)
    assert times(events) == [0.0, 0.5, 1.0, 1.5, 2.0]
    assert labels(events) == ["beat", "downbeat", "beat", "beat", "beat"]
    assert [event.strength for event in events] == [1.0] * 5
    assert [(event.duration, event.pitch) for event in events] == [(None, None)] * 5


def test_beats_with_a_division_adds_sub_events_between_the_beats():
    events = lib.beats(grid=STEADY, division=2)
    assert times(events) == [0.0, 0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0]
    assert labels(events) == ["beat", "sub", "downbeat", "sub", "beat", "sub", "beat", "sub", "beat"]


def test_beats_puts_a_sub_halfway_in_beats_between_the_beats_it_lies_between():
    # the first beat lasts 0.5 s and the second 1.0 s: the sub-beats are at 0.25 s and at 1.0 s
    events = lib.beats(grid=SLOWING, division=2)
    assert times(events) == [0.0, 0.25, 0.5, 1.0, 1.5]
    assert labels(events) == ["downbeat", "sub", "beat", "sub", "beat"]


def test_beats_with_a_division_that_does_not_halve_puts_the_subs_a_third_apart():
    steady = Grid(beats=(0.0, 0.6, 1.2), meter=2, first_downbeat=0)
    events = lib.beats(grid=steady, division=3)
    assert times(events) == pytest.approx([0.0, 0.2, 0.4, 0.6, 0.8, 1.0, 1.2], abs=1e-12)
    assert labels(events) == ["downbeat", "sub", "sub", "beat", "sub", "sub", "downbeat"]


@pytest.mark.parametrize(
    ("meter", "first_downbeat", "expected"),
    [
        pytest.param(
            4, 1, "beat downbeat beat beat beat downbeat beat beat beat", id="a bar of 4 from the second beat"
        ),
        pytest.param(3, 2, "beat beat downbeat beat beat downbeat beat", id="a bar of 3 from the third beat"),
        pytest.param(1, 0, "downbeat downbeat downbeat downbeat", id="a bar of 1: every beat is a downbeat"),
        pytest.param(2, 0, "downbeat beat downbeat beat downbeat", id="a bar of 2 from the first beat"),
        pytest.param(4, 3, "beat beat beat downbeat", id="the downbeat on the last beat there is"),
    ],
)
def test_beats_labels_every_meter_th_beat_from_the_first_downbeat_a_downbeat_and_no_other_beat(
    meter, first_downbeat, expected
):
    grid = Grid(
        beats=tuple(index * 0.5 for index in range(len(expected.split()))), meter=meter, first_downbeat=first_downbeat
    )
    assert labels(lib.beats(grid=grid)) == expected.split()


def test_beats_has_the_count_of_beats_times_division_and_the_subs_between():
    events = lib.beats(grid=STEADY, division=64)
    assert len(events) == 4 * 64 + 1
    assert labels(events).count("sub") == 4 * 63
    assert labels(events).count("downbeat") == 1


def test_selecting_the_downbeats_of_the_beats_gives_the_first_beat_of_each_bar():
    assert times(lib.select(events=lib.beats(grid=STEADY), label="downbeat")) == [0.5]


def test_beats_needs_a_grid():
    with pytest.raises(TypeError, match="grid"):
        lib.beats()  # type: ignore[call-arg]  # left out on purpose: refused at run time too


@pytest.mark.parametrize("division", [0, 65])
def test_beats_refuses_a_division_out_of_range(division, reports):
    with pytest.raises(ParameterError) as refused:
        lib.beats(grid=STEADY, division=division)
    reports(refused.value, "division")


@settings(max_examples=200)
@given(
    gaps=st.lists(st.integers(min_value=1, max_value=64), min_size=1, max_size=12),
    first=st.integers(min_value=0, max_value=64),
    meter=st.integers(min_value=1, max_value=8),
    division=st.integers(min_value=1, max_value=8),
    data=st.data(),
)
def test_beats_lies_on_the_grid_and_its_downbeats_are_the_grids(gaps, first, meter, division, data):
    # beat times in 64ths of a second, which floats hold exactly
    steps = [first]
    for gap in gaps:
        steps.append(steps[-1] + gap)
    first_downbeat = data.draw(st.integers(min_value=0, max_value=min(meter, len(steps)) - 1))
    grid = Grid(beats=tuple(step / 64 for step in steps), meter=meter, first_downbeat=first_downbeat)
    events = lib.beats(grid=grid, division=division)
    assert len(events) == (len(steps) - 1) * division + 1
    assert times(events)[::division] == list(grid.beats)  # the whole beats are the grid's own times
    assert all(left < right for left, right in zip(times(events), times(events)[1:], strict=False))  # strictly later
    assert [event.t for event in events if event.label == "downbeat"] == list(grid.downbeats)
    assert [event.t for event in events if event.label in ("beat", "downbeat")] == list(grid.beats)
    assert all(label == "sub" for index, label in enumerate(labels(events)) if index % division)
