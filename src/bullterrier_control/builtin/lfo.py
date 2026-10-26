"""lfo: a periodic wave, free-running or following the beat: a pulse on every beat, a slow drift."""

from typing import Literal

import numpy as np
from numpy.typing import NDArray

from ..plugin import Finite, Grid, Params, Positions, Signal, counter, param, source


class LfoParams(Params):
    """The wave: its shape, how fast it repeats and where in the cycle it starts, and the range it moves in."""

    shape: Literal["sine", "triangle", "square", "saw", "saw-down"] = param(
        "sine",
        doc=(
            "The wave: 'sine' (smooth), 'triangle' (straight up and down), 'square' (high, then low), 'saw' (rises,"
            " then jumps back) or 'saw-down' (falls, then jumps back)."
        ),
    )
    rate: Finite = param(
        1.0,
        doc=(
            "Cycles per second, or per beat if `per` is 'beat'; 0 holds the wave still at `phase`. Raising it makes"
            " the wave faster."
        ),
        unit="cycles",
        min=0,
        max=1000,
    )
    per: Literal["second", "beat"] = param(
        "second",
        doc=(
            "What `rate` counts in: 'second' (free-running) or 'beat' (follows the tempo of the `grid`, which it"
            " needs)."
        ),
    )
    phase: Finite = param(
        0.0,
        doc="Where in the cycle time 0 falls, as a share of a cycle, 0 to 1: 0.25 starts a quarter of the way in.",
        unit="cycles",
        min=0,
        max=1,
    )
    low: Finite = param(
        0.0,
        doc="The value at the bottom of the wave; set above `high` to flip the wave upside down.",
        unit="ratio",
        min=-1e6,
        max=1e6,
    )
    high: Finite = param(
        1.0,
        doc="The value at the top of the wave.",
        unit="ratio",
        min=-1e6,
        max=1e6,
    )
    duty: Finite = param(
        0.5,
        doc="Square only: the share of each cycle spent high, 0 to 1; 0 is never high, 1 always.",
        unit="fraction",
        min=0,
        max=1,
    )


class _Lfo(Signal):
    """The wave of `params` read at the positions `position` gives."""

    def __init__(self, params: LfoParams, position: Positions) -> None:
        self._params = params
        self._position = position

    def compute(self, times: NDArray[np.float64]) -> NDArray[np.float64]:
        params = self._params
        cycle = np.mod(self._position(times) + params.phase, 1.0)
        # a float modulo of a tiny negative number rounds to exactly 1.0, which is the end of a cycle, not its start:
        # one step below it, where a square with duty 1 is still high and a saw is at the top of its ramp
        cycle = np.where(cycle == 1.0, np.nextafter(1.0, 0.0), cycle)
        low, high = params.low, params.high
        values = low + (high - low) * _wave(params.shape, cycle, params.duty)
        # low + (high - low) * 1 can land an ulp beyond high: the range is a promise, so it is kept exactly
        return np.clip(values, min(low, high), max(low, high))


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
