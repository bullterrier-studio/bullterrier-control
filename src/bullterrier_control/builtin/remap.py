"""remap: put a 0 to 1 signal onto a range, through an ease: a level onto an opacity, a brightness or a pitch."""

from collections.abc import Callable

import numpy as np
from numpy.typing import ArrayLike, NDArray
from pydantic import field_validator

from ..plugin import Finite, Params, Signal, Text, between, curve, param, shaper


class RemapParams(Params):
    """The range the input is put onto, and the ease that bends the way there."""

    low: Finite = param(
        0.0,
        doc="The output when the input is 0; set above `high` to flip the signal.",
        unit="ratio",
        min=-1e6,
        max=1e6,
    )
    high: Finite = param(1.0, doc="The output when the input is 1.", unit="ratio", min=-1e6, max=1e6)
    ease: Text = param(
        "linear",
        doc=(
            "How the input is bent before it is put onto the range: one of the eases keyframes take (linear, hold, in,"
            " out, in-out, cubic-in, cubic-out, cubic-in-out, overshoot) or cubic-bezier(x1, y1, x2, y2). 'linear'"
            " keeps the shape."
        ),
    )

    @field_validator("ease")
    @classmethod
    def _is_an_ease(cls, name: str) -> str:
        curve(name)  # raises InvalidError listing the valid forms
        return name


class _Remapped(Signal):
    """The input bent by an ease and put onto a range."""

    def __init__(
        self, signal: Signal, low: float, high: float, ease: Callable[[ArrayLike], NDArray[np.float64]]
    ) -> None:
        self._signal, self._low, self._high, self._ease = signal, low, high, ease

    def compute(self, times: NDArray[np.float64]) -> NDArray[np.float64]:
        # the ease clips its progress to 0..1, so an input outside it is read as 0 or 1
        return between(self._low, self._high, self._ease(self._signal.compute(times)))


@shaper(params=RemapParams)
def remap(signal: Signal, params: RemapParams) -> Signal:
    """A signal that puts its input, taken as 0 to 1, onto the range from `low` to `high`, bent by an ease.

    An input outside 0 to 1 is clipped to it first: below 0 gives `low`, above 1 gives `high`. A `low` above `high`
    inverts. The output is exactly inside the range: an ease that overshoots (`overshoot`, or a Bezier with a y outside
    0 to 1) is clipped to `low` and `high`.
    """
    return _Remapped(signal, params.low, params.high, curve(params.ease))
