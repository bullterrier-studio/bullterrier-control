"""clip: keep a signal between a lowest and a highest value."""

from ..plugin import Params, Signal, maximum, minimum, shaper


class ClipParams(Params, builtin="clip"):
    """The range a clipped signal stays inside."""


@shaper(params=ClipParams)
def clip(signal: Signal, params: ClipParams) -> Signal:
    """A signal that follows its input but never leaves the range from `low` to `high`."""
    return minimum(maximum(signal, params.low), params.high)
