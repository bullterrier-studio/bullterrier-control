import numpy as np
import pytest

from bullterrier_control import Event, FormatError, events


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
    "optional fields written as null": {"t": 1, "duration": None, "label": None, "pitch": None},
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
def test_an_invalid_event_is_refused_naming_the_field(data, named, reports):
    with pytest.raises(FormatError) as refused:
        Event.parse(data)
    reports(refused.value, named)


def moments(made):
    return [(event.t, event.strength, event.label, event.duration) for event in made]


EVENTS = {
    "labelled times": (([0.5, 1.0], {"label": "kick"}), [(0.5, 1.0, "kick", None), (1.0, 1.0, "kick", None)]),
    "a strength each, in order": (
        ([0.5, 1.0], {"strength": [0.2, 0.9]}),
        [(0.5, 0.2, None, None), (1.0, 0.9, None, None)],
    ),
    "one strength for all": (([0.5, 1.0], {"strength": 0.4}), [(0.5, 0.4, None, None), (1.0, 0.4, None, None)]),
    "a duration for all": (([2.0], {"duration": 0.25}), [(2.0, 1.0, None, 0.25)]),
    "times in the order given": (([3.0, 1.0], {}), [(3.0, 1.0, None, None), (1.0, 1.0, None, None)]),
    "an array of times": (
        (np.array([0.5, 1.0]), {"strength": np.array([0.1, 0.2])}),
        [(0.5, 0.1, None, None), (1.0, 0.2, None, None)],
    ),
    "no times": (([], {}), []),
}


@pytest.mark.parametrize(("given", "expected"), EVENTS.values(), ids=EVENTS.keys())
def test_times_make_events_in_one_call(given, expected):
    times, options = given
    made = events(times, **options)
    assert isinstance(made, tuple)
    assert moments(made) == expected


EVENTS_REFUSED = {
    "fewer strengths than times": (([0.5, 1.0], {"strength": [0.2]}), "strength"),
    "more strengths than times": (([0.5], {"strength": [0.2, 0.3]}), "strength"),
    "a strength above one": (([0.5], {"strength": 1.5}), "strength"),
    "a negative time": (([-0.5], {}), "t"),
    "an array of bools as times": ((np.array([True, False]), {}), "t"),
    "an array of bools as strengths": ((np.array([0.5, 1.0]), {"strength": np.array([True, False])}), "strength"),
    "a zero duration": (([0.5], {"duration": 0.0}), "duration"),
    "an empty label": (([0.5], {"label": ""}), "label"),
}


@pytest.mark.parametrize(("given", "named"), EVENTS_REFUSED.values(), ids=EVENTS_REFUSED.keys())
def test_times_that_break_the_event_rules_are_refused_naming_the_field(given, named, reports):
    times, options = given
    with pytest.raises(FormatError) as refused:
        events(times, **options)
    reports(refused.value, named)


EVENTS_REFUSED_AT = {
    "a bad time in the second event": (([0.5, -1.0], {}), "events[1].t"),
    "a bad time in the third event": (([0.5, 1.0, -2.0], {}), "events[2].t"),
    "a bad strength in the second event": (([0.5, 1.0], {"strength": [0.5, 2.0]}), "events[1].strength"),
}


@pytest.mark.parametrize(("given", "path"), EVENTS_REFUSED_AT.values(), ids=EVENTS_REFUSED_AT.keys())
def test_the_event_that_breaks_a_rule_is_named_by_its_index(given, path, reports):
    times, options = given
    with pytest.raises(FormatError) as refused:
        events(times, **options)
    reports(refused.value, path)


NOTE = Event(0.5, strength=0.7, duration=0.25, label="lead", pitch=69.5)

REPLACED = {
    "a new time": ({"t": 2}, (2.0, 0.7, 0.25, "lead", 69.5)),
    "a new strength and label": ({"strength": 0.1, "label": "pad"}, (0.5, 0.1, 0.25, "pad", 69.5)),
    "the duration removed": ({"duration": None}, (0.5, 0.7, None, "lead", 69.5)),
    "nothing": ({}, (0.5, 0.7, 0.25, "lead", 69.5)),
}


@pytest.mark.parametrize(("changes", "fields"), REPLACED.values(), ids=REPLACED.keys())
def test_replace_gives_the_event_with_those_fields_changed_and_leaves_the_original(changes, fields):
    changed = NOTE.replace(**changes)
    assert (changed.t, changed.strength, changed.duration, changed.label, changed.pitch) == fields
    assert (NOTE.t, NOTE.strength, NOTE.duration, NOTE.label, NOTE.pitch) == (0.5, 0.7, 0.25, "lead", 69.5)


REPLACE_REFUSED = {
    "a strength above one": ({"strength": 2}, "strength"),
    "a negative time": ({"t": -1}, "t"),
    "a bool time": ({"t": True}, "t"),
    "an unknown field": ({"colour": "red"}, "colour"),
}


@pytest.mark.parametrize(("changes", "named"), REPLACE_REFUSED.values(), ids=REPLACE_REFUSED.keys())
def test_replace_checks_the_changed_event_again(changes, named, reports):
    with pytest.raises(FormatError) as refused:
        NOTE.replace(**changes)
    reports(refused.value, named)


AS_DICTS = {
    "a moment": (Event(2), {"t": 2.0, "strength": 1.0}),
    "a note": (NOTE, {"t": 0.5, "strength": 0.7, "duration": 0.25, "label": "lead", "pitch": 69.5}),
}


@pytest.mark.parametrize(("event", "data"), AS_DICTS.values(), ids=AS_DICTS.keys())
def test_to_dict_is_the_event_as_a_file_holds_it_and_parses_back(event, data):
    assert event.to_dict() == data
    assert Event.parse(event.to_dict()) == event
