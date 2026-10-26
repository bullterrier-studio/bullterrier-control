"""What every registered operation promises, checked over all of them, the installed ones included.

An architecture test of the extension point, like `test_contract.py`: its subject is every operation the registry
lists, so a new one is covered the moment it is registered and no test names it. (That an operation's data is valid at
the ends of its parameters' ranges is in `test_contract.py`, beside the other parameter tests.)
"""

import pytest

from bullterrier_control import Event, Grid, ParameterError, Series, Unit, registry

OPERATIONS = sorted(registry.operations())

# Events with distinct times, in time order, with a label, a duration and a pitch on some: reversing them must change
# nothing. (Events at the same time keep the order given, so an operation's own tests cover ties.)
EVENTS = (
    Event(t=0.25, strength=0.9, label="kick"),
    Event(t=0.5, strength=0.3, label="snare", duration=0.25),
    Event(t=1.0, strength=0.6),
    Event(t=1.3, strength=1.0, label="kick", pitch=36),
    Event(t=2.0, strength=0.05, label="hat"),
)
INPUTS = {
    "events": EVENTS,
    "grid": Grid(beats=(0.0, 0.5, 1.1, 1.8, 2.6), meter=4, first_downbeat=1),
    "series": Series(rate=10, values=(0.2, 1.0, 0.0, 0.6, 0.6, 0.9), unit=Unit.RATIO),
}
WITH_EVENTS = [name for name in OPERATIONS if "events" in registry.get(name).inputs]


def call(name, **changed):
    """The operation `name` called with every input it declares, taken from INPUTS except those in `changed`."""
    thing = registry.get(name)
    return thing(**{kind: changed.get(kind, INPUTS[kind]) for kind in thing.inputs})


def as_dicts(events):
    return [event.model_dump(exclude_none=True) for event in events]


@pytest.mark.parametrize("kind", sorted({kind for name in OPERATIONS for kind in registry.get(name).inputs}))
def test_every_kind_of_input_an_operation_declares_has_a_row_in_the_table(kind):
    # `call` looks up a row for every input it supplies; this names the kind that lacks one instead of a bare KeyError
    assert kind in INPUTS


@pytest.mark.parametrize("name", OPERATIONS)
def test_every_operation_gives_the_same_data_when_called_again(name):
    assert call(name) == call(name)


@pytest.mark.parametrize("name", WITH_EVENTS)
def test_every_operation_gives_the_same_data_for_events_given_reversed(name):
    assert call(name, events=EVENTS[::-1]) == call(name)


@pytest.mark.parametrize("name", WITH_EVENTS)
def test_every_operation_gives_the_same_data_for_events_given_as_dicts(name):
    assert call(name, events=as_dicts(EVENTS)) == call(name)


@pytest.mark.parametrize("name", WITH_EVENTS)
def test_every_operation_refuses_a_bad_event_naming_its_place_in_the_list(name, reports):
    broken = [*as_dicts(EVENTS[:1]), {"t": 1.0, "strength": 1.5}]
    with pytest.raises(ParameterError) as refused:
        call(name, events=broken)
    reports(refused.value, "events[1].strength")
