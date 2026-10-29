"""Event: a moment, or a span when it has a duration (docs/formats.md)."""

from collections.abc import Sequence
from typing import Annotated

import numpy as np
from numpy.typing import NDArray
from pydantic import Field

from ..errors import FormatError
from ..validated import Finite, Text, plain
from .model import FormatModel

__all__ = ["Event", "events"]

_HIGHEST_MIDI_NOTE = 127  # MIDI note numbers run 0..127; a fraction is a pitch between two notes


class Event(FormatModel):
    """Something at `t` seconds: an onset, a beat, a note (with `duration` and `pitch`), or a section start."""

    t: Annotated[Finite, Field(ge=0)]
    strength: Annotated[Finite, Field(ge=0, le=1)] = 1.0
    duration: Annotated[Finite, Field(gt=0)] | None = None
    label: Annotated[Text, Field(min_length=1)] | None = None
    pitch: Annotated[Finite, Field(ge=0, le=_HIGHEST_MIDI_NOTE)] | None = None


def events(
    times: Sequence[float] | NDArray[np.float64],
    *,
    strength: float | Sequence[float] | NDArray[np.float64] = 1.0,
    label: str | None = None,
    duration: float | None = None,
) -> tuple[Event, ...]:
    """One `Event` per time, in the order given. `strength` is one number for all or one per time; `label` and
    `duration` apply to all. A `FormatError` names the field that breaks an `Event` rule and the event's place in
    the list (`events[1].t`), or `strength` when its count differs from the times'."""
    moments = plain(times)
    strengths = plain(strength) if isinstance(strength, Sequence | np.ndarray) else [strength] * len(moments)
    if len(strengths) != len(moments):
        raise FormatError.at(
            "strength", f"needs one value per time or a single number (got {len(strengths)} for {len(moments)} times)"
        )
    made = []
    for index, (t, each) in enumerate(zip(moments, strengths, strict=True)):
        try:
            made.append(Event(t=t, strength=each, label=label, duration=duration))
        except FormatError as error:  # the same problems, under the event's place in the list
            problems = tuple((f"events[{index}].{path}", rule) for path, rule in error.problems)
            raise FormatError("; ".join(f"{path}: {rule}" for path, rule in problems), problems) from None
    return tuple(made)
