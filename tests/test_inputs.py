import pytest

from bullterrier_control import Event, Grid, ParameterError, Series, Unit, registry
from bullterrier_control.inputs import INPUTS, matching
from bullterrier_control.plugin import EASES, Constant, Keyframe, Params, Signal, source


def test_a_keyframe_has_a_linear_ease_by_default():
    frame = Keyframe.parse({"t": 2.5, "value": -0.75})
    assert (frame.t, frame.value, frame.ease) == (2.5, -0.75, "linear")


def test_a_keyframe_carries_its_time_value_and_ease():
    frame = Keyframe.parse({"t": 4, "value": 1e300, "ease": "cubic-bezier(0.1, -2, 0.9, 3)"})
    assert (frame.t, frame.value, frame.ease) == (4.0, 1e300, "cubic-bezier(0.1, -2, 0.9, 3)")


EDGES = {
    "time zero": {"t": 0, "value": 1},
    "a negative value": {"t": 1, "value": -1e300},
    "a whole number for a decimal": {"t": 1, "value": 3},
    **{f"the ease {name}": {"t": 1, "value": 0, "ease": name} for name in EASES},
    "a bezier with its y outside 0..1": {"t": 1, "value": 0, "ease": "cubic-bezier(0, -3, 1, 4)"},
}


@pytest.mark.parametrize("data", EDGES.values(), ids=EDGES.keys())
def test_the_edges_of_every_range_are_valid(data):
    Keyframe.parse(data)


INVALID = {
    "negative time": ({"t": -0.01, "value": 1}, "t"),
    "time missing": ({"value": 1}, "t"),
    "time a bool": ({"t": True, "value": 1}, "t"),
    "time a string": ({"t": "1", "value": 1}, "t"),
    "time not finite": ({"t": float("nan"), "value": 1}, "t"),
    "time infinite": ({"t": float("inf"), "value": 1}, "t"),
    "value missing": ({"t": 1}, "value"),
    "value a string": ({"t": 1, "value": "high"}, "value"),
    "value a bool": ({"t": 1, "value": False}, "value"),
    "value not finite": ({"t": 1, "value": float("-inf")}, "value"),
    "unknown ease": ({"t": 1, "value": 1, "ease": "bounce"}, "ease"),
    "ease not text": ({"t": 1, "value": 1, "ease": 2}, "ease"),
    "bezier with x outside 0..1": ({"t": 1, "value": 1, "ease": "cubic-bezier(1.5, 0, 0.5, 1)"}, "ease"),
    "unknown field": ({"t": 1, "value": 1, "colour": "red"}, "colour"),
}


@pytest.mark.parametrize(("data", "named"), INVALID.values(), ids=INVALID.keys())
def test_an_invalid_keyframe_is_refused_naming_the_field(data, named, reports):
    with pytest.raises(ParameterError) as refused:
        Keyframe.parse(data)
    reports(refused.value, named)
    assert [path for path, _ in refused.value.problems] == [named]


def test_a_bad_ease_lists_the_valid_ones():
    with pytest.raises(ParameterError, match="in-out"):
        Keyframe.parse({"t": 1, "value": 1, "ease": "bounce"})


def check_points(value):
    return INPUTS["points"].check(value)


def test_points_given_as_mappings_become_keyframes_in_a_tuple():
    given = [{"t": 0.5, "value": 2.0, "ease": "in"}, {"t": 1.5, "value": -1.0}, {"t": 4, "value": 0.25, "ease": "out"}]
    checked = check_points(given)
    assert type(checked) is tuple
    assert checked == (
        Keyframe(t=0.5, value=2.0, ease="in"),
        Keyframe(t=1.5, value=-1.0, ease="linear"),
        Keyframe(t=4.0, value=0.25, ease="out"),
    )


def test_points_may_be_a_tuple_of_keyframes_and_mappings_mixed_and_a_single_point_is_enough():
    first = Keyframe(t=1.0, value=3.0, ease="hold")
    assert check_points((first, {"t": 2.0, "value": 5.0})) == (first, Keyframe(t=2.0, value=5.0))
    assert check_points([{"t": 0, "value": 9.0}]) == (Keyframe(t=0.0, value=9.0),)


GOOD_ORDER = {
    "close together": [{"t": 1.0, "value": 0}, {"t": 1.0 + 1e-9, "value": 0}],
    "from zero": [{"t": 0, "value": 0}, {"t": 7, "value": 1}, {"t": 7.5, "value": 2}],
}


@pytest.mark.parametrize("given", GOOD_ORDER.values(), ids=GOOD_ORDER.keys())
def test_times_that_strictly_increase_are_valid(given):
    assert [frame.t for frame in check_points(given)] == [point["t"] for point in given]


BAD = {
    "empty list": ([], ["points"]),
    "empty tuple": ((), ["points"]),
    "a number": (5, ["points"]),
    "a string": ("0:1", ["points"]),
    "nothing": (None, ["points"]),
    "a set of keyframes (no order)": ({Keyframe(t=0.0, value=1.0)}, ["points"]),
    "a mapping": ({"t": 0, "value": 1}, ["points"]),
    "a keyframe, not a sequence of them": (Keyframe(t=0.0, value=1.0), ["points"]),
    "an item that is not a mapping": ([{"t": 0, "value": 1}, 3], ["points[1]"]),
    "unsorted": ([{"t": 0, "value": 1}, {"t": 5, "value": 1}, {"t": 2, "value": 1}], ["points[2].t"]),
    "duplicate time": ([{"t": 0, "value": 1}, {"t": 1, "value": 1}, {"t": 1, "value": 2}], ["points[2].t"]),
    "starts with a repeat": ([{"t": 3, "value": 1}, {"t": 3, "value": 1}], ["points[1].t"]),
    "a bad ease at index 1": ([{"t": 0, "value": 1}, {"t": 1, "value": 1, "ease": "wobble"}], ["points[1].ease"]),
    "an unknown field at index 1": ([{"t": 0, "value": 1}, {"t": 1, "value": 1, "x": 1}], ["points[1].x"]),
    "a negative time at index 0": ([{"t": -1, "value": 1}, {"t": 1, "value": 1}], ["points[0].t"]),
    "every bad item is listed": (
        [{"t": 0, "value": "a"}, {"t": 1, "value": 1, "ease": "wobble"}],
        ["points[0].value", "points[1].ease"],
    ),
}


@pytest.mark.parametrize(("given", "named"), BAD.values(), ids=BAD.keys())
def test_bad_points_are_refused_naming_each_path(given, named, reports):
    with pytest.raises(ParameterError) as refused:
        check_points(given)
    for path in named:
        reports(refused.value, path)
    assert [path for path, _ in refused.value.problems] == named


class Nothing(Params):
    """No parameters."""


@pytest.fixture
def listen(throwaway_registry):
    """A throwaway source that takes `events`: calling it returns the events the function received."""
    received = []

    @source(params=Nothing)
    def listener(params: Nothing, *, events: tuple[Event, ...]) -> Signal:
        """Takes events."""
        received.append(events)
        return Constant(0.0)

    def hear(given):
        listener(events=given)
        return received[-1]

    return hear


KICK, SNARE, HAT = Event(t=0.5, strength=0.3), Event(t=1.0, strength=0.8), Event(t=2.0, strength=0.45)
FIRST_OF_TWO, SECOND_OF_TWO = Event(t=1.0, strength=0.2), Event(t=1.0, strength=0.9)
NOTE = {"t": 1.25, "strength": 0.7, "duration": 0.5, "label": "snare", "pitch": 38}

GOOD_EVENTS = {
    "a list of events": ([KICK, SNARE, HAT], (KICK, SNARE, HAT)),
    "a tuple": ((KICK, SNARE), (KICK, SNARE)),
    "mappings": (
        [{"t": 0.5, "strength": 0.3}, NOTE],
        (KICK, Event(t=1.25, strength=0.7, duration=0.5, label="snare", pitch=38)),
    ),
    "mappings and events mixed": ([KICK, {"t": 1.0, "strength": 0.8}], (KICK, SNARE)),
    "defaults filled in": ([{"t": 3}], (Event(t=3.0, strength=1.0, duration=None, label=None, pitch=None),)),
    "none at all": ([], ()),
    "none in a tuple": ((), ()),
    "unsorted": ([HAT, KICK, SNARE], (KICK, SNARE, HAT)),
    "equal times keep their order, weak first": ([FIRST_OF_TWO, SECOND_OF_TWO], (FIRST_OF_TWO, SECOND_OF_TWO)),
    "equal times keep their order, strong first": ([SECOND_OF_TWO, FIRST_OF_TWO], (SECOND_OF_TWO, FIRST_OF_TWO)),
    "equal times beside an earlier one": (
        [FIRST_OF_TWO, KICK, SECOND_OF_TWO],
        (KICK, FIRST_OF_TWO, SECOND_OF_TWO),
    ),
}


@pytest.mark.parametrize(("given", "expected"), GOOD_EVENTS.values(), ids=GOOD_EVENTS.keys())
def test_events_arrive_as_a_tuple_of_events_sorted_by_time(given, expected, listen):
    heard = listen(given)
    assert type(heard) is tuple
    assert heard == expected


BAD_EVENTS = {
    "a string": ("0.5, 1.0", ["events"]),
    "a mapping": ({"t": 0.5}, ["events"]),
    "a set of events (no order)": ({KICK, SNARE}, ["events"]),
    "an event, not a sequence of them": (KICK, ["events"]),
    "nothing": (None, ["events"]),
    "an item that is not a mapping": ([KICK, 3], ["events[1]"]),
    "a strength above 1 at index 1": ([KICK, {"t": 1, "strength": 1.5}], ["events[1].strength"]),
    "a negative strength": ([{"t": 1, "strength": -0.1}], ["events[0].strength"]),
    "an unknown field at index 0": ([{"t": 1, "colour": "red"}, KICK], ["events[0].colour"]),
    "a bool time": ([{"t": True}], ["events[0].t"]),
    "a missing time": ([KICK, {"strength": 0.5}], ["events[1].t"]),
    "a negative time at index 1": ([KICK, {"t": -0.25}], ["events[1].t"]),
    "a zero duration": ([{"t": 1, "duration": 0}], ["events[0].duration"]),
    "an empty label": ([KICK, {"t": 1, "label": ""}], ["events[1].label"]),
    "a pitch above 127": ([{"t": 1, "pitch": 128}], ["events[0].pitch"]),
    "every broken item is listed": (
        [{"t": -1, "strength": 0.5}, KICK, {"t": 1, "strength": 2}, {"t": 2, "duration": 0}],
        ["events[0].t", "events[2].strength", "events[3].duration"],
    ),
}


@pytest.mark.parametrize(("given", "named"), BAD_EVENTS.values(), ids=BAD_EVENTS.keys())
def test_bad_events_are_refused_naming_each_path(given, named, listen, reports):
    with pytest.raises(ParameterError) as refused:
        listen(given)
    for path in named:
        reports(refused.value, path)
    assert [path for path, _ in refused.value.problems] == named


def test_a_refused_event_is_reported_under_the_source_not_under_a_private_model(listen):
    with pytest.raises(ParameterError) as refused:
        listen([KICK, {"t": 1, "strength": 1.5}])
    assert str(refused.value).startswith("source 'listener': events[1].strength: ")


def test_help_lists_events_as_an_input(listen):
    text = registry.get("listener").__doc__ or ""
    assert "events (required)" in text


STEADY = {"beats": [0.0, 0.5, 1.0, 1.5], "meter": 4, "first_downbeat": 0}


def check_grid(value):
    return INPUTS["grid"].check(value)


def test_a_grid_given_as_a_mapping_becomes_a_grid():
    checked = check_grid({"beats": [0.0, 0.5, 1.1], "meter": 3, "first_downbeat": 1})
    assert checked == Grid(beats=(0.0, 0.5, 1.1), meter=3, first_downbeat=1)


def test_a_grid_may_be_given_as_a_grid():
    grid = Grid(beats=(0.25, 0.75), meter=2, first_downbeat=0)
    assert check_grid(grid) == grid


BAD_GRIDS = {
    "a number": (5, ["grid"]),
    "a string": ("0.5, 1.0", ["grid"]),
    "nothing": (None, ["grid"]),
    "a list of beats, not a grid": ([0.0, 0.5, 1.0], ["grid"]),
    "meter missing": ({"beats": [0.0, 0.5], "first_downbeat": 0}, ["grid.meter"]),
    "a single beat": ({"beats": [0.0], "meter": 4, "first_downbeat": 0}, ["grid.beats"]),
    "beats not increasing": ({**STEADY, "beats": [0.0, 1.0, 0.5]}, ["grid.beats[2]"]),
    "a negative beat": ({**STEADY, "beats": [-0.5, 0.5, 1.0]}, ["grid.beats[0]"]),
    "a downbeat outside the bar": ({**STEADY, "first_downbeat": 4}, ["grid.first_downbeat"]),
    "an unknown field": ({**STEADY, "tempo": 120}, ["grid.tempo"]),
}


@pytest.mark.parametrize(("given", "named"), BAD_GRIDS.values(), ids=BAD_GRIDS.keys())
def test_a_bad_grid_is_refused_as_a_parameter_error_naming_each_path(given, named, reports):
    with pytest.raises(ParameterError) as refused:
        check_grid(given)
    for path in named:
        reports(refused.value, path)
    assert [path for path, _ in refused.value.problems] == named


def check_values(value):
    return INPUTS["values"].check(value)


GOOD_VALUES = {
    "floats in a list": ([0.25, -1.5, 3.0], (0.25, -1.5, 3.0)),
    "floats in a tuple": ((0.25, -1.5), (0.25, -1.5)),
    "whole numbers become floats": ([1, 2, -3], (1.0, 2.0, -3.0)),
    "a single value": ([0.5], (0.5,)),
    "large and small": ([1e300, -1e-300], (1e300, -1e-300)),
}


@pytest.mark.parametrize(("given", "expected"), GOOD_VALUES.values(), ids=GOOD_VALUES.keys())
def test_values_arrive_as_a_tuple_of_floats(given, expected):
    checked = check_values(given)
    assert type(checked) is tuple
    assert checked == expected
    assert all(type(value) is float for value in checked)


BAD_VALUES = {
    "an empty list": ([], ["values"]),
    "an empty tuple": ((), ["values"]),
    "a number": (5, ["values"]),
    "a string": ("1, 2, 3", ["values"]),
    "nothing": (None, ["values"]),
    "a set (no order)": ({1.0, 2.0}, ["values"]),
    "a mapping": ({"a": 1.0}, ["values"]),
    "a bool item": ([0.5, True], ["values[1]"]),
    "a string item": ([0.5, "high"], ["values[1]"]),
    "a NaN item": ([0.0, 1.0, float("nan")], ["values[2]"]),
    "an infinite item": ([float("-inf"), 1.0], ["values[0]"]),
    "an item that is a list": ([[1.0], 2.0], ["values[0]"]),
    "every bad item is listed": ([True, 1.0, "x"], ["values[0]", "values[2]"]),
}


@pytest.mark.parametrize(("given", "named"), BAD_VALUES.values(), ids=BAD_VALUES.keys())
def test_bad_values_are_refused_naming_each_path(given, named, reports):
    with pytest.raises(ParameterError) as refused:
        check_values(given)
    for path in named:
        reports(refused.value, path)
    assert [path for path, _ in refused.value.problems] == named


def check_series(value):
    return INPUTS["series"].check(value)


def test_a_series_given_as_a_mapping_becomes_a_series():
    checked = check_series({"start": 0.5, "rate": 4, "values": [0.0, 1.0], "unit": "ratio"})
    assert checked == Series(start=0.5, rate=4.0, values=(0.0, 1.0), unit=Unit.RATIO)


def test_a_series_may_be_given_as_a_series():
    series = Series(rate=2.0, values=(0.25, 0.75), unit=Unit.FRACTION, scaling="by hand")
    assert check_series(series) == series


BAD_SERIES = {
    "a number": (5, ["series"]),
    "a string": ("0.1, 0.2", ["series"]),
    "nothing": (None, ["series"]),
    "a list of values, not a series": ([0.1, 0.2], ["series"]),
    "rate missing": ({"values": [0.1], "unit": "ratio"}, ["series.rate"]),
    "rate zero": ({"rate": 0, "values": [0.1], "unit": "ratio"}, ["series.rate"]),
    "no values": ({"rate": 1, "values": [], "unit": "ratio"}, ["series.values"]),
    "a bool value": ({"rate": 1, "values": [0.1, True], "unit": "ratio"}, ["series.values[1]"]),
    "an unknown unit": ({"rate": 1, "values": [0.1], "unit": "pixels"}, ["series.unit"]),
    "a fraction without scaling": ({"rate": 1, "values": [0.1], "unit": "fraction"}, ["series.scaling"]),
    "a fraction above 1": ({"rate": 1, "values": [0.1, 1.5], "unit": "fraction", "scaling": "x"}, ["series.values[1]"]),
    "a negative start": ({"start": -1, "rate": 1, "values": [0.1], "unit": "ratio"}, ["series.start"]),
    "an unknown field": ({"rate": 1, "values": [0.1], "unit": "ratio", "gain": 2}, ["series.gain"]),
}


@pytest.mark.parametrize(("given", "named"), BAD_SERIES.values(), ids=BAD_SERIES.keys())
def test_a_bad_series_is_refused_as_a_parameter_error_naming_each_path(given, named, reports):
    with pytest.raises(ParameterError) as refused:
        check_series(given)
    for path in named:
        reports(refused.value, path)
    assert [path for path, _ in refused.value.problems] == named


LABELLED = (
    Event(t=0.5, label="kick"),
    Event(t=0.7, label="snare"),
    Event(t=0.9),
    Event(t=1.1, label="kick", strength=0.4),
)


@pytest.mark.parametrize(
    ("label", "times"),
    [
        pytest.param("", [0.5, 0.7, 0.9, 1.1], id="no label: every event, the unlabelled one too"),
        pytest.param("kick", [0.5, 1.1], id="a label: only the events carrying it"),
        pytest.param("snare", [0.7], id="another label"),
        pytest.param("hat", [], id="a label nothing has: none"),
    ],
)
def test_matching_keeps_the_events_with_the_label_in_their_order(label, times):
    kept = matching(LABELLED, label)
    assert type(kept) is tuple
    assert [event.t for event in kept] == times


def test_matching_keeps_every_other_field_of_the_events():
    assert matching(LABELLED, "kick")[1] == Event(t=1.1, label="kick", strength=0.4)
