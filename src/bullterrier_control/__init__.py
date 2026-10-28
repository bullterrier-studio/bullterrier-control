"""Control signals: numbers that change over time and drive the parameters of anything.

Keyframes, LFOs, envelopes and shaping, read at any moment in seconds. This package never touches pixels or sound; it
answers "what is this value at 12.4 s?". The public names are listed in `__all__`; the sources, shapers and operations
are in `lib` (`from bullterrier_control import lib; lib.slew(signal, rise=0.2)`).
"""

from importlib import metadata

from . import lib
from .errors import (
    BackendUnavailableError,
    BullterrierControlError,
    FormatError,
    InvalidError,
    ParameterError,
    SignalError,
    UnknownNameError,
)
from .formats import Bundle, Event, Grid, Series, Unit, events, load, save
from .inputs import Keyframe
from .patch import Patch, load_patch
from .signal import Signal, from_series, from_values, maximum, minimum

__version__ = metadata.version("bullterrier-control")

__all__ = [
    "BackendUnavailableError",
    "BullterrierControlError",
    "Bundle",
    "Event",
    "FormatError",
    "Grid",
    "InvalidError",
    "Keyframe",
    "ParameterError",
    "Patch",
    "Series",
    "Signal",
    "SignalError",
    "Unit",
    "UnknownNameError",
    "__version__",
    "events",
    "from_series",
    "from_values",
    "lib",
    "load",
    "load_patch",
    "maximum",
    "minimum",
    "save",
]
