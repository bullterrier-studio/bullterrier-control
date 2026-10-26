"""The units a Series may carry (docs/formats.md)."""

from enum import StrEnum

__all__ = ["Unit"]


class Unit(StrEnum):
    """What a Series' values measure. `FRACTION` values lie in 0..1 and the Series says how that was reached."""

    FRACTION = "fraction"
    DECIBELS = "decibels"
    HERTZ = "hertz"
    MIDI = "midi"
    RATIO = "ratio"
    SECONDS = "seconds"
    BEATS = "beats"
