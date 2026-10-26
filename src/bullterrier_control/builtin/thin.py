"""thin: drop events that come too soon after the last one kept: a flash that cannot fire twice in 0.1 s."""

from ..plugin import Event, Finite, Params, operation, param


class ThinParams(Params):
    """The shortest time allowed between two kept events."""

    gap: Finite = param(
        0.1,
        doc=(
            "The least time between two kept events, in seconds, 0 to 3600: an event is kept when it comes at least"
            " this long after the last kept one. Raising it keeps fewer events; 0 keeps all."
        ),
        unit="seconds",
        min=0,
        max=3600,
    )


@operation(params=ThinParams)
def thin(params: ThinParams, *, events: tuple[Event, ...]) -> tuple[Event, ...]:
    """The events, from the earliest on, that are at least `gap` seconds after the last one kept.

    The earliest event is always kept; a later one is kept when its time minus the time of the last kept event is at
    least `gap`, so a dropped event never moves the reference. Of events at the same time the first given is kept (for
    a `gap` above 0). Every field of a kept event is unchanged.
    """
    kept: list[Event] = []
    for event in events:
        if not kept or event.t - kept[-1].t >= params.gap:
            kept.append(event)
    return tuple(kept)
