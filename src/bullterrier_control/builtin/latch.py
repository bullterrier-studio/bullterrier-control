"""latch: switch once, at the first matching event: the wake at a drop, a scene change on the first note."""

import numpy as np
from numpy.typing import NDArray

from ..plugin import Event, Params, Signal, matching, source


class LatchParams(Params, builtin="latch"):
    """Which events count, and the level before and after the first of them."""


class _Latched(Signal):
    """`before` up to the switch time, `after` from it on."""

    def __init__(self, switch: float, before: float, after: float) -> None:
        self._switch = switch
        self._before = before
        self._after = after

    def compute(self, times: NDArray[np.float64]) -> NDArray[np.float64]:
        return np.where(times >= self._switch, self._after, self._before)


@source(params=LatchParams)
def latch(params: LatchParams, *, events: tuple[Event, ...]) -> Signal:
    """A signal that is `before` until the first event with `label` and `after` from the moment of that event on.

    At exactly the event's time the value is already `after`. The first event is the earliest by time, whatever the
    order of `events`; their strength, duration and the later events change nothing, and it never switches back. With
    no matching event (an empty `label` matches every event) the value is `before` always.
    """
    # no match switches at infinity, which no read reaches: the same comparison serves
    switch = min((event.t for event in matching(events, params.label)), default=float("inf"))
    return _Latched(switch, params.before, params.after)
