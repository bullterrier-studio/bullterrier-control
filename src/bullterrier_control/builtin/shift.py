"""shift: move events earlier or later in time: a flash that leads its kick, a cue that lags the beat."""

from ..plugin import Event, Params, at_least, more_than, operation


class ShiftParams(Params, builtin="shift"):
    """How far to move the events."""


@operation(params=ShiftParams)
def shift(params: ShiftParams, *, events: tuple[Event, ...]) -> tuple[Event, ...]:
    """The events with every time moved by `offset`, without what would come before time 0.

    A moment that lands before 0 is dropped (one landing at 0 is kept). An event with a duration that starts before 0
    and ends after it starts at 0 with the rest of its duration; one that ends at or before 0 is dropped. Landing at 0
    holds to within float rounding (a nanosecond): 0.1 + 0.2 - 0.3 is 0, not a span of 3e-17 s. Every other field is
    unchanged, and so is the order.
    """
    moved: list[Event] = []
    for event in events:
        start = event.t + params.offset
        if at_least(start, 0.0):
            moved.append(event.replace(t=max(start, 0.0)))
        elif event.duration is not None and more_than(start + event.duration, 0.0):
            moved.append(event.replace(t=0.0, duration=start + event.duration))
    return tuple(moved)
