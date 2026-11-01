from typing import Literal

import pytest

from bullterrier_control import ParameterError
from bullterrier_control.plugin import Finite, Params, Switch, Whole, param


class Wave(Params):
    """A model with every kind of field."""

    depth: Finite = param(
        0.5, doc="How far the wave moves the value, as a share of the range; 0 is flat.", unit="fraction", min=0, max=1
    )
    steps: Whole = param(4, doc="How many steps one cycle has; more is smoother.", unit="count", min=1, max=64)
    shape: Literal["sine", "saw"] = param("sine", doc="The wave: 'sine' or 'saw'.")
    smooth: Switch = param(True, doc="Whether the corners are rounded.")
    rate: Finite = param(
        1.0, doc="Cycles per second; raising it speeds the wave up.", unit="hertz", min=0.001, max=100, advanced=True
    )


def test_a_model_has_the_defaults_it_declared():
    wave = Wave.parse({})
    assert (wave.depth, wave.steps, wave.shape, wave.smooth, wave.rate) == (0.5, 4, "sine", True, 1.0)


def test_values_given_replace_the_defaults_and_a_whole_number_is_fine_for_a_decimal():
    wave = Wave.parse({"depth": 1, "steps": 8, "shape": "saw", "smooth": False})
    assert (wave.depth, wave.steps, wave.shape, wave.smooth) == (1.0, 8, "saw", False)


def test_the_constructor_reads_keywords_as_parse_reads_a_mapping():
    assert Wave(depth=0.25, steps=2) == Wave.parse({"depth": 0.25, "steps": 2})
    with pytest.raises(ParameterError, match="depth"):
        Wave(depth=2)


def test_a_model_is_a_frozen_value():
    wave = Wave.parse({"depth": 0.25})
    with pytest.raises(AttributeError):
        wave.depth = 0.75  # the point: a field cannot be rebound
    assert wave.depth == 0.25
    assert wave == Wave.parse({"depth": 0.25})
    assert wave != Wave.parse({"depth": 0.75})
    assert hash(wave) == hash(Wave.parse({"depth": 0.25}))
    assert "depth=0.25" in repr(wave)


def test_to_dict_holds_every_field_with_defaults_filled_in():
    assert Wave.parse({"steps": 8}).to_dict() == {
        "depth": 0.5,
        "steps": 8,
        "shape": "sine",
        "smooth": True,
        "rate": 1.0,
    }


def test_the_fields_are_described_basic_ones_first_then_advanced():
    fields = Wave.describe()
    assert list(fields) == ["depth", "steps", "shape", "smooth", "rate"]
    depth = fields["depth"]
    assert (depth["unit"], depth["minimum"], depth["maximum"], depth["default"]) == ("fraction", 0, 1, 0.5)
    assert depth["range"] == "0 to 1"
    assert fields["shape"]["unit"] is None
    assert "minimum" not in fields["shape"]
    assert fields["shape"]["enum"] == ["sine", "saw"]
    assert [field["advanced"] for field in fields.values()] == [False, False, False, False, True]
    assert "speeds the wave up" in fields["rate"]["description"]


class Reordered(Params):
    """An advanced field declared before a basic one."""

    fine: Finite = param(1.0, doc="A tuning value.", unit="ratio", min=0, max=2, advanced=True)
    main: Finite = param(1.0, doc="The main value.", unit="ratio", min=0, max=2)


def test_advanced_fields_are_listed_after_the_basic_ones_whatever_the_declaration_order():
    assert list(Reordered.describe()) == ["main", "fine"]


EDGES = {
    "lowest depth": {"depth": 0},
    "highest depth": {"depth": 1},
    "fewest steps": {"steps": 1},
    "most steps": {"steps": 64},
    "lowest rate": {"rate": 0.001},
    "highest rate": {"rate": 100},
}


@pytest.mark.parametrize("values", EDGES.values(), ids=EDGES.keys())
def test_each_number_is_valid_at_both_ends_of_its_range(values):
    Wave.parse(values)


INVALID = {
    "a bool for a decimal": ({"depth": True}, "depth"),
    "a string for a decimal": ({"depth": "0.5"}, "depth"),
    "a decimal above its range": ({"depth": 1.5}, "depth"),
    "a decimal below its range": ({"depth": -0.01}, "depth"),
    "not a number": ({"depth": float("nan")}, "depth"),
    "a float for a whole number": ({"steps": 4.0}, "steps"),
    "a whole number above its range": ({"steps": 65}, "steps"),
    "a choice not offered": ({"shape": "square"}, "shape"),
    "a number for a switch": ({"smooth": 1}, "smooth"),
    "an unknown name": ({"fps": 30}, "fps"),
}


@pytest.mark.parametrize(("values", "named"), INVALID.values(), ids=INVALID.keys())
def test_an_invalid_value_is_refused_naming_the_field(values, named, reports):
    with pytest.raises(ParameterError) as refused:
        Wave.parse(values)
    reports(refused.value, named)
    assert [path for path, _ in refused.value.problems] == [named]


def test_every_broken_rule_is_listed():
    with pytest.raises(ParameterError) as refused:
        Wave.parse({"depth": 2, "steps": 1.0, "fps": 30})
    assert {path for path, _ in refused.value.problems} == {"depth", "steps", "fps"}


def declared(annotation, spec, name="amount"):
    """Make a model with one field, so that a broken declaration shows when the class is made."""
    return type("Broken", (Params,), {"__annotations__": {name: annotation}, name: spec, "__doc__": "A broken model."})


DECLARATIONS = {
    "a field without a description": (Finite, param(1.0, doc="", unit="ratio", min=0, max=2), "description"),
    "a number without a unit": (Finite, param(1.0, doc="Some number.", min=0, max=2), "unit"),
    "a unit that is not one of the units": (
        Finite,
        param(1.0, doc="Some number.", unit="percent", min=0, max=2),
        "percent",
    ),
    "a number without a range": (Finite, param(1.0, doc="Some number.", unit="ratio"), "min"),
    "a number with half a range": (Finite, param(1.0, doc="Some number.", unit="ratio", min=0), "max"),
    "a default below its range": (Finite, param(-1.0, doc="Some number.", unit="ratio", min=0, max=2), "range"),
    "a default above its range": (Finite, param(3.0, doc="Some number.", unit="ratio", min=0, max=2), "range"),
    "a switch with a unit": (Switch, param(True, doc="A switch.", unit="count"), "unit"),
    "a choice with a range": (Literal["a", "b"], param("a", doc="A choice.", min=0, max=1), "min"),
    "a number not declared with param": (Finite, 1.0, "unit"),
    "a discouraged name": (Finite, param(1.0, doc="Some number.", unit="ratio", min=0, max=2), "strength"),
    "a name that is a unit": (Finite, param(1.0, doc="Some number.", unit="ratio", min=0, max=2), "seconds"),
    "an optional number": (Finite | None, param(None, doc="Maybe a number."), "None"),
    "a list of numbers": (list[Finite], param([1.0], doc="Some numbers."), "list"),
    "a choice of numbers": (Literal[1, 2], param(1, doc="A choice."), "names"),
    "a range with no top": (Finite, param(1.0, doc="Some number.", unit="ratio", min=0, max=float("inf")), "finite"),
    "a range with a nan end": (Finite, param(1.0, doc="Some number.", unit="ratio", min=float("nan"), max=2), "finite"),
    "a choice whose default is not offered": (Literal["a", "b"], param("zzz", doc="A choice."), "default"),
    "a switch whose default is 1": (Switch, param(1, doc="A switch."), "default"),
    "a whole number whose default is 1.5": (Whole, param(1.5, doc="A count.", unit="count", min=0, max=9), "default"),
    "a cyclic number in seconds": (
        Finite,
        param(0.5, doc="Some number.", unit="seconds", min=0, max=1, cyclic=True),
        "cyclic",
    ),
    "a cyclic range of two cycles": (
        Finite,
        param(0.5, doc="Some number.", unit="cycles", min=0, max=2, cyclic=True),
        "cyclic",
    ),
    "a cyclic range of half a cycle": (
        Finite,
        param(0.25, doc="Some number.", unit="cycles", min=0, max=0.5, cyclic=True),
        "cyclic",
    ),
    "a cyclic switch": (Switch, param(True, doc="A switch.", cyclic=True), "cyclic"),
}


@pytest.mark.parametrize(("annotation", "spec", "named"), DECLARATIONS.values(), ids=DECLARATIONS.keys())
def test_a_broken_declaration_fails_when_the_class_is_made_naming_the_field_and_rule(annotation, spec, named):
    field = named if named in ("strength", "seconds") else "amount"
    with pytest.raises(ParameterError, match=named) as refused:
        declared(annotation, spec, field)
    assert {path for path, _ in refused.value.problems} == {field}


UNITS = ["seconds", "beats", "hertz", "cycles", "fraction", "decibels", "ratio", "semitones", "count"]


@pytest.mark.parametrize("unit", UNITS)
def test_every_documented_unit_can_be_used(unit):
    declared(Finite, param(1.0, doc="Some number.", unit=unit, min=0, max=2))


@pytest.mark.parametrize("default", [0.0, 2.0], ids=["the lowest value", "the highest value"])
def test_a_default_may_sit_at_either_end_of_its_range(default):
    declared(Finite, param(default, doc="Some number.", unit="ratio", min=0, max=2))


PLAIN = {
    "a plain float": (float, 1.0, True),
    "a plain float given text": (float, 1.0, "0.5"),
    "a plain int": (int, 3, True),
    "a plain int given a float": (int, 3, 3.0),
    "a plain int given text": (int, 3, "3"),
}


@pytest.mark.parametrize(("annotation", "default", "wrong"), PLAIN.values(), ids=PLAIN.keys())
def test_a_number_declared_with_a_plain_float_or_int_is_as_strict_as_the_shared_types(
    annotation, default, wrong, reports
):
    model = declared(annotation, param(default, doc="Some number.", unit="count", min=0, max=9))
    with pytest.raises(ParameterError) as refused:
        model.parse({"amount": wrong})
    reports(refused.value, "amount")


def test_a_field_without_a_default_is_refused_when_the_class_is_made():
    with pytest.raises(ParameterError, match="default"):
        type("NoDefault", (Params,), {"__annotations__": {"amount": Finite}, "__doc__": "No default."})


class Turn(Params):
    """A model with cyclic fields: one cycle wide, and not starting at 0 for the second."""

    phase: Finite = param(0.0, doc="Where in the cycle to start.", unit="cycles", min=0, max=1, cyclic=True)
    centred: Finite = param(0.0, doc="A position, centred on 0.", unit="cycles", min=-0.5, max=0.5, cyclic=True)


WRAPPED = {
    "inside the range: kept": ({"phase": 0.25}, {"phase": 0.25}),
    "the lowest end: kept": ({"phase": 0.0}, {"phase": 0.0}),
    "the highest end: kept, not wrapped to the lowest": ({"phase": 1.0}, {"phase": 1.0}),
    "a quarter past the end": ({"phase": 1.25}, {"phase": 0.25}),
    "a quarter before the start": ({"phase": -0.25}, {"phase": 0.75}),
    "several cycles past": ({"phase": 3.5}, {"phase": 0.5}),
    "a whole number of cycles below": ({"phase": -1}, {"phase": 0.0}),
    "a whole number given for a decimal": ({"phase": 2}, {"phase": 0.0}),
    "a range centred on 0, past its end": ({"centred": 0.75}, {"centred": -0.25}),
    "a range centred on 0, before its start": ({"centred": -0.75}, {"centred": 0.25}),
}


@pytest.mark.parametrize(("given", "expected"), WRAPPED.values(), ids=WRAPPED.keys())
def test_a_cyclic_number_outside_its_range_is_the_same_position_inside_it(given, expected):
    turn = Turn.parse(given)
    assert {name: getattr(turn, name) for name in expected} == expected


NOT_A_POSITION = {
    "a bool": True,
    "a string": "0.5",
    "nothing": None,
    "NaN": float("nan"),
    "an infinity": float("inf"),
    "a whole number beyond 64 bits": 10**400,
}


@pytest.mark.parametrize("field", ["phase", "centred"])  # `true` is 1: outside the second range, where it could wrap
@pytest.mark.parametrize("wrong", NOT_A_POSITION.values(), ids=NOT_A_POSITION.keys())
def test_a_cyclic_number_still_refuses_what_is_not_a_number(field, wrong, reports):
    with pytest.raises(ParameterError) as refused:
        Turn.parse({field: wrong})
    reports(refused.value, field)


def test_a_cyclic_range_may_start_anywhere_a_cycle_wide():
    declared(Finite, param(0.5, doc="Some number.", unit="cycles", min=-0.5, max=0.5, cyclic=True))


def test_the_description_keeps_the_turn_as_minimum_and_maximum_beside_cyclic():
    phase = Turn.describe()["phase"]
    assert (phase["minimum"], phase["maximum"], phase["unit"], phase["cyclic"]) == (0, 1, "cycles", True)
    assert not any(field["cyclic"] for field in Wave.describe().values())
