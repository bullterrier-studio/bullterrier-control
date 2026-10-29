import numpy as np
import pytest
from hypothesis import given, settings
from hypothesis import strategies as st

from bullterrier_control import Event, ParameterError, lib

# the input at time t is t + 2, so a held value names the time it was read at (2 at time 0, 12 at 10 s)
CLOCK = lib.keyframes(points=[{"t": 0, "value": 2}, {"t": 10, "value": 12}])
EVENTS = [Event(t=0.5), Event(t=1.2), Event(t=3.0)]


@pytest.mark.parametrize(
    ("time", "expected"),
    [
        pytest.param(0.0, 2.0, id="before the first event: the input at time 0"),
        pytest.param(float(np.nextafter(0.5, 0)), 2.0, id="the instant before the first event"),
        pytest.param(0.5, 2.5, id="at an event: already its value"),
        pytest.param(1.0, 2.5, id="between events: the earlier one holds"),
        pytest.param(1.2, 3.2, id="at the second event"),
        pytest.param(2.9, 3.2, id="the second event still holds"),
        pytest.param(3.0, 5.0, id="at the last event"),
        pytest.param(9.0, 5.0, id="after the last event it holds for good"),
    ],
)
def test_the_value_is_the_input_at_the_time_of_the_latest_event_at_or_before_the_read(time, expected):
    assert lib.hold(CLOCK, events=EVENTS).sample([time]).tolist() == pytest.approx([expected], abs=1e-12)


def test_with_no_events_every_read_is_the_input_at_time_zero_not_zero():
    assert lib.hold(CLOCK, events=[]).sample([0.0, 1.0, 50.0]).tolist() == [2.0, 2.0, 2.0]


def test_an_event_at_time_zero_holds_the_input_at_zero_from_the_start():
    assert lib.hold(CLOCK, events=[Event(t=0.0)]).sample([0.0, 4.0]).tolist() == [2.0, 2.0]


def test_events_at_the_same_time_read_the_same_value_as_one():
    twice = lib.hold(CLOCK, events=[Event(t=1.0), Event(t=1.0), Event(t=4.0)])
    reads = [0.9, 1.0, 3.9, 4.0]
    assert twice.sample(reads).tolist() == pytest.approx([2.0, 3.0, 3.0, 6.0], abs=1e-12)


def test_the_order_strength_duration_label_and_pitch_of_the_events_change_nothing():
    plain = lib.hold(CLOCK, events=EVENTS)
    dressed = lib.hold(
        CLOCK,
        events=[
            Event(t=3.0, strength=0.1, duration=2.0, label="drop", pitch=60),
            Event(t=0.5, strength=0.9),
            Event(t=1.2, strength=0.0, label="kick"),
        ],
    )
    times = np.linspace(0, 6, 61)
    assert dressed.sample(times).tolist() == plain.sample(times).tolist()


def test_a_moving_input_is_read_only_at_the_events():
    wave = lib.lfo(shape="sine", rate=0.7, phase=0.3)
    held = lib.hold(wave, events=EVENTS)
    assert held.sample([2.0]).tolist() == wave.sample([1.2]).tolist()  # not the wave's value at 2.0
    assert wave.sample([2.0]).tolist() != wave.sample([1.2]).tolist()


def test_events_are_checked_like_every_events_input():
    with pytest.raises(ParameterError, match="events"):
        lib.hold(CLOCK, events=[{"t": -1}])
    with pytest.raises(TypeError, match="events"):
        lib.hold(CLOCK)  # type: ignore[call-arg]  # left out on purpose: refused at run time too


@settings(max_examples=200)
@given(
    times=st.lists(st.floats(min_value=0, max_value=10), max_size=8),
    reads=st.lists(st.floats(min_value=0, max_value=10), min_size=1, max_size=12),
)
def test_it_agrees_with_a_plain_loop_over_the_events(times, reads):
    # reads at, and a hair before, every event as well: the inclusive edge is where an off-by-one hides
    reads = [*reads, *times, *(float(np.nextafter(t, 0)) for t in times)]
    expected = []
    for read in reads:
        latest = 0.0  # the input at time 0 until an event has happened
        for time in times:
            if time <= read:
                latest = max(latest, time)
        expected.append(latest + 2)
    held = lib.hold(CLOCK, events=[Event(t=t) for t in times])
    assert held.sample(reads).tolist() == pytest.approx(expected, abs=1e-9)
