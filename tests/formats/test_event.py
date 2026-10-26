import pytest

from bullterrier_control import Event, FormatError


def test_a_moment_has_full_strength_and_no_span_by_default():
    event = Event.parse({"t": 2})
    assert event.t == 2.0
    assert event.strength == 1.0
    assert event.duration is None
    assert event.label is None
    assert event.pitch is None


def test_a_note_carries_its_span_label_and_pitch():
    event = Event.parse({"t": 0.5, "strength": 0.7, "duration": 0.25, "label": "lead", "pitch": 69.5})
    assert (event.t, event.strength, event.duration, event.label, event.pitch) == (0.5, 0.7, 0.25, "lead", 69.5)


EDGES = {
    "time zero": {"t": 0},
    "strength zero": {"t": 1, "strength": 0},
    "strength one": {"t": 1, "strength": 1},
    "lowest pitch": {"t": 1, "pitch": 0},
    "highest pitch": {"t": 1, "pitch": 127},
    "a very short span": {"t": 1, "duration": 1e-9},
}


@pytest.mark.parametrize("data", EDGES.values(), ids=EDGES.keys())
def test_the_edges_of_every_range_are_valid(data):
    Event.parse(data)


INVALID = {
    "negative time": ({"t": -0.01}, "t"),
    "time missing": ({"strength": 1}, "t"),
    "time a bool": ({"t": False}, "t"),
    "time a string": ({"t": "1.0"}, "t"),
    "time not finite": ({"t": float("nan")}, "t"),
    "strength above one": ({"t": 1, "strength": 1.2}, "strength"),
    "strength below zero": ({"t": 1, "strength": -0.1}, "strength"),
    "zero duration": ({"t": 1, "duration": 0}, "duration"),
    "negative duration": ({"t": 1, "duration": -1}, "duration"),
    "empty label": ({"t": 1, "label": ""}, "label"),
    "pitch above 127": ({"t": 1, "pitch": 128}, "pitch"),
    "pitch below 0": ({"t": 1, "pitch": -1}, "pitch"),
    "unknown key": ({"t": 1, "time": 1}, "time"),
}


@pytest.mark.parametrize(("data", "named"), INVALID.values(), ids=INVALID.keys())
def test_an_invalid_event_is_refused_naming_the_field(data, named):
    with pytest.raises(FormatError, match=named):
        Event.parse(data)
