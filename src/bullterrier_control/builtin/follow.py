"""follow: smooth a signal with separate attack and release times: a level that swells on a hit and eases back."""

from functools import partial

from bullterrier_control._native import _core

from ..plugin import Params, Rendered, Signal, shaper


class FollowParams(Params, builtin="follow"):
    """How quickly the output chases its input upward and downward."""


@shaper(params=FollowParams)
def follow(signal: Signal, params: FollowParams) -> Signal:
    """A signal that chases its input like a capacitor: it closes a share of the gap to it with every step, at its
    own speed up (`attack`) and down (`release`).

    It starts at the input's value at time 0. The gap shrinks by e^(-1 / (rate * time)) per internal sample, `time`
    being `attack` while the input is above the output and `release` while it is below, so a jump is followed by a
    curve that is steep at first and flattens out, and never quite arrives. The same value for both is plain smoothing.
    `slew` differs: it moves at a fixed speed, however far the input is, and arrives. The output stays between the
    lowest and highest value the input has had, to within float rounding.
    """
    return Rendered(signal, rate=params.rate, run=partial(_core.render, "follow", params=params.to_dict()))
