"""The base of every format of docs/formats.md: a validated model whose errors are `FormatError`."""

from pathlib import Path
from typing import ClassVar

from ..errors import FormatError, InvalidError
from ..validated import ValidatedModel

__all__ = ["FormatModel", "read_text"]


class FormatModel(ValidatedModel):
    """A format of docs/formats.md: frozen, strict, refusing unknown fields; a broken rule is a `FormatError`."""

    error: ClassVar[type[InvalidError]] = FormatError


def read_text(path: Path) -> str:
    """The UTF-8 text of the file at `path`; a `FormatError` starting with the path if it is not UTF-8, the `OSError`
    the system gave if it cannot be read at all."""
    try:
        return path.read_text(encoding="utf-8")
    except UnicodeDecodeError as error:
        raise FormatError(f"{path}: not UTF-8 text ({error.reason})") from None
