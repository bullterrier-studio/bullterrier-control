import pytest

from bullterrier_control import Event, lib

# Most times and offsets are multiples of 1/8, which floats hold exactly; the decimal ones show a landing at 0 decided
# to within float rounding (1e-9 s) and no looser.
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


def spans(events):
    """Each event's start and duration, flat, to compare to a hand-worked list."""
    return [number for event in events for number in (event.t, event.duration)]


@pytest.mark.parametrize(
    ("event", "offset", "expected"),
    [
        pytest.param(
            Event(t=0.125, duration=1.0), -0.375, [0.0, 0.75], id="a span cut at 0 keeps the rest: 1.125 - 0.375"
        ),
        pytest.param(Event(t=0.25, duration=0.5), -0.5, [0.0, 0.25], id="starts at -0.25, ends at 0.25: 0.25 left"),
        pytest.param(Event(t=0.5, duration=1.0), -0.5, [0.0, 1.0], id="starts exactly at 0: whole, not cut"),
        pytest.param(Event(t=0.5, duration=1.0), 0.25, [0.75, 1.0], id="moved later: the duration is unchanged"),
        pytest.param(Event(t=0.0, duration=LARGEST), -3600, [0.0, LARGEST], id="a span of the largest length"),
        pytest.param(Event(t=LARGEST, duration=LARGEST), 3600, [LARGEST, LARGEST], id="the largest time and span"),
        pytest.param(
            Event(t=0.1, duration=0.200001), -0.3, [0.0, 1e-6], id="decimals: a span ending 1e-6 after 0 is kept"
        ),
        pytest.param(
            Event(t=0.1, duration=0.7), -0.3, [0.0, 0.5], id="decimals: starts at -0.2, ends at 0.5, 0.5 left"
        ),
        pytest.param(
            Event(t=0.3, duration=1.4), -0.7, [0.0, 1.0], id="decimals: starts at -0.4, ends at 1.0, 1.0 left"
        ),
    ],
)
def test_shift_cuts_a_span_that_starts_before_zero_at_zero_and_keeps_the_rest(event, offset, expected):
    assert spans(lib.shift(events=[event], offset=offset)) == pytest.approx(expected, rel=0, abs=1e-12)


@pytest.mark.parametrize(
    ("event", "offset"),
    [
        pytest.param(Event(t=0.25, duration=0.5), -0.75, id="ends exactly at 0"),
        pytest.param(Event(t=0.25, duration=0.25), -1.0, id="ends before 0"),
        pytest.param(Event(t=0.1, duration=0.2), -0.3, id="decimals: 0.1 + 0.2 - 0.3 is 3e-17, which is 0"),
        pytest.param(Event(t=0.3, duration=0.4), -0.7, id="decimals: ends at 0 within rounding"),
        pytest.param(Event(t=1.1, duration=0.2), -1.3, id="decimals: ends at 0 within rounding, below it"),
        pytest.param(Event(t=0.1, duration=0.199999), -0.3, id="decimals: ends 1e-6 before 0"),
    ],
)
def test_shift_drops_a_span_that_ends_at_or_before_zero(event, offset):
    assert lib.shift(events=[event], offset=offset) == ()


@pytest.mark.parametrize(
    ("event", "offset", "expected"),
    [
        pytest.param(Event(t=0.7999999999999999), -0.8, [0.0], id="a moment 1e-16 before 0 (0.1 + 0.7 - 0.8) is at 0"),
        pytest.param(Event(t=0.3), -0.3, [0.0], id="a decimal moment landing at 0"),
        pytest.param(Event(t=0.5), -0.499999, [1e-6], id="a moment 1e-6 after 0 is kept"),
    ],
)
def test_shift_keeps_a_moment_landing_at_zero_within_rounding(event, offset, expected):
    moved = times(lib.shift(events=[event], offset=offset))
    assert moved == pytest.approx(expected, rel=0, abs=1e-12)
    assert min(moved) >= 0.0  # exactly: an event cannot start before 0


def test_shift_drops_a_moment_a_millionth_before_zero():
    assert lib.shift(events=[Event(t=0.5)], offset=-0.500001) == ()


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
