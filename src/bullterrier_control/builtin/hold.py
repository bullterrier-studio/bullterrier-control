"""hold: sample and hold at events: a value that changes only when something happens."""

import numpy as np
from numpy.typing import NDArray

from ..plugin import Event, Params, Signal, shaper


class HoldParams(Params):
    """Hold has no parameters: the events say when the value is taken."""


class _Held(Signal):
    """The input as it was at the latest event."""

    def __init__(self, signal: Signal, events: tuple[Event, ...]) -> None:
        # time 0 comes first: until the first event the input at time 0 is held, and no read needs a branch for it
        self._signal = signal
        self._moments = np.array([0.0, *(event.t for event in events)])

    def compute(self, times: NDArray[np.float64]) -> NDArray[np.float64]:
        # "right": a read exactly at an event belongs to it; the moments are in order, since the events input sorts them
        latest = np.searchsorted(self._moments, times, side="right") - 1
        return self._signal.compute(self._moments[latest])


@shaper(params=HoldParams)
def hold(signal: Signal, params: HoldParams, *, events: tuple[Event, ...]) -> Signal:
    """A signal that takes its input's value at each event and keeps it until the next event.

    The value at time t is the input at the time of the latest event at or before t: at exactly an event's time it
    is already that event's value. Before the first event, and when there are no events, it is the input at time 0.
    Events at the same time are one; their strength, duration, label and pitch change nothing (choose events first
    with an operation on them). Quantizing a signal to the beat grid is this, with the grid's beats as events.
    """
    return _Held(signal, events)
