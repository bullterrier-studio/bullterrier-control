"""The extension's inputs (`Keyframe`, `check_input`, `input_docs`): the core's rules for the data a source takes
besides its parameters, read from Python objects.

An architecture test of the extension's own interface. Every expected path and rule is worked out by hand from the
inputs' promises (`points` non-empty, `t` from 0 and strictly increasing; `events` sorted, ties in the order given;
`grid` and `series` as in a bundle; `values` non-empty finite numbers), not taken from the core.
"""

import copy
import math
import subprocess
import sys
from collections import OrderedDict, UserDict
from collections.abc import Iterator, Mapping
from types import MappingProxyType

import numpy as np
import pytest

from bullterrier_control import Event, Grid, Series, Unit
from bullterrier_control._native import _core  # noqa: PLC2701  architecture: the boundary the inputs are read through
from bullterrier_control.errors import ParameterError

Keyframe = _core.Keyframe
LONE = "\ud800"  # a lone surrogate: a str that cannot be written as UTF-8


def assert_lists(refused: pytest.ExceptionInfo[ParameterError], expected: list[tuple[str, str]]) -> None:
    """The error lists exactly the paths of `expected`, in order, each rule holding its part."""
    problems = refused.value.problems
    assert [path for path, _ in problems] == [path for path, _ in expected], problems
    for (_, rule), (_, part) in zip(problems, expected, strict=True):
        assert part in rule, problems


# ---- Keyframe ----


def test_a_keyframe_is_made_from_keywords_and_carries_its_fields():
    frame = Keyframe(t=2.5, value=-0.75, ease="in-out")
    assert (frame.t, frame.value, frame.ease) == (2.5, -0.75, "in-out")
    assert (type(frame.t), type(frame.value)) == (float, float)


def test_a_keyframe_is_linear_unless_told_otherwise():
    assert Keyframe(t=1, value=3).ease == "linear"
    assert Keyframe.parse({"t": 1, "value": 3}).ease == "linear"


def test_a_keyframe_takes_keywords_only():
    with pytest.raises(TypeError):
        Keyframe(1.0, 2.0)  # type: ignore[call-arg]  # the call this test makes on purpose


@pytest.mark.parametrize(
    ("ease", "named"),
    [
        *[pytest.param(name, name, id=f"the name {name}") for name in _core.EASES],
        pytest.param("cubic-bezier(0.1, -2, 0.9, 3)", "cubic-bezier(0.1, -2, 0.9, 3)", id="a bezier as written"),
        pytest.param(" cubic-bezier( 0.25, 0.1 ,0.25, 1.0 ) ", "cubic-bezier(0.25, 0.1, 0.25, 1)", id="tidied"),
    ],
)
def test_a_keyframe_names_its_ease_as_the_text_that_parses_back_to_it(ease, named):
    assert Keyframe(t=0, value=0, ease=ease).ease == named


@pytest.mark.parametrize(
    ("data", "expected"),
    [
        pytest.param({"t": -0.01, "value": 1}, [("t", ">= 0")], id="a negative time"),
        pytest.param({"value": 1}, [("t", "required")], id="time missing"),
        pytest.param({"t": True, "value": 1}, [("t", "boolean")], id="time a bool"),
        pytest.param({"t": "1", "value": 1}, [("t", "string")], id="time a string"),
        pytest.param({"t": math.nan, "value": 1}, [("t", "finite")], id="time NaN"),
        pytest.param({"t": math.inf, "value": 1}, [("t", "finite")], id="time infinite"),
        pytest.param({"t": 1}, [("value", "required")], id="value missing"),
        pytest.param({"t": 1, "value": False}, [("value", "boolean")], id="value a bool"),
        pytest.param({"t": 1, "value": -math.inf}, [("value", "finite")], id="value infinite"),
        pytest.param({"t": 1, "value": 1, "ease": "bounce"}, [("ease", "in-out")], id="an unknown ease: names listed"),
        pytest.param({"t": 1, "value": 1, "ease": 2}, [("ease", "integer")], id="ease not text"),
        pytest.param({"t": 1, "value": 1, "ease": LONE}, [("ease", "UTF-8")], id="ease with a lone surrogate"),
        pytest.param({"t": 1, "value": 1, "ease": "cubic-bezier(1.5, 0, 0.5, 1)"}, [("ease", "0..1")],
                     id="a bezier handle beyond 0..1"),
        pytest.param({"t": 1, "value": 1, "colour": "red"}, [("colour", "unknown field")], id="an unknown field"),
        pytest.param({"t": -1, "value": math.nan, "ease": "x"}, [("t", ">= 0"), ("value", "finite"), ("ease", "x")],
                     id="every broken rule is listed"),
        pytest.param({1: 2}, [("", "must be text")], id="a key that is not text"),
        pytest.param([("t", 1), ("value", 2)], [("", "an object")], id="pairs are not a mapping"),
    ],
)  # fmt: skip
def test_a_keyframe_that_breaks_a_rule_is_refused_naming_each(data, expected):
    with pytest.raises(ParameterError) as refused:
        Keyframe.parse(data)
    assert_lists(refused, expected)
    assert str(refused.value).startswith("Keyframe: ")


@pytest.mark.parametrize(
    ("fields", "expected"),
    [
        pytest.param({"t": -1, "value": math.nan}, [("t", ">= 0"), ("value", "finite")], id="every rule broken"),
        pytest.param({"t": -1, "value": True}, [("value", "boolean")], id="a wrong type stops the reading there"),
    ],
)
def test_a_keyframe_made_from_keywords_is_held_to_the_same_rules(fields, expected):
    with pytest.raises(ParameterError) as refused:
        Keyframe(**fields)
    assert_lists(refused, expected)


@pytest.mark.parametrize(
    ("other", "equal"),
    [
        pytest.param(Keyframe(t=1.0, value=2.0, ease="in"), True, id="the same fields"),
        pytest.param(Keyframe.parse({"t": 1, "value": 2, "ease": "in"}), True, id="parsed, whole numbers"),
        pytest.param(Keyframe(t=2.0, value=2.0, ease="in"), False, id="another t"),
        pytest.param(Keyframe(t=1.0, value=1.0, ease="in"), False, id="another value"),
        pytest.param(Keyframe(t=1.0, value=2.0, ease="out"), False, id="another ease"),
        pytest.param({"t": 1.0, "value": 2.0, "ease": "in"}, False, id="its mapping is not a keyframe"),
    ],
)
def test_keyframes_are_equal_when_their_fields_are(other, equal):
    frame = Keyframe(t=1.0, value=2.0, ease="in")
    assert (frame == other) is equal
    assert (frame != other) is not equal


@pytest.mark.parametrize(
    ("first", "second"),
    [
        pytest.param(Keyframe(t=1.0, value=2.0), Keyframe(t=1, value=2), id="the same fields"),
        pytest.param(Keyframe(t=0.0, value=0.0), Keyframe(t=-0.0, value=-0.0), id="0 and -0 are the same number"),
        pytest.param(
            Keyframe(t=0, value=0, ease="cubic-bezier(0, 0, 1, 1)"),
            Keyframe(t=0, value=0, ease="cubic-bezier(-0, -0, 1, 1)"),
            id="a bezier's 0 and -0",
        ),
    ],
)
def test_equal_keyframes_hash_equally(first, second):
    assert first == second
    assert hash(first) == hash(second)
    assert len({first, second}) == 1


@pytest.mark.parametrize(
    ("frame", "shown"),
    [
        pytest.param(Keyframe(t=0, value=1), "Keyframe(t=0.0, value=1.0, ease='linear')", id="defaults"),
        pytest.param(
            Keyframe(t=1.5, value=-2e-10, ease="cubic-bezier(0.25, 0.1, 0.25, 1)"),
            "Keyframe(t=1.5, value=-2e-10, ease='cubic-bezier(0.25, 0.1, 0.25, 1)')",
            id="a bezier",
        ),
    ],
)
def test_a_keyframe_shows_itself_as_the_call_that_makes_it(frame, shown):
    assert repr(frame) == shown


def test_a_keyframe_cannot_be_changed():
    frame = Keyframe(t=1, value=2)
    with pytest.raises(AttributeError):
        frame.t = 3.0  # type: ignore[misc]  # the assignment this test makes on purpose


def test_a_keyframe_survives_a_deep_copy():
    frame = Keyframe(t=1.25, value=-3.0, ease="cubic-bezier(0.1, -2, 0.9, 3)")
    assert copy.deepcopy(frame) == frame


# ---- check_input: good and bad data of each kind ----

KICK, SNARE, HAT = Event(t=0.5, strength=0.3), Event(t=1.0, strength=0.8), Event(t=2.0, strength=0.45)
FIRST_OF_TWO, SECOND_OF_TWO = Event(t=1.0, strength=0.2), Event(t=1.0, strength=0.9)
STEADY = {"beats": [0.0, 0.5, 1.0, 1.5], "meter": 4, "first_downbeat": 0}
RATIOS = {"rate": 4, "values": [0.0, 1.0], "unit": "ratio"}

GOOD = {
    "points: mappings": (
        "points",
        [{"t": 0.5, "value": 2.0, "ease": "in"}, {"t": 1.5, "value": -1.0}, {"t": 4, "value": 0.25, "ease": "out"}],
        (Keyframe(t=0.5, value=2.0, ease="in"), Keyframe(t=1.5, value=-1.0), Keyframe(t=4.0, value=0.25, ease="out")),
    ),
    "points: one point at 0 in a tuple": ("points", ({"t": 0, "value": 9},), (Keyframe(t=0.0, value=9.0),)),
    "points: close together": (
        "points",
        [{"t": 1.0, "value": 0}, {"t": 1.0 + 1e-9, "value": 0}],
        (Keyframe(t=1.0, value=0.0), Keyframe(t=1.0 + 1e-9, value=0.0)),
    ),
    "points: a null ease is linear": ("points", [{"t": 0, "value": 1, "ease": None}], (Keyframe(t=0, value=1),)),
    "events: in time order": ("events", [KICK, SNARE, HAT], (KICK, SNARE, HAT)),
    "events: none": ("events", [], ()),
    "events: none in a tuple": ("events", (), ()),
    "events: unsorted are sorted": ("events", [HAT, KICK, SNARE], (KICK, SNARE, HAT)),
    "events: mappings, defaults filled in": (
        "events",
        [{"t": 3}, {"t": 1.25, "strength": 0.7, "duration": 0.5, "label": "snare", "pitch": 38}],
        (Event(t=1.25, strength=0.7, duration=0.5, label="snare", pitch=38), Event(t=3.0)),
    ),
    "events: ties in order, weak first": ("events", [FIRST_OF_TWO, SECOND_OF_TWO], (FIRST_OF_TWO, SECOND_OF_TWO)),
    "events: ties in order, strong first": ("events", [SECOND_OF_TWO, FIRST_OF_TWO], (SECOND_OF_TWO, FIRST_OF_TWO)),
    "events: ties beside an earlier one": (
        "events",
        [FIRST_OF_TWO, KICK, SECOND_OF_TWO],
        (KICK, FIRST_OF_TWO, SECOND_OF_TWO),
    ),
    "grid: a mapping": (
        "grid",
        {"beats": [0.0, 0.5, 1.1], "meter": 3, "first_downbeat": 1},
        Grid(beats=(0.0, 0.5, 1.1), meter=3, first_downbeat=1),
    ),
    "values: floats in a list": ("values", [0.25, -1.5, 3.0], (0.25, -1.5, 3.0)),
    "values: whole numbers are decimals": ("values", (1, 2, -3), (1.0, 2.0, -3.0)),
    "values: large and small": ("values", [1e300, -1e-300], (1e300, -1e-300)),
    "series: a mapping": ("series", RATIOS, Series(start=0.0, rate=4.0, values=(0.0, 1.0), unit=Unit.RATIO)),
}  # fmt: skip


@pytest.mark.parametrize(("kind", "given", "expected"), GOOD.values(), ids=GOOD.keys())
def test_good_data_comes_back_in_its_checked_form(kind, given, expected):
    checked = _core.check_input(kind, given)
    assert type(checked) is type(expected)
    assert checked == expected


def test_values_come_back_as_floats():
    assert [type(value) for value in _core.check_input("values", [1, 2.5])] == [float, float]


BAD = {
    "points: an empty list": ("points", [], [("points", "at least one keyframe")]),
    "points: a number": ("points", 5, [("points", "sequence")]),
    "points: text": ("points", "0:1", [("points", "sequence")]),
    "points: None": ("points", None, [("points", "sequence")]),
    "points: a mapping": ("points", {"t": 0, "value": 1}, [("points", "sequence")]),
    "points: a set has no order": ("points", {1.0}, [("points", "set")]),
    "points: an item that is no mapping": ("points", [{"t": 0, "value": 1}, 3], [("points[1]", "integer")]),
    "points: unsorted": (
        "points",
        [{"t": 0, "value": 1}, {"t": 5, "value": 1}, {"t": 2, "value": 1}],
        [("points[2].t", "not after the previous t = 5")],
    ),
    "points: a repeated time": (
        "points",
        [{"t": 3, "value": 1}, {"t": 3, "value": 1}],
        [("points[1].t", "not after the previous t = 3")],
    ),
    "points: a negative time, then before it": (
        "points",
        [{"t": 1, "value": 1}, {"t": -1, "value": 1}],
        [("points[1].t", ">= 0"), ("points[1].t", "not after the previous t = 1")],
    ),
    "points: every bad item is listed": (
        "points",
        [{"t": 0, "value": "a"}, {"t": 1, "value": 1, "ease": "wobble"}, {"t": 2, "value": 1, "x": 1}],
        [("points[0].value", "string"), ("points[1].ease", "wobble"), ("points[2].x", "unknown field")],
    ),
    "events: text": ("events", "0.5, 1.0", [("events", "sequence")]),
    "events: a mapping": ("events", {"t": 0.5}, [("events", "sequence")]),
    "events: None": ("events", None, [("events", "sequence")]),
    "events: an item that is no mapping": ("events", [{"t": 0}, 3], [("events[1]", "integer")]),
    "events: every broken item is listed": (
        "events",
        [{"t": -1, "strength": 0.5}, {"t": 0}, {"t": 1, "strength": 2}, {"t": 2, "duration": 0}, {"t": 1, "x": 0}],
        [
            ("events[0].t", ">= 0"),
            ("events[2].strength", "[0, 1]"),
            ("events[3].duration", "> 0"),
            ("events[4].x", "unknown field"),
        ],
    ),
    "events: a bool time": ("events", [{"t": True}], [("events[0].t", "boolean")]),
    "events: a pitch above 127": ("events", [{"t": 1, "pitch": 128}], [("events[0].pitch", "[0, 127]")]),
    "events: an empty label": ("events", [{"t": 1, "label": ""}], [("events[0].label", "empty")]),
    "grid: a number": ("grid", 5, [("grid", "an object")]),
    "grid: a list of beats": ("grid", [0.0, 0.5], [("grid", "an object")]),
    "grid: meter missing": ("grid", {"beats": [0.0, 0.5], "first_downbeat": 0}, [("grid.meter", "required")]),
    "grid: one beat": ("grid", {**STEADY, "beats": [0.0]}, [("grid.beats", "at least")]),
    "grid: beats out of order": ("grid", {**STEADY, "beats": [0.0, 1.0, 0.5]}, [("grid.beats[2]", "")]),
    "grid: a downbeat outside the bar": ("grid", {**STEADY, "first_downbeat": 4}, [("grid.first_downbeat", "")]),
    "grid: an unknown field": ("grid", {**STEADY, "tempo": 120}, [("grid.tempo", "unknown field")]),
    "values: an empty list": ("values", [], [("values", "at least one value")]),
    "values: a number": ("values", 5, [("values", "sequence")]),
    "values: a mapping": ("values", {"a": 1.0}, [("values", "sequence")]),
    "values: a set has no order": ("values", {1.0, 2.0}, [("values", "set")]),
    "values: every bad item is listed": (
        "values",
        [True, 1.0, "x", math.nan, [1.0], -math.inf, None],
        [
            ("values[0]", "boolean"),
            ("values[2]", "string"),
            ("values[3]", "finite"),
            ("values[4]", "sequence"),
            ("values[5]", "finite"),
            ("values[6]", "number"),
        ],
    ),
    "values: a whole number beyond 2^53": ("values", [2**53 + 1], [("values[0]", "2^53")]),
    "values: an integer beyond 64 bits": ("values", [2**64], [("values[0]", "64 bits")]),
    "series: a list of values": ("series", [0.1, 0.2], [("series", "an object")]),
    "series: rate zero": ("series", {**RATIOS, "rate": 0}, [("series.rate", "> 0")]),
    "series: a bool value": ("series", {**RATIOS, "values": [0.1, True]}, [("series.values[1]", "boolean")]),
    "series: an unknown unit": ("series", {**RATIOS, "unit": "pixels"}, [("series.unit", "pixels")]),
    "series: a negative start": ("series", {**RATIOS, "start": -1}, [("series.start", ">= 0")]),
    "series: an unknown field": ("series", {**RATIOS, "gain": 2}, [("series.gain", "unknown field")]),
}  # fmt: skip


@pytest.mark.parametrize(("kind", "given", "expected"), BAD.values(), ids=BAD.keys())
def test_bad_data_is_refused_as_a_parameter_error_naming_each_path(kind, given, expected):
    with pytest.raises(ParameterError) as refused:
        _core.check_input(kind, given)
    assert_lists(refused, expected)
    assert str(refused.value).startswith(f"{expected[0][0]}: ")  # the message names no private model


def test_an_unknown_kind_is_a_bug_of_the_caller_naming_the_kinds():
    with pytest.raises(ValueError, match="unknown input 'keyframes': use points, events, grid, values or series"):
        _core.check_input("keyframes", [])


# ---- Python's own values ----

NUMPY = {
    "values: numpy scalars of every width": (
        "values",
        [np.float64(0.5), np.float32(0.25), np.float16(-2.0), np.int64(3), np.int8(-4), np.uint16(7)],
        (0.5, 0.25, -2.0, 3.0, -4.0, 7.0),
    ),
    "values: a numpy array": ("values", np.array([[1.5], [2.5]]).ravel(), (1.5, 2.5)),
    "points: numpy numbers in a keyframe": (
        "points",
        [{"t": np.int32(0), "value": np.float32(1.5)}, {"t": np.float64(2), "value": np.int64(-1)}],
        (Keyframe(t=0.0, value=1.5), Keyframe(t=2.0, value=-1.0)),
    ),
    "events: a numpy time and strength": (
        "events",
        [{"t": np.float32(0.5), "strength": np.float64(0.3)}],
        (Event(t=0.5, strength=0.3),),
    ),
}  # fmt: skip


@pytest.mark.parametrize(("kind", "given", "expected"), NUMPY.values(), ids=NUMPY.keys())
def test_numpy_numbers_are_numbers(kind, given, expected):
    assert _core.check_input(kind, given) == expected


@pytest.mark.parametrize(
    ("kind", "given", "expected"),
    [
        pytest.param("values", [np.bool_(True), 1.0], [("values[0]", "boolean")], id="a numpy bool is no number"),
        pytest.param("values", [np.float64("nan")], [("values[0]", "finite")], id="a numpy NaN"),
        pytest.param("values", [np.longdouble(1)], [("values[0]", "longdouble")], id="a long double: no twin"),
        pytest.param("points", [{"t": np.bool_(False), "value": 0}], [("points[0].t", "boolean")],
                     id="a numpy bool time"),
    ],
)  # fmt: skip
def test_numpy_values_that_are_no_finite_numbers_are_refused(kind, given, expected):
    with pytest.raises(ParameterError) as refused:
        _core.check_input(kind, given)
    assert_lists(refused, expected)


class Text(str):
    """A subclass of str: still text."""

    __slots__ = ()


class Items(list[object]):
    """A subclass of list: still a list."""


class Fields(dict[str, object]):
    """A subclass of dict: still a mapping."""


class Frozen(Mapping[str, object]):
    """A mapping that is no dict."""

    def __init__(self, **fields: object) -> None:
        self._fields = fields

    def __getitem__(self, key: str) -> object:
        return self._fields[key]

    def __iter__(self) -> Iterator[str]:
        return iter(self._fields)

    def __len__(self) -> int:
        return len(self._fields)


SUBCLASSES = {
    "points: a list subclass of dict subclasses": (
        "points",
        Items([Fields(t=0, value=1), Fields(t=1, value=2, ease=Text("hold"))]),
        (Keyframe(t=0.0, value=1.0), Keyframe(t=1.0, value=2.0, ease="hold")),
    ),
    "points: an OrderedDict and a read-only view": (
        "points",
        [OrderedDict(t=0, value=1), MappingProxyType({"t": 2, "value": 3})],
        (Keyframe(t=0.0, value=1.0), Keyframe(t=2.0, value=3.0)),
    ),
    "events: a Mapping that is no dict, a UserDict": (
        "events",
        (Frozen(t=1.0, label=Text("kick")), UserDict(t=0.5)),
        (Event(t=0.5), Event(t=1.0, label="kick")),
    ),
    "grid: a Mapping that is no dict": (
        "grid",
        Frozen(beats=Items([0.0, 0.5]), meter=2, first_downbeat=0),
        Grid(beats=(0.0, 0.5), meter=2, first_downbeat=0),
    ),
    "values: a tuple subclass": ("values", type("Pair", (tuple,), {})((1, 2)), (1.0, 2.0)),
}  # fmt: skip


@pytest.mark.parametrize(("kind", "given", "expected"), SUBCLASSES.values(), ids=SUBCLASSES.keys())
def test_subclasses_of_list_dict_and_str_and_any_mapping_are_read_as_what_they_are(kind, given, expected):
    assert _core.check_input(kind, given) == expected


NONE_KEYED: dict[object, object] = {None: 1, "beats": [0.0, 0.5], "meter": 2, "first_downbeat": 0}


@pytest.mark.parametrize(
    ("kind", "given", "expected"),
    [
        pytest.param("points", [{"t": 0, "value": 1, 2: "x"}], [("points[0]", "must be text (got 2)")],
                     id="an int key in a keyframe"),
        pytest.param("events", [{"t": 0}, {(1, 2): 0}], [("events[1]", "must be text (got (1, 2))")],
                     id="a tuple key in an event"),
        pytest.param("grid", NONE_KEYED, [("grid", "must be text (got None)")], id="a None key in a grid"),
        pytest.param("points", [{"t": 0, "value": 1, LONE: 1}], [("points[0]", "UTF-8")],
                     id="a key with a lone surrogate"),
        pytest.param("events", [{"t": 0, "label": Text(LONE)}], [("events[0].label", "UTF-8")],
                     id="a str subclass with a lone surrogate"),
    ],
)  # fmt: skip
def test_a_mapping_with_a_key_that_is_no_text_is_refused_at_its_place(kind, given, expected):
    with pytest.raises(ParameterError) as refused:
        _core.check_input(kind, given)
    assert_lists(refused, expected)


# ---- the package's own objects ----

OBJECTS = {
    "points: keyframes and mappings mixed": (
        "points",
        (Keyframe(t=1.0, value=3.0, ease="hold"), {"t": 2.0, "value": 5.0}),
        (Keyframe(t=1.0, value=3.0, ease="hold"), Keyframe(t=2.0, value=5.0)),
    ),
    "events: events and mappings mixed": ("events", [SNARE, {"t": 0.5, "strength": 0.3}], (KICK, SNARE)),
    "events: an event with every field": (
        "events",
        [Event(t=1.0, strength=0.5, duration=0.25, label="note", pitch=60.5)],
        (Event(t=1.0, strength=0.5, duration=0.25, label="note", pitch=60.5),),
    ),
    "grid: a grid": ("grid", Grid(beats=(0.25, 0.75), meter=2, first_downbeat=1), Grid(beats=(0.25, 0.75), meter=2,
                                                                                         first_downbeat=1)),
    "series: a series": (
        "series",
        Series(rate=2.0, values=(0.25, 0.75), unit=Unit.FRACTION, scaling="by hand", start=1.5),
        Series(rate=2.0, values=(0.25, 0.75), unit=Unit.FRACTION, scaling="by hand", start=1.5),
    ),
}  # fmt: skip


@pytest.mark.parametrize(("kind", "given", "expected"), OBJECTS.values(), ids=OBJECTS.keys())
def test_the_packages_own_objects_are_read_as_the_data_they_hold(kind, given, expected):
    assert _core.check_input(kind, given) == expected


@pytest.mark.parametrize(
    ("kind", "given", "expected"),
    [
        pytest.param("points", [Keyframe(t=2, value=0), Keyframe(t=1, value=0)],
                     [("points[1].t", "not after the previous t = 2")], id="keyframes out of order"),
        pytest.param("points", Keyframe(t=0, value=1), [("points", "sequence")], id="a keyframe, not a list of them"),
        pytest.param("events", KICK, [("events", "sequence")], id="an event, not a list of them"),
        pytest.param("points", [KICK], [("points[0].strength", "unknown field")], id="an event is no keyframe"),
        pytest.param("values", [Keyframe(t=0, value=1)], [("values[0]", "map")], id="a keyframe is no number"),
    ],
)  # fmt: skip
def test_an_object_of_the_wrong_kind_is_refused_as_its_data_would_be(kind, given, expected):
    with pytest.raises(ParameterError) as refused:
        _core.check_input(kind, given)
    assert_lists(refused, expected)


@pytest.mark.parametrize(
    ("kind", "given", "unknown"),
    [
        pytest.param("events", [Keyframe(t=0, value=1)], {"events[0].value", "events[0].ease"},
                     id="a keyframe is no event"),
        pytest.param("grid", Series(rate=1.0, values=(0.5,), unit=Unit.RATIO),
                     {"grid.start", "grid.rate", "grid.values", "grid.unit", "grid.scaling"}, id="a series is no grid"),
    ],
)  # fmt: skip
def test_an_object_with_fields_its_kind_lacks_is_refused_at_one_of_them(kind, given, unknown):
    with pytest.raises(ParameterError) as refused:
        _core.check_input(kind, given)
    [(path, rule)] = refused.value.problems  # an unknown field stops the reading of that object
    assert path in unknown
    assert "unknown field" in rule


# ---- hostile values ----


class Boom(Exception):
    """What a user's object raises."""


class Raising(Mapping[str, object]):
    """A mapping whose keys are listed but whose values cannot be read."""

    def __getitem__(self, key: str) -> object:
        raise Boom(f"no {key}")

    def __iter__(self) -> Iterator[str]:
        return iter(["t", "value"])

    def __len__(self) -> int:
        return 2


class Unlisted(Mapping[str, object]):
    """A mapping whose keys cannot be listed."""

    def __getitem__(self, key: str) -> object:
        return 0

    def __iter__(self) -> Iterator[str]:
        raise Boom("no keys")

    def __len__(self) -> int:
        return 2


class Unlistable(np.ndarray):
    """A numpy array whose items cannot be listed."""

    def tolist(self) -> object:  # type: ignore[override]  # raising is the point
        raise Boom("no list")


@pytest.mark.parametrize(
    ("kind", "given", "said"),
    [
        pytest.param("points", [{"t": 0, "value": 1}, Raising()], "no t", id="a keyframe's raising __getitem__"),
        pytest.param("events", [Raising()], "no t", id="an event's raising __getitem__"),
        pytest.param("grid", Raising(), "no t", id="a grid's raising __getitem__"),
        pytest.param("series", Unlisted(), "no keys", id="a series' raising __iter__"),
        pytest.param("events", [{"t": 0}, Unlisted(), {"t": -1}], "no keys", id="beside broken items, the raise wins"),
        pytest.param("values", np.zeros(2).view(Unlistable), "no list", id="an array's raising tolist"),
        pytest.param("grid", {**STEADY, "beats": np.zeros(2).view(Unlistable)}, "no list", id="inside a field"),
    ],
)  # fmt: skip
def test_an_exception_a_users_object_raises_while_read_reaches_the_caller_unchanged(kind, given, said):
    with pytest.raises(Boom, match=said):
        _core.check_input(kind, given)


class Unloopable(list[object]):
    """A list that raises when looped over: its items are read as a list's, not through `__iter__`."""

    def __iter__(self) -> Iterator[object]:
        raise Boom("no loop")


def test_a_list_subclass_is_read_by_its_items_not_its_iter():
    assert _core.check_input("values", Unloopable([1.0, 2.0])) == (1.0, 2.0)


DEEPEST = 128  # the most lists or mappings a value read from Python may be nested in (one more is refused)


def nested(depth):
    """`depth` lists, each inside the next: `nested(2)` is `[[]]`."""
    value: list[object] = []
    for _ in range(depth - 1):
        value = [value]
    return value


@pytest.mark.parametrize(
    ("kind", "given", "expected"),
    [
        pytest.param("values", nested(DEEPEST), [("values[0]", "sequence")], id="values: a list of lists"),
        pytest.param("events", nested(DEEPEST), [("events[0]", "sequence")], id="events: a list of lists"),
        pytest.param("grid", {**STEADY, "beats": nested(DEEPEST)}, [("grid.beats[0]", "sequence")],
                     id="grid: beats nested"),
    ],
)  # fmt: skip
def test_a_value_nested_deeply_is_refused_where_its_kind_needs_no_list(kind, given, expected):
    with pytest.raises(ParameterError) as refused:
        _core.check_input(kind, given)
    assert_lists(refused, expected)


# each in a fresh interpreter: a reader that recursed without a limit would crash it, not fail a test
NESTING = """
from bullterrier_control._native import _core
from bullterrier_control.errors import ParameterError

cyclic = []
cyclic.append(cyclic)
deep = []
for _ in range(1_000_000):
    deep = [deep]
frame = {"t": 0, "value": 1}
frame["ease"] = frame
try:
    READ
except ParameterError as error:
    print(type(error).__name__, error)
"""


@pytest.mark.parametrize(
    ("read", "said"),
    [
        pytest.param("_core.check_input('values', deep)", "ParameterError values[0]: ", id="values, 1M deep"),
        pytest.param("_core.check_input('values', cyclic)", "ParameterError values[0]: ", id="values, a cycle"),
        pytest.param("_core.check_input('points', [frame])", "ParameterError points[0].ease: ",
                     id="a keyframe that holds itself"),
        pytest.param("_core.check_input('events', deep)", "ParameterError events[0]: ", id="events, 1M deep"),
        pytest.param("_core.check_input('grid', {'beats': deep, 'meter': 4, 'first_downbeat': 0})",
                     "ParameterError grid.beats[0]: ", id="grid, 1M deep"),
        pytest.param("_core.Keyframe.parse({'t': deep, 'value': 0})", "ParameterError Keyframe: t: ",
                     id="a keyframe, 1M deep"),
    ],
)  # fmt: skip
def test_a_cyclic_or_a_million_deep_value_is_refused_without_crashing(read, said):
    code = NESTING.replace("READ", read)
    done = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True, encoding="utf-8", check=False)
    assert done.returncode == 0, done.stderr
    assert done.stdout.startswith(said), done.stdout


MILLION = 1_000_000


def test_a_million_good_values_come_back_whole():
    given = [float(index) for index in range(MILLION)]
    checked = _core.check_input("values", given)
    assert len(checked) == MILLION
    assert (checked[0], checked[-1]) == (0.0, 999_999.0)


@pytest.mark.parametrize(
    ("kind", "given", "first"),
    [
        pytest.param("values", [True] * MILLION, "values[0]", id="a million bools"),
        pytest.param("events", [{"t": -1.0}] * MILLION, "events[0].t", id="a million events at -1 s"),
        pytest.param("points", [{"t": 0, "value": "x"}] * MILLION, "points[0].value", id="a million bad keyframes"),
    ],
)
def test_a_million_broken_items_keep_100_problems_and_count_the_rest(kind, given, first):
    with pytest.raises(ParameterError) as refused:
        _core.check_input(kind, given)
    problems = refused.value.problems
    assert len(problems) == 100
    assert (problems[0][0], problems[99][0]) == (first, first.replace("[0]", "[99]"))
    assert str(refused.value).endswith(f"; ... and {MILLION - 20} more")  # 20 shown, every other one counted


# ---- names and docs ----


def test_the_inputs_are_named_and_documented_as_help_shows_them():
    assert _core.input_docs() == {
        "points": (
            "Keyframes: a list of {t, value, ease}, t in seconds from 0 and strictly increasing, ease the curve of the"
            " segment that starts there (ignored on the last point; default linear): one of linear, hold, in, out,"
            " in-out, cubic-in, cubic-out, cubic-in-out, overshoot, or cubic-bezier(x1, y1, x2, y2)."
        ),
        "events": (
            "Events: a list of {t, strength, duration, label, pitch}, as in a bundle, in any order (checked into time"
            " order)."
        ),
        "grid": (
            "A beat grid: {beats, meter, first_downbeat} as in a bundle (beats in seconds, strictly increasing, at"
            " least two). Converts seconds to beats, so a source can follow tempo changes."
        ),
        "values": "Values: a non-empty list of numbers, in the order a source cycles through them.",
        "series": (
            "A series: {start, rate, values, unit, scaling} as in a bundle (values at a fixed rate in samples per"
            " second from a start time in seconds)."
        ),
    }
    assert list(_core.input_docs()) == ["points", "events", "grid", "values", "series"]
