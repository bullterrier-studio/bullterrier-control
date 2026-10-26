from typing import Literal

import pytest

from bullterrier_control import FormatError
from bullterrier_control.formats.model import Finite, Switch, Whole
from bullterrier_control.params import UNITS, Params, param


class Wave(Params):
    """A model with every kind of field."""

    depth: Finite = param(
        0.5, doc="How far the wave moves the value, as a share of the range; 0 is flat.", unit="fraction", min=0, max=1
    )
    steps: Whole = param(4, doc="How many steps one cycle has; more is smoother.", unit="count", min=1, max=64)
    shape: Literal["sine", "saw"] = param("sine", doc="The wave: 'sine' or 'saw'.")
    smooth: Switch = param(True, doc="Whether the corners are rounded.")
    rate: Finite = param(
        1.0, doc="Cycles per second; raising it speeds the wave up.", unit="hz", min=0.001, max=100, advanced=True
    )


def test_a_model_has_the_defaults_it_declared():
    wave = Wave.parse({})
    assert (wave.depth, wave.steps, wave.shape, wave.smooth, wave.rate) == (0.5, 4, "sine", True, 1.0)


def test_values_given_replace_the_defaults_and_a_whole_number_is_fine_for_a_decimal():
    wave = Wave.parse({"depth": 1, "steps": 8, "shape": "saw", "smooth": False})
    assert (wave.depth, wave.steps, wave.shape, wave.smooth) == (1.0, 8, "saw", False)


def test_the_fields_are_described_basic_ones_first_then_advanced():
    fields = {field.name: field for field in Wave.fields_doc()}
    assert list(fields) == ["depth", "steps", "shape", "smooth", "rate"]
    assert (fields["depth"].unit, fields["depth"].minimum, fields["depth"].maximum, fields["depth"].default) == (
        "fraction",
        0,
        1,
        0.5,
    )
    assert fields["shape"].unit is None
    assert fields["shape"].minimum is None
    assert [field.advanced for field in fields.values()] == [False, False, False, False, True]
    assert "speeds the wave up" in fields["rate"].doc


class Reordered(Params):
    """An advanced field declared before a basic one."""

    fine: Finite = param(1.0, doc="A tuning value.", unit="ratio", min=0, max=2, advanced=True)
    main: Finite = param(1.0, doc="The main value.", unit="ratio", min=0, max=2)


def test_advanced_fields_are_listed_after_the_basic_ones_whatever_the_declaration_order():
    assert [field.name for field in Reordered.fields_doc()] == ["main", "fine"]


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
    with pytest.raises(FormatError) as refused:
        Wave.parse(values)
    reports(refused.value, named)


def declared(annotation, spec, name="amount"):
    """Make a model with one field, so that a broken declaration shows when the class is made."""
    return type("Broken", (Params,), {"__annotations__": {name: annotation}, name: spec, "__doc__": "A broken model."})


DECLARATIONS = {
    "a field without a description": (Finite, param(1.0, doc="", unit="ratio", min=0, max=2), "amount"),
    "a number without a unit": (Finite, param(1.0, doc="Some number.", min=0, max=2), "unit"),
    "a unit that is not one of the units": (
        Finite,
        param(1.0, doc="Some number.", unit="percent", min=0, max=2),
        "percent",
    ),
    "a number without a range": (Finite, param(1.0, doc="Some number.", unit="ratio"), "min"),
    "a number with half a range": (Finite, param(1.0, doc="Some number.", unit="ratio", min=0), "max"),
    "a default below its range": (Finite, param(-1.0, doc="Some number.", unit="ratio", min=0, max=2), "default"),
    "a default above its range": (Finite, param(3.0, doc="Some number.", unit="ratio", min=0, max=2), "default"),
    "a switch with a unit": (Switch, param(True, doc="A switch.", unit="count"), "unit"),
    "a choice with a range": (Literal["a", "b"], param("a", doc="A choice.", min=0, max=1), "min"),
    "a number not declared with param": (Finite, 1.0, "unit"),
    "a discouraged name": (Finite, param(1.0, doc="Some number.", unit="ratio", min=0, max=2), "strength"),
}


@pytest.mark.parametrize(("annotation", "spec", "named"), DECLARATIONS.values(), ids=DECLARATIONS.keys())
def test_a_broken_declaration_fails_when_the_class_is_made_naming_the_field_or_rule(annotation, spec, named):
    field = "strength" if named == "strength" else "amount"
    with pytest.raises(ValueError, match=named):
        declared(annotation, spec, field)


@pytest.mark.parametrize("unit", UNITS)
def test_every_declared_unit_can_be_used(unit):
    declared(Finite, param(1.0, doc="Some number.", unit=unit, min=0, max=2))


@pytest.mark.parametrize("default", [0.0, 2.0], ids=["the lowest value", "the highest value"])
def test_a_default_may_sit_at_either_end_of_its_range(default):
    declared(Finite, param(default, doc="Some number.", unit="ratio", min=0, max=2))
