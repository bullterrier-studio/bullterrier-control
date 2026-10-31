"""every: keep every Nth event: a flash on every second kick, a cut on every fourth beat."""

from ..plugin import Event, Params, operation


class EveryParams(Params, builtin="every"):
    """Which of the events in time order to keep: every `n`th, counting from `offset`."""


@operation(params=EveryParams)
def every(params: EveryParams, *, events: tuple[Event, ...]) -> tuple[Event, ...]:
    """The events at positions `offset`, `offset + n`, `offset + 2n`, ... of the events in time order.

    Events at the same time count one by one, in the order given. Every field of a kept event is unchanged.
    """
    return events[params.offset :: params.n]
