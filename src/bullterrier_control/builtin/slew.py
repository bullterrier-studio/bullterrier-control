"""slew: limit how fast a signal may rise and fall, so jumps become ramps."""

from typing import cast

from bullterrier_control._native import _core

from ..plugin import Params, Signal, shaper


class SlewParams(Params, builtin="slew"):
    """How fast the output may climb and fall."""


@shaper(params=SlewParams)
def slew(signal: Signal, params: SlewParams) -> Signal:
    """A signal that follows its input but climbs and falls no faster than `rise` and `fall` allow.

    It starts at the input's value at time 0. A limit is a time per unit of change: rise = 2 means at most 0.5 per
    second.
    """
    return cast("Signal", _core.shape("slew", signal, params.to_dict()))
