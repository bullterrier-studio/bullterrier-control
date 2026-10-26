import pytest
from hypothesis import given, settings
from hypothesis import strategies as st

from bullterrier_control import Event, Grid, ParameterError, lib

# a beat every half second, positions 0 to 4
STEADY = Grid(beats=(0.0, 0.5, 1.0, 1.5, 2.0), meter=4, first_downbeat=1)
# the first beat is a second long, the second half a second
SLOWING = Grid(beats=(0.0, 1.0, 1.5), meter=2, first_downbeat=0)
# beats start at 0.3 s: before them the grid continues backwards, one 0.5 s beat per position
LATE = Grid(beats=(0.3, 0.8), meter=1, first_downbeat=0)
LARGEST = 1.7976931348623157e308


def snapped(grid, given, **values):
    return [event.t for event in lib.quantize(events=[Event(t=t) for t in given], grid=grid, **values)]


@pytest.mark.parametrize(
    ("given", "expected"),
    [
        pytest.param([0.2], [0.0], id="0.2 is 0.4 of a beat: down to the beat at 0"),
        pytest.param([0.3], [0.5], id="0.3 is 0.6 of a beat: up to the beat at 0.5"),
        pytest.param([0.74], [0.5], id="0.74 is 1.48 beats: down to 1"),
        pytest.param([0.25], [0.5], id="exactly halfway between two beats goes to the later one"),
        pytest.param([0.5, 1.0], [0.5, 1.0], id="an event on a beat stays"),
        pytest.param([2.3], [2.5], id="after the last beat the last beat length goes on: 4.6 beats snaps to 5"),
        pytest.param([2.2], [2.0], id="after the last beat: 4.4 beats snaps to 4"),
        pytest.param([0.0], [0.0], id="the very start"),
    ],
)
def test_quantize_moves_each_event_to_the_nearest_beat_and_halfway_to_the_later_one(given, expected):
    assert snapped(STEADY, given) == expected


@pytest.mark.parametrize(
    ("given", "expected"),
    [
        pytest.param([0.3], [0.25], id="0.3 is 0.6 of a beat, 1.2 quarters: down to 1 quarter"),
        pytest.param([0.125], [0.25], id="exactly halfway between two quarters goes to the later one"),
        pytest.param([0.12], [0.0], id="just under halfway goes to the earlier one"),
        pytest.param([0.4], [0.5], id="0.4 is 1.6 quarters: up to 2 quarters"),
    ],
)
def test_quantize_with_a_division_snaps_to_that_many_positions_per_beat(given, expected):
    assert snapped(STEADY, given, division=2) == expected


def test_quantize_with_the_largest_division_snaps_to_a_sixty_fourth_of_a_beat():
    # 0.5 s per beat: a position every 1/128 s; 0.1 s is 12.8 positions, so 13
    assert snapped(STEADY, [0.1], division=64) == [13 / 128]


@pytest.mark.parametrize(
    ("given", "expected"),
    [
        pytest.param([0.01], [0.3], id="the nearest position, at -0.2 s, is before 0: the next one up, the first beat"),
        pytest.param([0.0], [0.3], id="time 0 itself: the position at -0.2 s is before 0, so the next one"),
        pytest.param([0.1], [0.3], id="the nearest position is the first beat"),
        pytest.param([0.6], [0.8], id="between the beats: 0.6 is 0.6 of the beat, up to the second"),
        pytest.param([1.1], [1.3], id="after the last beat: 1.6 beats snaps to 2"),
    ],
)
def test_quantize_never_moves_an_event_to_before_zero_but_to_the_next_position_up(given, expected):
    assert snapped(LATE, given) == pytest.approx(expected, abs=1e-12)


def test_quantize_steps_up_by_one_division_when_the_nearest_position_is_before_zero_and_not_the_first_beat():
    # beats at 0.7 and 1.2, positions every quarter second: ..., -0.05, 0.2, 0.45, ...; 0.01 is 0.06 from -0.05 and 0.19
    # from 0.2, so -0.05 is the nearest and the next position up is 0.2
    late = Grid(beats=(0.7, 1.2), meter=1, first_downbeat=0)
    assert snapped(late, [0.01], division=2) == pytest.approx([0.2], abs=1e-12)


def test_quantize_uses_the_grids_continuation_before_the_first_beat_when_that_is_still_after_zero():
    # beats at 1.0 and 1.5: positions before the first are at 0.5 s, 0.0 s; the one at -0.5 s is not reachable
    early = Grid(beats=(1.0, 1.5), meter=1, first_downbeat=0)
    assert snapped(early, [0.0, 0.1, 0.3, 0.74, 0.75]) == [0.0, 0.0, 0.5, 0.5, 1.0]


def test_quantize_snaps_in_beats_on_a_grid_whose_tempo_changes():
    # beats at 0, 1.0 and 1.5 s: 0.4 is 0.4 beats, 1.2 is 1.4 beats, 1.3 is 1.6 beats
    assert snapped(SLOWING, [0.4, 0.6, 1.2, 1.3]) == [0.0, 1.0, 1.0, 1.5]


def test_quantize_keeps_events_that_snap_to_one_time_in_the_order_given():
    first, second, third = (Event(t=t, label=name) for t, name in ((0.2, "a"), (0.1, "b"), (0.3, "c")))
    kept = lib.quantize(events=[first, second, third], grid=STEADY)
    assert [(event.t, event.label) for event in kept] == [(0.0, "b"), (0.0, "a"), (0.5, "c")]


def test_quantize_keeps_the_strength_duration_label_and_pitch():
    note = Event(t=0.3, strength=0.6, duration=0.25, label="lead", pitch=64.5)
    assert lib.quantize(events=[note], grid=STEADY) == (
        Event(t=0.5, strength=0.6, duration=0.25, label="lead", pitch=64.5),
    )


def test_quantize_of_no_events_is_no_events():
    assert lib.quantize(events=[], grid=STEADY) == ()


def test_quantize_needs_a_grid():
    with pytest.raises(TypeError, match="grid"):
        lib.quantize(events=[])


def test_quantize_far_from_the_beats_still_gives_a_finite_time():
    assert snapped(STEADY, [1e300]) == pytest.approx([1e300], rel=1e-12)


@pytest.mark.parametrize(
    ("time", "division"),
    [
        pytest.param(LARGEST, 1, id="a time whose beat position overflows"),
        pytest.param(1e307, 64, id="a position that overflows once multiplied by the division"),
    ],
)
def test_quantize_refuses_a_time_too_far_from_the_beats_to_snap_naming_the_events(time, division, reports):
    with pytest.raises(ParameterError) as refused:
        lib.quantize(events=[Event(t=time)], grid=STEADY, division=division)
    reports(refused.value, "events")


@settings(max_examples=200)
@given(
    steps=st.lists(st.integers(min_value=0, max_value=512), max_size=20),
    division=st.sampled_from([1, 2, 4, 8]),
)
def test_quantize_lands_on_a_position_within_half_a_step_and_never_reorders(steps, division):
    # a steady half-second beat: a position every 0.5 / division seconds, all exact in floats for division 1, 2, 4, 8
    given = [step / 128 for step in steps]
    out = snapped(STEADY, given, division=division)
    width = 0.5 / division
    assert out == sorted(out)
    assert all(abs(after - before) <= width / 2 for before, after in zip(sorted(given), out, strict=True))
    assert all((after / width).is_integer() for after in out)
