"""Grid: the musical clock, stored as the time of every beat (docs/formats.md)."""

from typing import Annotated

from pydantic import Field, Strict, model_validator

from ..errors import FormatError
from .model import Finite, FormatModel

__all__ = ["Grid"]

Whole = Annotated[int, Strict()]  # a JSON integer: not a float, not a bool


class Grid(FormatModel):
    """Beat times in seconds, strictly increasing. Tempo is derived from them, never stored beside them."""

    beats: Annotated[tuple[Annotated[Finite, Field(ge=0)], ...], Field(min_length=2)]
    meter: Annotated[Whole, Field(ge=1)]
    first_downbeat: Annotated[Whole, Field(ge=0)]

    @model_validator(mode="after")
    def _beats_increase_and_the_downbeat_is_in_the_bar(self) -> "Grid":
        for index in range(1, len(self.beats)):
            if self.beats[index] <= self.beats[index - 1]:
                now, before = self.beats[index], self.beats[index - 1]
                raise FormatError(f"beats[{index}] = {now} does not come after beats[{index - 1}] = {before}")
        if self.first_downbeat >= self.meter:
            raise FormatError(f"first_downbeat = {self.first_downbeat} is not inside a bar of meter = {self.meter}")
        return self
