"""Grid: the musical clock, stored as the time of every beat (docs/formats.md), and its conversions.

grid.seconds_at(2.5)      # the time of beat position 2.5 (halfway between the third and fourth beat)
grid.beat_at(12.4)        # the beat position at 12.4 s
"""

from typing import Annotated

import numpy as np
from numpy.typing import ArrayLike, NDArray
from pydantic import Field, model_validator

from ..errors import FormatError
from .model import Finite, FormatModel, Whole

__all__ = ["Grid"]


class Grid(FormatModel):
    """Beat times in seconds, strictly increasing. Tempo is derived from them, never stored beside them."""

    beats: Annotated[tuple[Annotated[Finite, Field(ge=0)], ...], Field(min_length=2)]
    meter: Annotated[Whole, Field(ge=1)]
    first_downbeat: Annotated[Whole, Field(ge=0)]

    @model_validator(mode="after")
    def _beats_increase_and_the_downbeat_is_a_beat_in_the_bar(self) -> "Grid":
        for index in range(1, len(self.beats)):
            if self.beats[index] <= self.beats[index - 1]:
                now, before = self.beats[index], self.beats[index - 1]
                raise FormatError.at(f"beats[{index}]", f"{now} does not come after beats[{index - 1}] = {before}")
        if self.first_downbeat >= min(self.meter, len(self.beats)):
            raise FormatError.at(
                "first_downbeat",
                f"{self.first_downbeat} must be a beat inside a bar of meter = {self.meter} ({len(self.beats)} beats)",
            )
        return self

    @property
    def downbeats(self) -> tuple[float, ...]:
        """The time of the first beat of every bar: `beats[first_downbeat::meter]`."""
        return self.beats[self.first_downbeat :: self.meter]

    def seconds_at(self, beats: ArrayLike) -> NDArray[np.float64]:
        """The time in seconds of each beat position in `beats` (a number or an array; the result has its shape).

        Position 0 is the first beat, 1 the second, and so on; a fraction is linear between two beats. Before the first
        beat and after the last, the first or last beat length continues, so every position has a time.
        """
        position = _finite_numbers(beats)
        times = np.asarray(self.beats, dtype=np.float64)
        last = len(times) - 1
        with np.errstate(all="ignore"):  # an overflow is refused below, not warned about
            inside = np.interp(position, np.arange(last + 1), times)
            before = times[0] + position * (times[1] - times[0])
            after = times[-1] + (position - last) * (times[-1] - times[-2])
            return _finite_result(np.where(position < 0, before, np.where(position > last, after, inside)))

    def beat_at(self, seconds: ArrayLike) -> NDArray[np.float64]:
        """The beat position of each time in `seconds`: the inverse of `seconds_at`, with the same shape."""
        moment = _finite_numbers(seconds)
        times = np.asarray(self.beats, dtype=np.float64)
        last = len(times) - 1
        with np.errstate(all="ignore"):
            inside = np.interp(moment, times, np.arange(last + 1))
            before = (moment - times[0]) / (times[1] - times[0])
            after = last + (moment - times[-1]) / (times[-1] - times[-2])
            return _finite_result(np.where(moment < times[0], before, np.where(moment > times[-1], after, inside)))


def _finite_numbers(values: ArrayLike) -> NDArray[np.float64]:
    raw = np.asarray(values)
    if raw.dtype.kind not in "iuf" or not np.isfinite(raw.astype(np.float64)).all():
        raise FormatError(f"a position or time must be a finite number (got {values!r})")
    return raw.astype(np.float64)


def _finite_result(values: NDArray[np.float64]) -> NDArray[np.float64]:
    if not np.isfinite(values).all():
        raise FormatError("the result is not a finite number: the position or time is too far from the beats")
    return np.asarray(values)
