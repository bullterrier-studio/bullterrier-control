"""steps: a step sequencer: a list of values stepped through at a rate, free-running or on the beat."""

from typing import Literal

import numpy as np
from numpy.typing import NDArray

from ..plugin import Finite, Grid, Params, Positions, Signal, counter, param, source


class StepsParams(Params):
    """How fast the list of values is stepped through."""

    rate: Finite = param(
        4.0,
        doc=("Steps per second, or per beat if `per` is 'beat'; each step holds one value. Raising it steps faster."),
        unit="count",
        min=0.001,
        max=1000,
    )
    per: Literal["second", "beat"] = param(
        "second",
        doc=(
            "What `rate` counts in: 'second' (free-running) or 'beat' (follows the tempo of the `grid`, which it"
            " needs)."
        ),
    )


class _Steps(Signal):
    """The value of the step each read falls in, going round the list."""

    def __init__(self, values: tuple[float, ...], position: Positions) -> None:
        self._values = np.array(values)
        self._position = position

    def compute(self, times: NDArray[np.float64]) -> NDArray[np.float64]:
        # in floats until the last moment: a position beyond 2^53 is a whole number, and mod keeps a negative one
        # (before a first beat that is not at 0) counting back from the end of the list
        step = np.mod(np.floor(self._position(times)), len(self._values))
        return self._values[step.astype(np.intp)]


@source(params=StepsParams)
def steps(params: StepsParams, *, values: tuple[float, ...], grid: Grid | None = None) -> Signal:
    """A signal that steps through `values`, holding each for one step, and goes back to the first after the last.

    The position is rate * t, with t counted in seconds, or, if `per` is 'beat', in beats of `grid` (so it follows a
    tempo change, and a time before the first beat or after the last counts beats of the first or last beat's
    length). The value is the one at index floor(position) mod len(values); a step starts where that position reaches
    a whole number, to within float rounding (of rate * t, and per beat of the grid's seconds-to-beats conversion).
    `grid` is needed only per beat and ignored per second. A time too far from 0 for the position to be a number is a
    `SignalError`.
    """
    return _Steps(values, counter(params.rate, params.per, grid))
