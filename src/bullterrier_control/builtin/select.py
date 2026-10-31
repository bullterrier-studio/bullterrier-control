"""select: keep the events with a label and at least a strength: the kicks that are loud enough to matter."""

from ..plugin import Event, Params, matching, operation


class SelectParams(Params, builtin="select"):
    """Which events to keep: by label and by strength."""


@operation(params=SelectParams)
def select(params: SelectParams, *, events: tuple[Event, ...]) -> tuple[Event, ...]:
    """The events with `label` (all if it is empty) whose strength is at least `threshold`, in time order.

    A strength equal to the threshold is kept. Every other field of a kept event is unchanged.
    """
    return tuple(event for event in matching(events, params.label) if event.strength >= params.threshold)
