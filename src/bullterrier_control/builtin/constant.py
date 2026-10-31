"""constant: the same value at every time."""

from ..plugin import Constant, Params, Signal, source


class ConstantParams(Params, builtin="constant"):
    """The value a constant signal holds."""


@source(params=ConstantParams)
def constant(params: ConstantParams) -> Signal:
    """A signal that holds one value at every time."""
    return Constant(params.value)
