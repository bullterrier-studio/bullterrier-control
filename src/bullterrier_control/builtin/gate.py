"""gate: turn a signal into on and off, with hysteresis: a light that comes on above one level and off below another."""

import numpy as np
from numpy.typing import NDArray

from ..plugin import Params, Rendered, Signal, shaper


class GateParams(Params, builtin="gate"):
    """The two levels the gate opens and closes at."""


@shaper(params=GateParams)
def gate(signal: Signal, params: GateParams) -> Signal:
    """A signal that is 1 while its input is high and 0 while it is low, with a gap between the levels that opens and
    closes it (hysteresis).

    A closed gate opens when the input reaches `open` (at or above it); an open one closes when the input falls below
    `close`. Between the two it keeps its state. At time 0 it is open if the input is at or above `open`. The change
    happens in the internal sample where the input crosses, so a jump exactly on a sample is seen at that time.
    """
    opening, closing = params.open, params.close

    def run(inputs: NDArray[np.float64]) -> NDArray[np.float64]:
        outputs = np.empty_like(inputs)
        is_open = False  # closed to begin with: the first sample opens it if the input is at or above `open`
        for index, level in enumerate(inputs.tolist()):
            # a closed gate needs the input at `open`; an open one stays unless it is below `close`
            is_open = level >= closing if is_open else level >= opening
            outputs[index] = is_open
        return outputs

    return Rendered(signal, rate=params.rate, run=run)
