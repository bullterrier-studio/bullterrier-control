"""delay: make a signal trail its input by a fixed time: a light that follows a beat a moment late."""

import numpy as np
from numpy.typing import NDArray

from ..plugin import Params, Signal, shaper


class DelayParams(Params, builtin="delay"):
    """How far the output trails its input."""


class _Delayed(Signal):
    """The input as it was `lag` seconds earlier."""

    def __init__(self, signal: Signal, lag: float) -> None:
        self._signal = signal
        self._lag = lag

    def compute(self, times: NDArray[np.float64]) -> NDArray[np.float64]:
        return self._signal.compute(np.maximum(times - self._lag, 0.0))


@shaper(params=DelayParams)
def delay(signal: Signal, params: DelayParams) -> Signal:
    """A signal that shows its input `lag` seconds late.

    The value at time t is the input at t - lag; until `lag` seconds have passed that is the input at time 0 (a signal
    has no values before 0). A signal can only lag: a lead would read the input's future, and exists for events only.
    """
    return _Delayed(signal, params.lag)
