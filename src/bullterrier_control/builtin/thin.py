"""thin: drop events that come too soon after the last one kept: a flash that cannot fire twice in 0.1 s."""

from ..plugin import Event, Params, at_least, operation


class ThinParams(Params, builtin="thin"):
    """The shortest time allowed between two kept events."""


@operation(params=ThinParams)
def thin(params: ThinParams, *, events: tuple[Event, ...]) -> tuple[Event, ...]:
    """The events, from the earliest on, that are at least `gap` seconds after the last one kept.

    The earliest event is always kept; a later one is kept when its time minus the time of the last kept event is at
    least `gap`, so a dropped event never moves the reference. Of events at the same time the first given is kept (for
    a `gap` above 0). Every field of a kept event is unchanged. "At least `gap`" holds to within float rounding (a
    nanosecond), so 0.1 and 0.3 are a gap of 0.2 apart, though 0.3 - 0.1 is a hair under it in floats.
    """
    kept: list[Event] = []
    for event in events:
        if not kept or at_least(event.t - kept[-1].t, params.gap):
            kept.append(event)
    return tuple(kept)
