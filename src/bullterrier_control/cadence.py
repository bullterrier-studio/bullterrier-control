"""Where a read falls in a periodic source: its position, counted in cycles or steps per second or per beat.

    position = counter(rate=2.0, per="beat", grid=grid)   # reading a time gives how many cycles have passed
    position(times)                                         # float64, the shape of `times`

Promises:
- `counter` (the Rust core's `cadence::positions`) returns a function from times (finite, from 0 up; a number or an
  array of any shape) to positions: `rate * t` for `per="second"`, `rate * grid.beat_at(t)` for `per="beat"`, so a
  source follows a tempo change. The result is always finite; its whole part says which cycle or step, its fraction
  where in it. It is negative for a time before a first beat that is not at 0. A step or cycle edge sits where
  `rate * t` reaches a whole number, to within float rounding: a time that is a multiple of 1 / rate in decimal
  (frame times at rate 30) may read the step before.
- `per="beat"` without a grid is a `ParameterError` naming `per`, raised by `counter`; a grid is ignored per second.
- A time too far for the position to be a number (`rate * t` overflows, or the grid cannot reach it) is a `SignalError`.
"""

from collections.abc import Callable

import numpy as np
from numpy.typing import NDArray

from ._native import _core

__all__ = ["Positions", "counter"]

Positions = Callable[[NDArray[np.float64]], NDArray[np.float64]]
counter = _core.counter
