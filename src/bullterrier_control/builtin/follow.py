"""follow: smooth a signal with separate attack and release times: a level that swells on a hit and eases back."""

from functools import partial

from bullterrier_control._native import _core

from ..plugin import Finite, Params, Rendered, Signal, param, shaper


class FollowParams(Params):
    """How quickly the output chases its input upward and downward."""

    attack: Finite = param(
        0.01,
        doc=(
            "Seconds the output takes to close about two thirds (1 - 1/e) of the gap to a higher input; larger is"
            " slower. 0 follows a rise at once."
        ),
        unit="seconds",
        min=0,
        max=3600,
    )
    release: Finite = param(
        0.25,
        doc=(
            "Seconds the output takes to close about two thirds (1 - 1/e) of the gap to a lower input; larger is"
            " slower. 0 follows a fall at once."
        ),
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
    return Rendered(signal, rate=params.rate, run=partial(_core.render, "follow", params=params.model_dump()))
