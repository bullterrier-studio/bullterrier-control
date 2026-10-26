"""Where a read falls in a periodic source: its position, counted in cycles or steps per second or per beat.

    position = counter(rate=2.0, per="beat", grid=grid)   # reading a time gives how many cycles have passed
    position(times)                                         # float64, the shape of `times`

Promises:
- `counter` returns a function from times (finite, from 0 up) to positions: `rate * t` for `per="second"`,
  `rate * grid.beat_at(t)` for `per="beat"`, so a source follows a tempo change. The result is always finite; its
  whole part says which cycle or step, its fraction where in it. It is negative for a time before a first beat that
  is not at 0.
- `per="beat"` without a grid is a `ParameterError` naming `per`, raised by `counter`; a grid is ignored per second.
- A time too far for the position to be a number (`rate * t` overflows, or the grid cannot reach it) is a `SignalError`.
"""

from collections.abc import Callable
from typing import Literal

import numpy as np
from numpy.typing import NDArray

from .errors import FormatError, ParameterError, SignalError
from .formats import Grid

__all__ = ["Positions", "counter"]

Positions = Callable[[NDArray[np.float64]], NDArray[np.float64]]


def counter(rate: float, per: Literal["second", "beat"], grid: Grid | None) -> Positions:
    """The function that turns times into positions, `rate` cycles or steps per second or per beat."""
    if per == "second":
        return lambda times: _counted(rate, times)
    if grid is None:
        raise ParameterError.at("per", "'beat' needs a grid")

    def per_beat(times: NDArray[np.float64]) -> NDArray[np.float64]:
        try:
            beats = grid.beat_at(times)
        except FormatError as error:  # a Grid reports a time out of its reach as a format error; a read is a signal's
            raise SignalError(str(error)) from None
        return _counted(rate, beats)

    return per_beat


def _counted(rate: float, amounts: NDArray[np.float64]) -> NDArray[np.float64]:
    with np.errstate(over="ignore"):  # an overflow is refused below, not warned about
        positions = rate * amounts
    if not np.isfinite(positions).all():
        raise SignalError("the position is not a finite number: the time is too far from 0")
    return positions
