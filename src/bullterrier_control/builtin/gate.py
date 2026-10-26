"""gate: turn a signal into on and off, with hysteresis: a light that comes on above one level and off below another."""

import numpy as np
from numpy.typing import NDArray
from pydantic import model_validator

from ..plugin import Finite, InvalidError, Params, Rendered, Signal, param, shaper


class GateParams(Params):
    """The two levels the gate opens and closes at."""

    open: Finite = param(
        0.5,
        doc="The level at which a closed gate opens: the input at or above it. Raising it asks for a stronger input.",
        unit="ratio",
        min=-1e6,
        max=1e6,
    )
    close: Finite = param(
        0.4,
        doc=(
            "The level an open gate closes below: the input under it. At most `open`; the gap between the two is the"
            " band in which the gate keeps its state, which stops a flicker. Equal to `open` is a plain threshold."
        ),
        unit="ratio",
        min=-1e6,
        max=1e6,
    )
    rate: Finite = param(
        1000.0,
        doc="Internal samples per second; the gate changes on one of them. Raising it is more exact and slower.",
        unit="hertz",
        min=10,
        max=48000,
        advanced=True,
    )

    @model_validator(mode="after")
    def _close_is_not_above_open(self) -> "GateParams":
        if self.close > self.open:
            raise InvalidError.at("close", f"{self.close} is above open = {self.open}")
        return self


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
