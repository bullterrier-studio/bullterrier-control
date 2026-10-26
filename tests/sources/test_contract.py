"""What every registered source and shaper promises, checked over all of them, the installed ones included.

An architecture test of the extension point: its subject is every thing the registry lists, so a new source or shaper is
covered the moment it is registered and no test names it.
"""

import numpy as np
import pytest

from bullterrier_control import FormatError, Series, Signal, SignalError, Unit, registry
from bullterrier_control.signal import from_series

ALL = sorted(registry.all_registered())
SHAPERS = sorted(registry.shapers())
# a signal with every kind of movement: a rise, a plateau, a fall, a jump, a drop below zero
HISTORY = (0.0, 0.2, 1.0, 1.0, 0.4, 2.0, -1.0, 0.5, 0.5, 3.0)
CHANGE_AT = 3.0  # seconds: from here on the input differs between the two inputs the causality test compares


def input_signal(tail: float = 0.0) -> Signal:
    """A 10 s series at 10 Hz; `tail` is added to every sample from CHANGE_AT on, to tell what depends on the future."""
    values = tuple(HISTORY[index % len(HISTORY)] + (tail if index / 10.0 >= CHANGE_AT else 0.0) for index in range(100))
    return from_series(Series(rate=10.0, values=values, unit=Unit.RATIO), interpolation="hold")


def build(name: str, **values) -> Signal:
    thing = registry.get(name)
    return thing(input_signal(), **values) if thing.kind == "shaper" else thing(**values)


TIMES = np.linspace(0, 10, 101)


def test_the_installed_package_provides_its_sources_and_shapers():
    assert {"constant"} <= set(registry.sources())
    assert {"clip", "slew"} <= set(registry.shapers())


@pytest.mark.parametrize("name", ALL)
def test_every_one_documents_itself(name):
    thing = registry.get(name)
    assert thing.summary
    assert thing.params_model.__doc__


@pytest.mark.parametrize("name", ALL)
@pytest.mark.parametrize(
    "given", [3.0, [], [0.0, 1.0, 2.5], [[0.0, 1.0], [2.0, 3.0]]], ids=["scalar", "empty", "list", "matrix"]
)
def test_every_one_makes_a_signal_that_keeps_the_shape_of_the_times_and_is_finite(name, given):
    values = build(name).sample(given)
    assert values.shape == np.shape(given)
    assert np.isfinite(values).all()


@pytest.mark.parametrize("name", ALL)
def test_every_one_gives_the_same_values_when_read_again_and_when_built_again(name):
    first, second = build(name), build(name)
    assert first.sample(TIMES).tolist() == first.sample(TIMES).tolist() == second.sample(TIMES).tolist()


@pytest.mark.parametrize("name", ALL)
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
    thing = registry.get(name)
    unchanged = thing(input_signal(0.0)).sample(BEFORE_THE_CHANGE)
    changed = thing(input_signal(7.0)).sample(BEFORE_THE_CHANGE)
    assert changed.tolist() == unchanged.tolist()
    # the test can fail: after the change the two outputs do differ
    later = np.array([CHANGE_AT + 1.0, 8.0])
    assert thing(input_signal(7.0)).sample(later).tolist() != thing(input_signal(0.0)).sample(later).tolist()


@pytest.mark.parametrize("name", ALL)
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


LIMITS = [(name, field.name, side) for name in ALL for field in numeric_fields(name) for side in ("minimum", "maximum")]


@pytest.mark.parametrize("name", ALL)
@pytest.mark.parametrize("side", ["minimum", "maximum"])
def test_every_one_works_with_all_its_numbers_at_the_same_end_of_their_ranges(name, side):
    # all at one end, not one at a time: a rule across fields (a clip's low may not pass its high) refuses some mixes
    limits = {field.name: value_at(field, getattr(field, side)) for field in numeric_fields(name)}
    assert np.isfinite(build(name, **limits).sample(TIMES)).all()


@pytest.mark.parametrize(("name", "field", "side"), LIMITS)
def test_every_number_beyond_its_range_is_refused(name, field, side):
    doc = next(f for f in numeric_fields(name) if f.name == field)
    beyond = getattr(doc, side) + (1 if side == "maximum" else -1)
    with pytest.raises(FormatError, match=field):
        build(name, **{field: value_at(doc, beyond)})


@pytest.mark.parametrize(("name", "field"), sorted({(name, field) for name, field, _ in LIMITS}))
@pytest.mark.parametrize("wrong", [True, "0.5", None], ids=["a bool", "a string", "nothing"])
def test_every_number_refuses_what_is_not_a_number(name, field, wrong):
    with pytest.raises(FormatError, match=field):
        build(name, **{field: wrong})


@pytest.mark.parametrize("name", ALL)
def test_every_one_refuses_an_unknown_parameter(name):
    with pytest.raises(FormatError, match="speed"):
        build(name, speed=1)
