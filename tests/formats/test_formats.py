"""What every format class (`Event`, `Series`, `Grid`, `Bundle`) promises as a Python value, and how a Python value is
held to the rules a file is: equality and a hash that agrees with it, a repr, copies and pickles, no change once made,
the package as its home; strict numbers and text; a refusal that lists every problem."""

import copy
import pickle

import numpy as np
import pytest

import bullterrier_control
from bullterrier_control import Bundle, Event, FormatError, Grid, Series, Unit

FORMAT = "bullterrier-control/bundle"
GRID = Grid(beats=[0.5, 1.0], meter=2, first_downbeat=0)
BASS = Series(rate=2, values=[0.0, 0.5], unit=Unit.RATIO)


def a_bundle(**changes):
    parts = {
        "format": FORMAT,
        "version": 1,
        "source": "song.wav",
        "duration": 4.0,
        "grid": GRID,
        "series": {"bass": BASS},
        "events": {"kick": (Event(0.5),)},
    }
    return Bundle(**{**parts, **changes})


# each class: a value, an equal one made another way, and values that differ from it in one field each
VALUES = {
    "Event": (
        Event(0.5, label="kick"),
        Event.parse({"t": 0.5, "strength": 1, "label": "kick"}),
        [
            Event(0.25, label="kick"),
            Event(0.5, strength=0.5, label="kick"),
            Event(0.5, duration=1.0, label="kick"),
            Event(0.5),
            Event(0.5, label="snare"),
            Event(0.5, label="kick", pitch=60),
        ],
    ),
    "Series": (
        BASS,
        Series.parse({"start": 0, "rate": 2.0, "values": [0, 0.5], "unit": "ratio"}),
        [
            Series(rate=2, values=[0.0, 0.5], unit=Unit.RATIO, start=1),
            Series(rate=4, values=[0.0, 0.5], unit=Unit.RATIO),
            Series(rate=2, values=[0.0, 0.25], unit=Unit.RATIO),
            Series(rate=2, values=[0.0, 0.5, 0.5], unit=Unit.RATIO),
            Series(rate=2, values=[0.0, 0.5], unit=Unit.HERTZ),
            Series(rate=2, values=[0.0, 0.5], unit=Unit.FRACTION, scaling="peak"),
        ],
    ),
    "Grid": (
        GRID,
        Grid.parse({"beats": (0.5, 1), "meter": 2, "first_downbeat": 0}),
        [
            Grid(beats=[0.5, 1.5], meter=2, first_downbeat=0),
            Grid(beats=[0.5, 1.0, 1.5], meter=2, first_downbeat=0),
            Grid(beats=[0.5, 1.0], meter=3, first_downbeat=0),
            Grid(beats=[0.5, 1.0], meter=2, first_downbeat=1),
        ],
    ),
    "Bundle": (
        a_bundle(),
        Bundle.parse(a_bundle().to_dict()),
        [
            a_bundle(source="other.wav"),
            a_bundle(source=None),
            a_bundle(duration=5.0),
            a_bundle(grid=None),
            a_bundle(series={}),
            a_bundle(series={"bass": BASS, "drums": BASS}),
            a_bundle(events={"kick": (Event(0.75),)}),
            a_bundle(events={"snare": (Event(0.5),)}),
        ],
    ),
}


@pytest.mark.parametrize(("value", "same", "others"), VALUES.values(), ids=VALUES.keys())
def test_equal_values_are_equal_and_hash_alike(value, same, others):
    assert same is not value
    assert same == value
    assert hash(same) == hash(value)


@pytest.mark.parametrize(("value", "same", "others"), VALUES.values(), ids=VALUES.keys())
def test_a_value_that_differs_in_any_one_field_is_unequal_and_hashes_apart(value, same, others):
    assert all(other != value for other in others)
    assert len({hash(each) for each in [value, *others]}) == 1 + len(others)


@pytest.mark.parametrize(("value", "same", "others"), VALUES.values(), ids=VALUES.keys())
def test_a_value_is_not_equal_to_its_plain_data(value, same, others):
    assert value != value.to_dict()


MINUS_ZERO = {
    "an event at -0.0": (Event(-0.0), Event(0.0)),
    "a series starting at -0.0": (
        Series(rate=1, values=[-0.0], unit=Unit.RATIO, start=-0.0),
        Series(rate=1, values=[0.0], unit=Unit.RATIO),
    ),
    "a grid whose first beat is -0.0": (
        Grid(beats=[-0.0, 1.0], meter=1, first_downbeat=0),
        Grid(beats=[0.0, 1.0], meter=1, first_downbeat=0),
    ),
}


@pytest.mark.parametrize(("minus", "plus"), MINUS_ZERO.values(), ids=MINUS_ZERO.keys())
def test_minus_zero_is_zero_for_equality_and_hash(minus, plus):
    assert minus == plus
    assert hash(minus) == hash(plus)


COPIES = {
    "copy": copy.copy,
    "deepcopy": copy.deepcopy,
    "pickle": lambda value: pickle.loads(pickle.dumps(value)),  # noqa: S301  (our own bytes, just made)
}


@pytest.mark.parametrize("make", COPIES.values(), ids=COPIES.keys())
@pytest.mark.parametrize(("value", "same", "others"), VALUES.values(), ids=VALUES.keys())
def test_a_copy_or_an_unpickled_value_is_equal_and_of_the_same_class(value, same, others, make):
    made = make(value)
    assert type(made) is type(value)
    assert made == value
    assert hash(made) == hash(value)


def test_an_unpickled_bundle_keeps_its_parts_as_formats():
    made = pickle.loads(pickle.dumps(a_bundle()))  # noqa: S301  (our own bytes, just made)
    assert made.grid == GRID
    assert made.series["bass"] == BASS
    assert made.events["kick"] == (Event(0.5),)


FIELDS = {
    "Event": (Event(0.5), ["t", "strength", "duration", "label", "pitch", "colour"]),
    "Series": (BASS, ["start", "rate", "values", "unit", "scaling"]),
    "Grid": (GRID, ["beats", "meter", "first_downbeat", "downbeats"]),
    "Bundle": (a_bundle(), ["source", "duration", "grid", "series", "events"]),
    "Unit": (Unit.RATIO, ["value", "name"]),
}


@pytest.mark.parametrize(("value", "fields"), FIELDS.values(), ids=FIELDS.keys())
def test_a_value_cannot_be_changed(value, fields):
    for field in fields:
        with pytest.raises(AttributeError):
            setattr(value, field, 1)


@pytest.mark.parametrize("cls", [Event, Series, Grid, Bundle, Unit], ids=lambda cls: cls.__name__)
def test_each_class_is_the_packages_own(cls):
    assert cls.__module__ == "bullterrier_control"
    assert getattr(bullterrier_control, cls.__name__) is cls


REPRS = {
    "a moment": (Event(0.5), "Event(t=0.5, strength=1.0)"),
    "a note": (
        Event(1, duration=0.25, label="kick", pitch=60),
        "Event(t=1.0, strength=1.0, duration=0.25, label='kick', pitch=60.0)",
    ),
    "a short series": (BASS, "Series(start=0.0, rate=2.0, values=[0.0, 0.5], unit=Unit.RATIO)"),
    "a long series shows its ends": (
        Series(rate=1, values=range(8), unit="hertz"),
        "Series(start=0.0, rate=1.0, values=[0.0, 1.0, 2.0, ..., 6.0, 7.0], unit=Unit.HERTZ)",
    ),
    "a scaled series": (
        Series(rate=1, values=[1], unit="fraction", scaling="peak"),
        "Series(start=0.0, rate=1.0, values=[1.0], unit=Unit.FRACTION, scaling='peak')",
    ),
    "a grid": (GRID, "Grid(beats=[0.5, 1.0], meter=2, first_downbeat=0)"),
    "the smallest bundle": (Bundle(format=FORMAT, version=1), "Bundle(series=[], events={})"),
    "a full bundle": (
        a_bundle(),
        "Bundle(source='song.wav', duration=4.0, grid=Grid(beats=[0.5, 1.0], meter=2, first_downbeat=0),"
        " series=['bass'], events={'kick': 1})",
    ),
    "a unit": (Unit.RATIO, "Unit.RATIO"),
}


@pytest.mark.parametrize(("value", "shown"), REPRS.values(), ids=REPRS.keys())
def test_the_repr_shows_the_fields_that_say_something(value, shown):
    assert repr(value) == shown


# --- Python values held to the rules of a file --------------------------------------------------------------------

STRICT = {
    "a bool for a number": (lambda: Event(True), "t"),
    "a numpy bool for a number": (lambda: Event(0, strength=np.bool_(True)), "strength"),  # type: ignore[arg-type]  # wrong on purpose: refused at run time
    "text for a number": (lambda: Event("1"), "t"),  # type: ignore[arg-type]  # wrong on purpose: refused at run time
    "NaN": (lambda: Event(float("nan")), "t"),
    "an infinity": (lambda: Event(0, pitch=float("inf")), "pitch"),
    "a whole number beyond 2^53": (lambda: Event(2**53 + 1), "t"),
    "a negative whole number beyond 2^53": (lambda: Event(0, pitch=-(2**53) - 1), "pitch"),
    "a whole number beyond 64 bits": (lambda: Event(2**70), "t"),
    "a decimal for meter": (lambda: Grid(beats=[0, 1], meter=4.0, first_downbeat=0), "meter"),  # type: ignore[arg-type]  # wrong on purpose: refused at run time
    "a bool for first_downbeat": (lambda: Grid(beats=[0, 1], meter=4, first_downbeat=False), "first_downbeat"),
    "a decimal for version": (lambda: Bundle(format=FORMAT, version=1.0), "version"),  # type: ignore[arg-type]  # wrong on purpose: refused at run time
    "a lone surrogate in a label": (lambda: Event(0, label="\ud800"), "label"),
    "a lone surrogate in a scaling": (
        lambda: Series(rate=1, values=[0.5], unit="fraction", scaling="\udfff"),
        "scaling",
    ),
    "a lone surrogate in a source": (lambda: Bundle(format=FORMAT, version=1, source="a\ud800"), "source"),
    "-0.0 for a duration above 0": (lambda: Event(0, duration=-0.0), "duration"),
    "an unknown unit name": (lambda: Series(rate=1, values=[0.5], unit="Ratio"), "unit"),
    "an unknown field in parse": (lambda: Event.parse({"t": 1, "colour": "red"}), "colour"),
    "a whole number beyond 2^53 in parse": (
        lambda: Series.parse({"rate": 2**60, "values": [1], "unit": "ratio"}),
        "rate",
    ),
}


@pytest.mark.parametrize(("make", "named"), STRICT.values(), ids=STRICT.keys())
def test_a_python_value_a_file_could_not_hold_is_refused_naming_the_field(make, named, reports):
    with pytest.raises(FormatError) as refused:
        make()
    reports(refused.value, named)


def test_a_lone_surrogate_breaks_the_utf8_rule():
    with pytest.raises(FormatError) as refused:
        Event(0, label="\ud800")
    assert refused.value.problems == (("label", "text must be writable as UTF-8"),)


ACCEPTED = {
    "a whole number at 2^53": (lambda: Event(2**53), 2.0**53),
    "a whole number for a decimal": (lambda: Event(3), 3.0),
    "-0.0 for a time from 0": (lambda: Event(-0.0), 0.0),
    "a numpy float": (lambda: Event(np.float32(0.5)), 0.5),  # type: ignore[arg-type]  # numpy scalars are read too
    "a numpy integer": (lambda: Event(np.int64(7)), 7.0),  # type: ignore[arg-type]  # numpy scalars are read too
}


@pytest.mark.parametrize(("make", "t"), ACCEPTED.values(), ids=ACCEPTED.keys())
def test_a_number_a_file_can_hold_is_read_exactly(make, t):
    assert make().t == t


def test_a_key_that_is_not_text_is_refused():
    with pytest.raises(FormatError, match="key"):
        Event.parse({0: 1.0})  # type: ignore[dict-item]  # wrong on purpose: refused at run time


def test_a_refusal_is_the_packages_format_error():
    with pytest.raises(FormatError) as refused:
        Event(-1)
    assert isinstance(refused.value, bullterrier_control.InvalidError)
    assert isinstance(refused.value, ValueError)
    assert refused.value.problems == (("t", "must be >= 0 (got -1.0)"),)


def test_every_broken_rule_is_listed_in_the_order_of_the_fields():
    with pytest.raises(FormatError) as refused:
        Event(-1, strength=2, duration=0, label="", pitch=200)
    assert [path for path, _ in refused.value.problems] == ["t", "strength", "duration", "label", "pitch"]
    assert str(refused.value).startswith("Event: t: must be >= 0 (got -1.0); strength: ")


CAPPED = {
    "twenty problems are all shown": (20, None),
    "the twenty-first is counted": (21, "... and 1 more"),
    "twenty-five show twenty": (25, "... and 5 more"),
}


@pytest.mark.parametrize(("count", "more"), CAPPED.values(), ids=CAPPED.keys())
def test_the_message_shows_twenty_problems_and_the_error_holds_every_one(count, more):
    with pytest.raises(FormatError) as refused:
        Series(rate=1, values=[float("nan")] * count, unit=Unit.RATIO)
    message = str(refused.value)
    assert [path for path, _ in refused.value.problems] == [f"values[{index}]" for index in range(count)]
    assert message.count("must be finite") == min(count, 20)
    assert "values[19]" in message
    assert "values[20]" not in message
    if more is None:
        assert "more" not in message
    else:
        assert message.endswith(f"; {more}")
