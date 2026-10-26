"""The exceptions bullterrier-control raises: catch `BullterrierControlError` for every failure the library reports on
purpose.

Library code raises a subclass of it, never a bare builtin, for anything a caller or an input file can get wrong; the
message says what was wrong and what is valid. A subclass also inherits the builtin it replaces (`ValueError`,
`KeyError`, ...), so code that already catches that builtin keeps working.

Dependency direction: the bottom layer. It imports nothing from bullterrier_control, so every layer may raise them.
"""

__all__ = ["BullterrierControlError", "FormatError"]


class BullterrierControlError(Exception):
    """The base of every exception bullterrier-control raises on purpose."""


class FormatError(BullterrierControlError, ValueError):
    """Data does not follow a format of docs/formats.md: the message names each field path, the rule and the value."""
