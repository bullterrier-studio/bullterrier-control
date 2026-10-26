"""What every registered source, shaper and operation promises, checked over all of them, the installed ones included.

An architecture test of the extension point: its subject is every thing the registry lists, so a new one is covered the
moment it is registered and no test names it. The signal tests run over sources and shapers (an operation makes data,
not a signal); the parameter and input tests run over every kind.
"""

from typing import Any

import numpy as np
import pytest

from bullterrier_control import Event, Grid, Keyframe, ParameterError, Series, Signal, SignalError, Unit, registry
from bullterrier_control.signal import from_series

ALL = sorted(registry.all_registered())
SIGNALS = sorted([*registry.sources(), *registry.shapers()])
SHAPERS = sorted(registry.shapers())
OPERATIONS = sorted(registry.operations())
# a signal with every kind of movement: a rise, a plateau, a fall, a jump, a drop below zero
HISTORY = (0.0, 0.2, 1.0, 1.0, 0.4, 2.0, -1.0, 0.5, 0.5, 3.0)
CHANGE_AT = 3.0  # seconds: from here on the input differs between the two inputs the causality test compares


def input_signal(tail: float = 0.0) -> Signal:
    """A 10 s series at 10 Hz; `tail` is added to every sample from CHANGE_AT on, to tell what depends on the future."""
    values = tuple(HISTORY[index % len(HISTORY)] + (tail if index / 10.0 >= CHANGE_AT else 0.0) for index in range(100))
    return from_series(Series(rate=10.0, values=values, unit=Unit.RATIO), interpolation="hold")


# What every kind of input is built from. A new kind of input (events, a grid, ...) is one row in each table: the
# first has a few of everything, the second the least the input allows.
SAMPLE_INPUTS: dict[str, object] = {
    "points": (
        Keyframe(t=0, value=0, ease="in-out"),
        Keyframe(t=4, value=1, ease="overshoot"),
        Keyframe(t=8, value=-0.5),
    ),
    # the last is after CHANGE_AT, so a shaper that reads the input at events sees the change in its output
    "events": (
        Event(t=0.5, strength=1.0),
        Event(t=1.0, strength=0.6),
        Event(t=1.5, strength=1.0),
        Event(t=3.5, strength=0.4),
    ),
    # a steady 120 BPM: a beat every half second, 40 of them (20 s)
    "grid": Grid(beats=tuple(index * 0.5 for index in range(40)), meter=4, first_downbeat=0),
    "values": (0.2, -0.5, 1.0, 0.7),
    "series": Series(rate=10, values=(0.2, 1.0, 0.0, 0.6, 0.6), unit=Unit.RATIO),
}
SMALLEST_INPUTS: dict[str, object] = {
    "points": (Keyframe(t=0, value=0.5),),
    "events": (),
    "grid": Grid(beats=(0.0, 0.5), meter=1, first_downbeat=0),
    "values": (0.5,),
    "series": Series(rate=1, values=(0.5,), unit=Unit.RATIO),
}


def make(
    name: str,
    inputs: dict[str, object],
    values: dict[str, object],
    leave_out: str | None = None,
    required_only: bool = False,
    tail: float = 0.0,
) -> Any:
    """The thing `name` made from every input it declares (only the required ones if `required_only`), taken from the
    table `inputs` (a kind without a row is a KeyError: the tables are complete), except `leave_out`, and parameter
    `values`; a shaper's signal is `input_signal(tail)`. A signal for a source or shaper, the data for an operation."""
    thing = registry.get(name)
    given = {
        kind: inputs[kind]
        for kind, required in thing.inputs.items()
        if kind != leave_out and (required or not required_only)
    }
    return thing(input_signal(tail), **given, **values) if thing.kind == "shaper" else thing(**given, **values)


def build(name: str, **values) -> Any:
    return make(name, SAMPLE_INPUTS, values)


TIMES = np.linspace(0, 10, 101)


def test_the_installed_package_provides_its_sources_shapers_and_operations():
    assert {"constant"} <= set(registry.sources())
    assert {"clip", "slew"} <= set(registry.shapers())
    assert {"select"} <= set(registry.operations())


@pytest.mark.parametrize("name", ALL)
def test_every_one_documents_itself(name):
    thing = registry.get(name)
    assert thing.summary
    assert thing.params_model.__doc__


@pytest.mark.parametrize("name", SIGNALS)
@pytest.mark.parametrize(
    "given", [3.0, [], [0.0, 1.0, 2.5], [[0.0, 1.0], [2.0, 3.0]]], ids=["scalar", "empty", "list", "matrix"]
)
def test_every_one_makes_a_signal_that_keeps_the_shape_of_the_times_and_is_finite(name, given):
    values = build(name).sample(given)
    assert values.shape == np.shape(given)
    assert np.isfinite(values).all()


@pytest.mark.parametrize("name", SIGNALS)
def test_every_one_gives_finite_values_from_the_smallest_inputs_it_accepts(name):
    # the least a call can give: optional inputs left out
    assert np.isfinite(make(name, SMALLEST_INPUTS, {}, required_only=True).sample(TIMES)).all()


DECLARED_KINDS = sorted({kind for name in ALL for kind in registry.get(name).inputs})


@pytest.mark.parametrize("kind", DECLARED_KINDS)
def test_every_kind_of_input_a_registered_thing_declares_has_a_row_in_both_tables(kind):
    # `make` looks up a row for every input it supplies; this names the kind that lacks one instead of a bare KeyError
    assert kind in SAMPLE_INPUTS
    assert kind in SMALLEST_INPUTS


REQUIRED_INPUTS = [(name, kind) for name in ALL for kind, required in registry.get(name).inputs.items() if required]


@pytest.mark.parametrize(("name", "kind"), REQUIRED_INPUTS)
def test_every_required_input_left_out_is_a_type_error_naming_it(name, kind):
    with pytest.raises(TypeError, match=kind):
        make(name, SAMPLE_INPUTS, {}, leave_out=kind)


@pytest.mark.parametrize("name", SIGNALS)
def test_every_one_gives_the_same_values_when_read_again_and_when_built_again(name):
    first, second = build(name), build(name)
    assert first.sample(TIMES).tolist() == first.sample(TIMES).tolist() == second.sample(TIMES).tolist()


@pytest.mark.parametrize("name", SIGNALS)
def test_every_one_gives_the_same_values_whatever_the_order_and_size_of_the_reads(name):
    expected = build(name).sample(TIMES)
    one_by_one = build(name)
    shuffled = np.random.default_rng(7).permutation(len(TIMES))
    singles = np.array([one_by_one.sample(TIMES[i]) for i in shuffled])
    assert singles[np.argsort(shuffled)].tolist() == expected.tolist()
    chunked = build(name)
    parts = [chunked.sample(chunk) for chunk in np.array_split(TIMES, 7)[::-1]]
    assert np.concatenate(parts[::-1]).tolist() == expected.tolist()


# up to a millionth of a second short of the change: closer than that is inside the float rounding every read allows
BEFORE_THE_CHANGE = np.concatenate([np.linspace(0, 2.9, 30), [2.99, 2.9995, 2.999999]])


@pytest.mark.parametrize("name", SHAPERS)
def test_every_shaper_is_causal_what_comes_later_in_its_input_never_changes_an_earlier_output(name):
    unchanged = make(name, SAMPLE_INPUTS, {}, tail=0.0)
    changed = make(name, SAMPLE_INPUTS, {}, tail=7.0)
    assert changed.sample(BEFORE_THE_CHANGE).tolist() == unchanged.sample(BEFORE_THE_CHANGE).tolist()
    # the test can fail: after the change the two outputs do differ
    later = np.array([CHANGE_AT + 1.0, 8.0])
    assert changed.sample(later).tolist() != unchanged.sample(later).tolist()


@pytest.mark.parametrize("name", SIGNALS)
@pytest.mark.parametrize("time", [1e12, 1e19, 1.7e306, 1.797e308, -1e300], ids=str)
def test_every_one_gives_a_finite_value_or_a_signal_error_at_any_time_never_a_warning_or_a_crash(name, time):
    try:
        values = build(name).sample([time])
    except SignalError:
        return
    assert np.isfinite(values).all()


def numeric_fields(name):
    return [field for field in registry.get(name).params_model.fields_doc() if field.minimum is not None]


def value_at(field, bound):
    return int(bound) if isinstance(field.default, int) else float(bound)


NUMBERS = sorted({(name, field.name) for name in ALL for field in numeric_fields(name)})
# a cyclic number takes any real (it wraps), so it has no "beyond its range"
LIMITS = [
    (name, field.name, side)
    for name in ALL
    for field in numeric_fields(name)
    if not field.cyclic
    for side in ("minimum", "maximum")
]


def limits_at(name, side):
    # all at one end, not one at a time: a rule across fields (a clip's low may not pass its high) refuses some mixes
    return {field.name: value_at(field, getattr(field, side)) for field in numeric_fields(name)}


@pytest.mark.parametrize("name", SIGNALS)
@pytest.mark.parametrize("side", ["minimum", "maximum"])
def test_every_signal_works_with_all_its_numbers_at_the_same_end_of_their_ranges(name, side):
    assert np.isfinite(build(name, **limits_at(name, side)).sample(TIMES)).all()


def events_are_valid(result):
    assert type(result) is tuple
    assert tuple(Event.parse(event.model_dump(mode="json")) for event in result) == result
    assert [event.t for event in result] == sorted(event.t for event in result)


def series_is_valid(result):
    assert isinstance(result, Series)
    assert Series.parse(result.model_dump(mode="json")) == result


VALID = {"events": events_are_valid, "series": series_is_valid}


@pytest.mark.parametrize("name", OPERATIONS)
def test_every_operation_gives_valid_data_from_the_smallest_inputs_it_accepts(name):
    # the least a call can give: no events, a grid of two beats, a series of one value, optional inputs left out
    VALID[registry.get(name).returns](make(name, SMALLEST_INPUTS, {}, required_only=True))


@pytest.mark.parametrize("name", OPERATIONS)
@pytest.mark.parametrize("case", ["defaults", "minimum", "maximum"])
def test_every_operation_gives_the_valid_data_it_declares_with_its_defaults_and_with_all_numbers_at_one_end(name, case):
    numbers = {} if case == "defaults" else limits_at(name, case)
    VALID[registry.get(name).returns](build(name, **numbers))


@pytest.mark.parametrize(("name", "field", "side"), LIMITS)
def test_every_number_beyond_its_range_is_refused(name, field, side):
    doc = next(f for f in numeric_fields(name) if f.name == field)
    beyond = getattr(doc, side) + (1 if side == "maximum" else -1)
    with pytest.raises(ParameterError, match=field):
        build(name, **{field: value_at(doc, beyond)})


@pytest.mark.parametrize(("name", "field"), NUMBERS)
@pytest.mark.parametrize("wrong", [True, "0.5", None], ids=["a bool", "a string", "nothing"])
def test_every_number_refuses_what_is_not_a_number(name, field, wrong):
    with pytest.raises(ParameterError, match=field):
        build(name, **{field: wrong})


@pytest.mark.parametrize("name", ALL)
def test_every_one_refuses_an_unknown_parameter(name):
    with pytest.raises(ParameterError, match="speed"):
        build(name, speed=1)


# Only signals are checked: no operation has a cyclic number yet, and one that gets it adds its own comparison here.
CYCLIC = [(name, field.name) for name in SIGNALS for field in numeric_fields(name) if field.cyclic]


def test_a_cyclic_number_exists_for_the_tests_below_to_run_over():
    assert ("lfo", "phase") in CYCLIC


@pytest.mark.parametrize(("name", "field"), CYCLIC)
def test_a_cyclic_number_at_the_top_of_its_range_gives_what_the_bottom_gives(name, field):
    doc = next(f for f in numeric_fields(name) if f.name == field)
    bottom = build(name, **{field: doc.minimum}).sample(TIMES)
    top = build(name, **{field: doc.maximum}).sample(TIMES)
    assert top.tolist() == pytest.approx(bottom.tolist(), abs=1e-9)


@pytest.mark.parametrize(("name", "field"), CYCLIC)
@pytest.mark.parametrize("cycles", [-3, -1, 1, 7])
def test_a_cyclic_number_a_whole_number_of_cycles_away_gives_the_same_signal(name, field, cycles):
    doc = next(f for f in numeric_fields(name) if f.name == field)
    inside = doc.minimum + 0.25
    here = build(name, **{field: inside}).sample(TIMES)
    away = build(name, **{field: inside + cycles}).sample(TIMES)
    assert away.tolist() == pytest.approx(here.tolist(), abs=1e-9)
    # the test can fail: a quarter of a cycle on is a different signal
    other = build(name, **{field: inside + 0.25}).sample(TIMES)
    assert other.tolist() != pytest.approx(here.tolist(), abs=1e-9)
