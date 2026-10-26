import pytest
from hypothesis import given, settings
from hypothesis import strategies as st

from bullterrier_control import Event, lib

# Times are multiples of 1/64, which floats hold exactly: a difference is exact, so "at least gap" is decided exactly.


def times(events):
    return [event.t for event in events]


@pytest.mark.parametrize(
    ("values", "given", "expected"),
    [
        pytest.param(
            {"gap": 0.25},
            [0.5, 0.6, 0.76, 1.0, 1.05],
            [0.5, 0.76, 1.05],
            id="0.6 is 0.1 after 0.5; 0.76 is 0.26 after it; 1.0 is 0.24 after 0.76; 1.05 is 0.29 after",
        ),
        pytest.param(
            {"gap": 0.25},
            [0.5, 0.75, 1.0, 1.25],
            [0.5, 0.75, 1.0, 1.25],
            id="a distance of exactly the gap keeps the event",
        ),
        pytest.param({"gap": 0.25}, [0.5, 0.7499], [0.5], id="a hair under the gap drops it"),
        pytest.param(
            {"gap": 0.25},
            [0.5, 0.6, 0.7, 0.8, 0.9],
            [0.5, 0.8],
            id="the distance is from the last kept event, not the last event seen: 0.7 is 0.2 after 0.5",
        ),
        pytest.param({"gap": 0}, [1.0, 1.0, 1.5], [1.0, 1.0, 1.5], id="a gap of 0 keeps all, same time included"),
        pytest.param({"gap": 0.25}, [1.0, 1.0], [1.0], id="two at the same time: the second is 0 after the first"),
        pytest.param({}, [0.5, 0.55, 0.625, 0.75], [0.5, 0.625, 0.75], id="the default gap is 0.1 s"),
        pytest.param({"gap": 3600}, [0.0, 3599.0, 3600.0], [0.0, 3600.0], id="the longest gap"),
        pytest.param(
            {"gap": 0.2}, [0.1, 0.3], [0.1, 0.3], id="decimals: 0.3 - 0.1 is 0.19999999999999998, which is the gap"
        ),
        pytest.param({"gap": 0.2}, [1.1, 1.3], [1.1, 1.3], id="decimals: 1.3 - 1.1 is a hair under the gap"),
        pytest.param({"gap": 0.1}, [0.1, 0.2, 0.3], [0.1, 0.2, 0.3], id="decimals: three events a tenth apart"),
        pytest.param({"gap": 0.2}, [0.5, 0.699999], [0.5], id="decimals: 1e-6 short of the gap is dropped"),
        pytest.param({"gap": 0.2}, [0.5, 0.7000001], [0.5, 0.7000001], id="decimals: 1e-7 past the gap is kept"),
    ],
)
def test_thin_keeps_an_event_when_it_is_at_least_gap_after_the_last_one_kept(values, given, expected):
    assert times(lib.thin(events=[Event(t=t) for t in given], **values)) == expected


def test_thin_keeps_the_first_of_events_at_the_same_time_and_every_field_of_what_it_keeps():
    loud, soft = Event(t=1.0, strength=0.9, label="a", pitch=40), Event(t=1.0, strength=0.2, label="b")
    assert lib.thin(events=[loud, soft], gap=0.5) == (loud,)
    assert lib.thin(events=[soft, loud], gap=0.5) == (soft,)


def test_thin_works_in_time_order_whatever_the_order_given():
    shuffled = [Event(t=t) for t in (1.05, 0.5, 1.0, 0.76, 0.6)]
    assert times(lib.thin(events=shuffled, gap=0.25)) == [0.5, 0.76, 1.05]


def test_thin_of_no_events_is_no_events():
    assert lib.thin(events=[]) == ()


@settings(max_examples=200)
@given(
    steps=st.lists(st.integers(min_value=0, max_value=256), max_size=30),
    gap_steps=st.integers(min_value=0, max_value=128),
)
def test_thin_leaves_events_a_gap_apart_and_drops_only_those_that_were_too_close(steps, gap_steps):
    # the label is the position, so events at equal times can be told apart
    events = [Event(t=step / 64, label=str(index)) for index, step in enumerate(steps)]
    gap = gap_steps / 64
    ordered = sorted(events, key=lambda event: event.t)
    kept = lib.thin(events=events, gap=gap)
    kept_labels = {event.label for event in kept}
    dropped = [event for event in ordered if event.label not in kept_labels]
    # the first event is always kept
    assert kept[:1] == tuple(ordered[:1])
    # kept events are a gap apart: each against every earlier one kept
    assert all(later.t - earlier.t >= gap for index, later in enumerate(kept) for earlier in kept[:index])
    # a dropped event is closer than gap to the last event kept before it
    assert all(
        event.t - max((k for k in kept if ordered.index(k) < ordered.index(event)), key=lambda k: k.t).t < gap
        for event in dropped
    )
