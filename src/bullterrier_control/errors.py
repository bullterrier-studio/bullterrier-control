"""The exceptions bullterrier-control raises: catch `BullterrierControlError` for every failure the library reports on
purpose.

Library code raises a subclass of it, never a bare builtin, for anything a caller or an input file can get wrong; the
message says what was wrong and what is valid. A subclass also inherits the builtin it replaces (`ValueError`,
`KeyError`, ...), so code that already catches that builtin keeps working.

Dependency direction: the bottom layer. It imports nothing from bullterrier_control, so every layer may raise them.
"""

__all__ = [
    "BullterrierControlError",
    "FormatError",
    "InvalidError",
    "ParameterError",
    "SignalError",
    "UnknownNameError",
]


class BullterrierControlError(Exception):
    """The base of every exception bullterrier-control raises on purpose."""


class InvalidError(BullterrierControlError, ValueError):
    """Values break the rules a model declares: the message names each field path, the rule and the value. Catch it for
    either kind below; raise `InvalidError.at(path, rule)` from a validator, and the model reports it as its own kind.

    `problems` holds the same as (path, rule) pairs when the error came from a model, so a model that holds another can
    report the inner paths under its own; it is empty for an error that has only a message.
    """

    def __init__(self, message: str, problems: tuple[tuple[str, str], ...] = ()) -> None:
        """An error with `message`, and optionally the (path, rule) pairs it lists."""
        super().__init__(message)
        self.problems = problems

    @classmethod
    def at(cls, path: str, rule: str) -> "InvalidError":
        """The error for one broken `rule` at the field `path` (`values[2]`, `series.bass.start`)."""
        return cls(f"{path}: {rule}", ((path, rule),))


class FormatError(InvalidError):
    """Data does not follow a format of docs/formats.md: a file, a Series, an Event, a Grid, a Bundle."""


class ParameterError(InvalidError):
    """The parameter values given to a source or shaper break the rules its `Params` model declares."""


class SignalError(BullterrierControlError, ValueError):
    """A signal cannot be read as asked: a time that is not a finite number, or a read too large to render."""


class UnknownNameError(BullterrierControlError, KeyError):
    """No source or shaper is registered under that name; the message lists the names that are."""

    def __str__(self) -> str:
        """The message as written (a `KeyError` would show it in quotes)."""
        return str(self.args[0])
