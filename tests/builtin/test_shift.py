import pytest

from bullterrier_control import Event, lib

# Times and offsets are multiples of 1/8 below, which floats hold exactly, so a landing at 0 is decided exactly.
LARGEST = 1.7976931348623157e308


def moments(*times):
    return [Event(t=t, strength=0.5) for t in times]


def times(events):
    return [event.t for event in events]


@pytest.mark.parametrize(
    ("offset", "given", "expected"),
    [
        pytest.param(0.25, [0.0, 0.5, 1.0], [0.25, 0.75, 1.25], id="a positive offset moves every moment later"),
        pytest.param(
            -0.25, [0.1, 0.25, 1.0], [0.0, 0.75], id="0.1 would land before 0 and is dropped; 0.25 lands at 0"
        ),
        pytest.param(-0.25, [0.125, 0.25, 0.375], [0.0, 0.125], id="a moment landing exactly at 0 is kept"),
        pytest.param(-3600, [3599.0, 3600.0, 3601.0], [0.0, 1.0], id="the largest lead"),
        pytest.param(3600, [0.0, 1.0], [3600.0, 3601.0], id="the largest lag"),
    ],
)
def test_shift_moves_each_moment_by_the_offset_and_drops_those_that_land_before_zero(offset, given, expected):
    assert times(lib.shift(events=moments(*given), offset=offset)) == expected


def test_shift_by_the_default_changes_nothing():
    given = [
        Event(t=0.1),
        Event(t=0.5, strength=0.3, duration=0.25, label="lead", pitch=64.5),
        Event(t=0.0, duration=2.0),
    ]
    assert lib.shift(events=given) == (given[2], given[0], given[1])


@pytest.mark.parametrize(
    ("event", "offset", "expected"),
    [
        pytest.param(
            Event(t=0.125, duration=1.0),
            -0.375,
            Event(t=0.0, duration=0.75),
            id="a span cut at 0 keeps the rest: 1.125 - 0.375",
        ),
        pytest.param(
            Event(t=0.25, duration=0.5),
            -0.5,
            Event(t=0.0, duration=0.25),
            id="starts at -0.25, ends at 0.25: 0.25 left",
        ),
        pytest.param(Event(t=0.25, duration=0.5), -0.75, None, id="ends exactly at 0: nothing left, dropped"),
        pytest.param(Event(t=0.25, duration=0.25), -1.0, None, id="ends before 0: dropped"),
        pytest.param(
            Event(t=0.5, duration=1.0), -0.5, Event(t=0.0, duration=1.0), id="starts exactly at 0: whole, not cut"
        ),
        pytest.param(
            Event(t=0.5, duration=1.0), 0.25, Event(t=0.75, duration=1.0), id="moved later: the duration is unchanged"
        ),
        pytest.param(
            Event(t=0.0, duration=LARGEST), -3600, Event(t=0.0, duration=LARGEST), id="a span of the largest length"
        ),
        pytest.param(
            Event(t=LARGEST, duration=LARGEST), 3600, Event(t=LARGEST, duration=LARGEST), id="the largest time and span"
        ),
    ],
)
def test_shift_cuts_a_span_that_starts_before_zero_at_zero_and_drops_one_that_ends_by_then(event, offset, expected):
    shifted = lib.shift(events=[event], offset=offset)
    assert shifted == (() if expected is None else (expected,))


def test_shift_keeps_the_strength_label_and_pitch_of_a_cut_span():
    span = Event(t=0.25, strength=0.6, duration=0.5, label="riser", pitch=60)
    assert lib.shift(events=[span], offset=-0.5) == (
        Event(t=0.0, strength=0.6, duration=0.25, label="riser", pitch=60),
    )


def test_shift_keeps_events_at_the_same_time_in_the_order_given():
    first, second = Event(t=1.0, label="a"), Event(t=1.0, label="b")
    assert [event.label for event in lib.shift(events=[second, first], offset=0.5)] == ["b", "a"]


def test_shift_works_in_time_order_whatever_the_order_given():
    assert times(lib.shift(events=moments(1.0, 0.25, 0.5), offset=0.25)) == [0.5, 0.75, 1.25]


def test_shift_of_no_events_is_no_events():
    assert lib.shift(events=[], offset=1.0) == ()
