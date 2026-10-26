"""clip: keep a signal between a lowest and a highest value."""

from pydantic import model_validator

from ..plugin import Finite, InvalidError, Params, Signal, maximum, minimum, param, shaper


class ClipParams(Params):
    """The range a clipped signal stays inside."""

    low: Finite = param(
        0.0, doc="The lowest value the output takes; lower values are raised to it.", unit="ratio", min=-1e6, max=1e6
    )
    high: Finite = param(
        1.0, doc="The highest value the output takes; higher values are lowered to it.", unit="ratio", min=-1e6, max=1e6
    )

    @model_validator(mode="after")
    def _high_is_not_below_low(self) -> "ClipParams":
        if self.high < self.low:
            raise InvalidError.at("high", f"{self.high} is below low = {self.low}")
        return self


@shaper(params=ClipParams)
def clip(signal: Signal, params: ClipParams) -> Signal:
    """A signal that follows its input but never leaves the range from `low` to `high`."""
    return minimum(maximum(signal, params.low), params.high)
