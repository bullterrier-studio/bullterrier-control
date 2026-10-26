"""constant: the same value at every time."""

from ..plugin import Constant, Finite, Params, Signal, param, source


class ConstantParams(Params):
    """The value a constant signal holds."""

    value: Finite = param(
        0.0, doc="The value the signal holds at every time; negative values are fine.", unit="ratio", min=-1e6, max=1e6
    )


@source(params=ConstantParams)
def constant(params: ConstantParams) -> Signal:
    """A signal that holds one value at every time."""
    return Constant(params.value)
