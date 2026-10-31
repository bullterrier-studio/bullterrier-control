"""lfo: a periodic wave, free-running or following the beat: a pulse on every beat, a slow drift."""

import numpy as np
from numpy.typing import NDArray

from ..plugin import Grid, Params, Positions, Signal, between, counter, source


class LfoParams(Params, builtin="lfo"):
    """The wave: its shape, how fast it repeats and where in the cycle it starts, and the range it moves in."""


class _Lfo(Signal):
    """The wave of `params` read at the positions `position` gives."""

    def __init__(self, params: LfoParams, position: Positions) -> None:
        self._params = params
        self._position = position

    def compute(self, times: NDArray[np.float64]) -> NDArray[np.float64]:
        params = self._params
        # a phase of 1 is the phase of 0: reduced first, so adding a whole cycle cannot move a time across a cycle edge
        cycle = np.mod(self._position(times) + params.phase % 1.0, 1.0)
        # a float modulo of a tiny negative number rounds to exactly 1.0, which is the end of a cycle, not its start:
        # one step below it, where a square with duty 1 is still high and a saw is at the top of its ramp
        cycle = np.where(cycle == 1.0, np.nextafter(1.0, 0.0), cycle)
        return between(params.low, params.high, _wave(params.shape, cycle, params.duty))


def _wave(shape: str, cycle: NDArray[np.float64], duty: float) -> NDArray[np.float64]:
    """The wave on 0..1 at each point of the cycle (`cycle` in [0, 1))."""
    match shape:
        case "sine":
            return 0.5 + 0.5 * np.sin(2.0 * np.pi * cycle)  # 2 pi: one full turn per cycle
        case "triangle":
            return 1.0 - np.abs(2.0 * cycle - 1.0)
        case "square":
            return np.where(cycle < duty, 1.0, 0.0)
        case "saw":
            return cycle
        case _:  # "saw-down"
            return 1.0 - cycle


@source(params=LfoParams)
def lfo(params: LfoParams, *, grid: Grid | None = None) -> Signal:
    """A periodic wave between `low` and `high`, repeating `rate` times per second or per beat.

    At time t the position in the cycle is x = (rate * t + phase) mod 1, with t counted in seconds, or, if `per` is
    'beat', in beats of `grid` (so it follows a tempo change, and a time before the first beat or after the last
    counts beats of the first or last beat's length). The wave on 0..1 is: sine 0.5 + 0.5 sin(2 pi x); triangle
    1 - |2x - 1|; square 1 while x < duty, else 0; saw x; saw-down 1 - x. The value is low + (high - low) times the
    wave, and never leaves the range between `low` and `high`. `grid` is needed only per beat and ignored per second.
    A time too far from 0 for the position to be a number is a `SignalError`.
    """
    return _Lfo(params, counter(params.rate, params.per, grid))
