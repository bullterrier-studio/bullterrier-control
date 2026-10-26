"""delay: make a signal trail its input by a fixed time: a light that follows a beat a moment late."""

import numpy as np
from numpy.typing import NDArray

from ..plugin import Finite, Params, Signal, param, shaper


class DelayParams(Params):
    """How far the output trails its input."""

    lag: Finite = param(
        0.1,
        doc="Seconds the output trails its input, 0 to 3600; raising it shows the input later. 0 passes it through.",
        unit="seconds",
        min=0,
        max=3600,
    )


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
