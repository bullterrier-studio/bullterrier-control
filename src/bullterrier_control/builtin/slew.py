"""slew: limit how fast a signal may rise and fall, so jumps become ramps."""

from functools import partial

from bullterrier_control._native import _core

from ..plugin import Finite, Params, Rendered, Signal, param, shaper


class SlewParams(Params):
    """How fast the output may climb and fall."""

    rise: Finite = param(
        0.1,
        doc="Seconds the output takes to climb by 1; larger is slower. 0 follows a rise at once.",
        unit="seconds",
        min=0,
        max=3600,
    )
    fall: Finite = param(
        0.1,
        doc="Seconds the output takes to fall by 1; larger is slower. 0 follows a fall at once.",
        unit="seconds",
        min=0,
        max=3600,
    )
    rate: Finite = param(
        1000.0,
        doc="Internal samples per second; the output is exact to one sample. Raising it is more exact and slower.",
        unit="hertz",
        min=10,
        max=48000,
        advanced=True,
    )


@shaper(params=SlewParams)
def slew(signal: Signal, params: SlewParams) -> Signal:
    """A signal that follows its input but climbs and falls no faster than `rise` and `fall` allow.

    It starts at the input's value at time 0. A limit is a time per unit of change: rise = 2 means at most 0.5 per
    second.
    """
    return Rendered(signal, rate=params.rate, run=partial(_core.render, "slew", params=params.model_dump()))
