"""Event: a moment, or a span when it has a duration (docs/formats.md)."""

from typing import Annotated

from pydantic import Field

from ..validated import Finite, Text
from .model import FormatModel

__all__ = ["Event"]

_HIGHEST_MIDI_NOTE = 127  # MIDI note numbers run 0..127; a fraction is a pitch between two notes


class Event(FormatModel):
    """Something at `t` seconds: an onset, a beat, a note (with `duration` and `pitch`), or a section start."""

    t: Annotated[Finite, Field(ge=0)]
    strength: Annotated[Finite, Field(ge=0, le=1)] = 1.0
    duration: Annotated[Finite, Field(gt=0)] | None = None
    label: Annotated[Text, Field(min_length=1)] | None = None
    pitch: Annotated[Finite, Field(ge=0, le=_HIGHEST_MIDI_NOTE)] | None = None
