"""The base of every format of docs/formats.md: a validated model whose errors are `FormatError`."""

from typing import ClassVar

from ..errors import FormatError, InvalidError
from ..validated import ValidatedModel

__all__ = ["FormatModel"]


class FormatModel(ValidatedModel):
    """A format of docs/formats.md: frozen, strict, refusing unknown fields; a broken rule is a `FormatError`."""

    error: ClassVar[type[InvalidError]] = FormatError
