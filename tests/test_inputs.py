import pytest

from bullterrier_control import ParameterError
from bullterrier_control.inputs import INPUTS
from bullterrier_control.plugin import EASES, Keyframe


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


def test_the_points_input_describes_itself():
    assert INPUTS["points"].doc
    assert "ease" in INPUTS["points"].doc


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
