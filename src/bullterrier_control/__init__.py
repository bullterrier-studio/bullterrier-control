"""Control signals: numbers that change over time and drive the parameters of anything.

Keyframes, LFOs, envelopes and shaping, read at any moment in seconds. This package never touches pixels or sound; it
answers "what is this value at 12.4 s?". The public names are listed in `__all__`.
"""

from importlib import metadata

from .errors import BullterrierControlError

__version__ = metadata.version("bullterrier-control")

__all__ = ["BullterrierControlError", "__version__"]
