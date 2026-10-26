"""The exceptions bullterrier-control raises: catch `BullterrierControlError` for every failure the library reports on
purpose.

Library code raises a subclass of it, never a bare builtin, for anything a caller or an input file can get wrong; the
message says what was wrong and what is valid. A subclass also inherits the builtin it replaces (`ValueError`,
`KeyError`, ...), so code that already catches that builtin keeps working.

Dependency direction: the bottom layer. It imports nothing from bullterrier_control, so every layer may raise them.
"""

__all__ = ["BullterrierControlError", "FormatError", "SignalError"]


class BullterrierControlError(Exception):
    """The base of every exception bullterrier-control raises on purpose."""


class FormatError(BullterrierControlError, ValueError):
    """Data breaks the rules a model declares (a format of docs/formats.md, a source's parameters): the message names
    each field path, the rule and the value.

    `problems` holds the same as (path, rule) pairs when the error came from a format model, so a model that holds
    another can report the inner paths under its own; it is empty for an error that has only a message.
    """

    def __init__(self, message: str, problems: tuple[tuple[str, str], ...] = ()) -> None:
        """An error with `message`, and optionally the (path, rule) pairs it lists."""
        super().__init__(message)
        self.problems = problems

    @classmethod
    def at(cls, path: str, rule: str) -> "FormatError":
        """The error for one broken `rule` at the field `path` (`values[2]`, `series.bass.start`)."""
        return cls(f"{path}: {rule}", ((path, rule),))


class SignalError(BullterrierControlError, ValueError):
    """A signal cannot be read as asked: a time that is not a finite number, or a read too large to render."""
