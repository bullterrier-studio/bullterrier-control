"""Control signals: numbers that change over time and drive the parameters of anything.

Keyframes, LFOs, envelopes and shaping, read at any moment in seconds. This package never touches pixels or sound; it
answers "what is this value at 12.4 s?". The public names are listed in `__all__`; the sources and shapers are in `lib`
(`from bullterrier_control import lib; lib.slew(signal, rise=0.2)`).
"""

from importlib import metadata

from . import lib
from .errors import (
    BullterrierControlError,
    FormatError,
    InvalidError,
    ParameterError,
    SignalError,
    UnknownNameError,
)
from .formats import Bundle, Event, Grid, Series, Unit, load, save
from .signal import Signal, from_series, maximum, minimum

__version__ = metadata.version("bullterrier-control")

__all__ = [
    "BullterrierControlError",
    "Bundle",
    "Event",
    "FormatError",
    "Grid",
    "InvalidError",
    "ParameterError",
    "Series",
    "Signal",
    "SignalError",
    "Unit",
    "UnknownNameError",
    "__version__",
    "from_series",
    "lib",
    "load",
    "maximum",
    "minimum",
    "save",
]
