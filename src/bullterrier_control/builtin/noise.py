"""noise: seeded random values, held, straight or smooth between them: a wobble, a drift, a flicker."""

import numpy as np
from numpy.typing import NDArray

from ..plugin import Params, Positions, Signal, between, counter, source, uniform

# a position is reduced to the index the hash reads: whole numbers wrap at 2^64 (and the float 2^64 is exact)
_INDEX_PERIOD = 2.0**64


class NoiseParams(Params, builtin="noise"):
    """The noise: how it moves between its random values, how often it draws a new one, and its seed and range."""


class _Noise(Signal):
    """The random values of `params.seed` at whole positions, joined as `params.shape` says."""

    def __init__(self, params: NoiseParams, position: Positions) -> None:
        self._params = params
        self._position = position

    def compute(self, times: NDArray[np.float64]) -> NDArray[np.float64]:
        params = self._params
        position = self._position(times)
        whole = np.floor(position)
        fraction = position - whole
        first = np.mod(whole, _INDEX_PERIOD).astype(np.uint64)
        here = uniform(params.seed, first)
        if params.shape == "hold":
            return between(params.low, params.high, here)
        there = uniform(params.seed, first + np.uint64(1))
        # 3x^2 - 2x^3: the smoothstep curve, 0 at 0 and 1 at 1, with no slope at either end
        progress = fraction * fraction * (3.0 - 2.0 * fraction) if params.shape == "smooth" else fraction
        return between(params.low, params.high, here + (there - here) * progress)


@source(params=NoiseParams)
def noise(params: NoiseParams) -> Signal:
    """Seeded random noise between `low` and `high`: a new random value every 1 / `rate` seconds, held, joined by
    straight lines or eased between them.

    The random value at whole position k (k = floor(rate * t)) is the k-th number of the stream of `seed`
    (SplitMix64, mapped to 0..1), so the signal is a pure function of seed and time: it can be read at any time in any
    order and is the same on every machine. A read at position k + x (x from 0 to 1) is the value at k for 'hold', the
    straight line from k to k + 1 for 'linear', or that line eased by 3x^2 - 2x^3 for 'smooth', which has no slope at
    a lattice point. The value is low + (high - low) times that, and never leaves the range between `low` and `high`.
    A time too far from 0 for the position to be a number is a `SignalError`.
    """
    return _Noise(params, counter(params.rate, "second", None))
