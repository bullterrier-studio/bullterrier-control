"""Series: values at a fixed rate (docs/formats.md)."""

from typing import Annotated

from pydantic import Field, model_validator

from ..errors import FormatError
from .model import Finite, FormatModel
from .units import Unit

__all__ = ["Series"]


class Series(FormatModel):
    """Sample *i* is at `start + i / rate` seconds. A `fraction` series states its `scaling` and stays within 0..1."""

    start: Annotated[Finite, Field(ge=0)] = 0.0
    rate: Annotated[Finite, Field(gt=0)]
    values: Annotated[tuple[Finite, ...], Field(min_length=1)]
    unit: Unit
    scaling: str | None = None

    @model_validator(mode="after")
    def _fraction_is_scaled_and_in_range(self) -> "Series":
        if self.unit is not Unit.FRACTION:
            return self
        if not self.scaling:
            raise FormatError("scaling is required when unit is 'fraction': say how 0..1 was reached")
        for index, value in enumerate(self.values):
            if not 0.0 <= value <= 1.0:
                raise FormatError(f"values[{index}] = {value} is outside 0..1 (unit 'fraction')")
        return self
