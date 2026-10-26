"""select: keep the events with a label and at least a strength: the kicks that are loud enough to matter."""

from ..plugin import Event, Finite, Params, Text, matching, operation, param


class SelectParams(Params):
    """Which events to keep: by label and by strength."""

    label: Text = param("", doc="Keep only events with this label; empty keeps every event, unlabelled ones too.")
    threshold: Finite = param(
        0.0,
        doc=(
            "The weakest strength kept, 0 to 1: an event is kept when its strength is at least this. Raising it keeps"
            " only the stronger events; 0 keeps them all."
        ),
        unit="fraction",
        min=0,
        max=1,
    )


@operation(params=SelectParams)
def select(params: SelectParams, *, events: tuple[Event, ...]) -> tuple[Event, ...]:
    """The events with `label` (all if it is empty) whose strength is at least `threshold`, in time order.

    A strength equal to the threshold is kept. Every other field of a kept event is unchanged.
    """
    return tuple(event for event in matching(events, params.label) if event.strength >= params.threshold)
