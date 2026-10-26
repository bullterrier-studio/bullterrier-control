"""The base of every format: frozen, strict about types, and one way in from outside data.

    series = Series.parse({"rate": 100, "values": [0.1, 0.2], "unit": "ratio"})   # the model, or a FormatError

Promises:
- A model is immutable and refuses unknown fields.
- Numbers are checked strictly (`Finite`): a whole number is fine where a decimal is expected, a `bool` or a numeric
  string is not, and `NaN` and the infinities never are. Sequences may be lists or tuples.
- `parse` is the one boundary: a mapping in, the model out, or one `FormatError` that lists every problem as
  `path: rule (got value)`.
"""

from collections.abc import Mapping
from typing import Annotated, Self

from pydantic import BaseModel, ConfigDict, Field, Strict, ValidationError

from ..errors import FormatError

__all__ = ["Finite", "FormatModel"]

# a JSON number: not a bool, not a string, not NaN or an infinity
Finite = Annotated[float, Strict(), Field(allow_inf_nan=False)]

_SHOWN = 60  # characters of an offending value a message shows


def _describe(error: Mapping[str, object]) -> str:
    path = ".".join(str(part) for part in error["loc"])  # type: ignore[attr-defined]  # pydantic's error dict
    shown = repr(error["input"])
    if len(shown) > _SHOWN:
        shown = shown[:_SHOWN] + "..."
    message = str(error["msg"]).removeprefix("Value error, ")
    where = f"{path}: " if path else ""
    return f"{where}{message} (got {shown})" if path else message


class FormatModel(BaseModel):
    """A format of docs/formats.md. Subclasses declare fields with `Finite` and `Field` constraints."""

    model_config = ConfigDict(extra="forbid", frozen=True, allow_inf_nan=False)

    @classmethod
    def parse(cls, data: Mapping[str, object]) -> Self:
        """The model for `data`, or a `FormatError` listing every field that breaks a rule."""
        try:
            return cls.model_validate(data)
        except ValidationError as error:
            problems = "; ".join(_describe(problem) for problem in error.errors())
            raise FormatError(f"{cls.__name__}: {problems}") from None
