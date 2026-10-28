"""The base of every validated model (the formats, a source's parameters): frozen, strict, one way in from outside.

    series = Series.parse({"rate": 100, "values": [0.1, 0.2], "unit": "ratio"})   # the model, or a FormatError

Promises:
- A model's fields cannot be rebound and unknown fields are refused. Mappings a model holds are read-only views.
- Numbers and switches are checked strictly (`Finite`, `Whole`, `Switch`): a whole number is fine where a decimal is
  expected; a `bool`, a numeric string and, for `Whole`, a float are not; `NaN` and the infinities never are; a
  `Switch` is `true` or `false`, not 1. Text (`Text`) must be writable as UTF-8. Sequences may be lists or tuples.
- Building a model, directly or with `parse`, is the one boundary: it returns the model or raises one error of the
  model's kind (`FormatError`, `ParameterError`). Field rules are all listed as `path: rule (got value)`; a rule across
  fields reports the first it finds.
"""

from collections.abc import Mapping, Sequence
from typing import Annotated, Any, ClassVar, Self

import numpy as np
from numpy.typing import NDArray
from pydantic import AfterValidator, BaseModel, BeforeValidator, ConfigDict, Field, Strict, ValidationError

from .errors import InvalidError

__all__ = ["Finite", "Switch", "Text", "ValidatedModel", "Whole", "plain", "whole_number"]

# a JSON number: not a bool, not a string, not NaN or an infinity
Finite = Annotated[float, Strict(), Field(allow_inf_nan=False)]


def whole_number(value: object) -> object:
    """A `BeforeValidator` for a JSON integer: `int` validation alone accepts `True` and `1.0`, the format does not."""
    if type(value) is not int:
        raise InvalidError(f"must be a whole number (got {value!r})")
    return value


def plain[T](values: Sequence[T] | NDArray[np.float64]) -> Sequence[T] | list[float]:
    """The values as Python numbers for a model to validate: an array is listed, so a numpy bool array is refused
    instead of being read as 0 and 1. Any other sequence is returned as it is."""
    return values.tolist() if isinstance(values, np.ndarray) else values


def _utf8_text(text: str) -> str:
    try:
        text.encode("utf-8")
    except UnicodeEncodeError:
        raise InvalidError("text must be writable as UTF-8 (it holds a lone surrogate)") from None
    return text


# a JSON true or false: not 1, not "true"
Switch = Annotated[bool, Strict()]
# a JSON integer: not a bool, not a float
Whole = Annotated[int, BeforeValidator(whole_number)]
# free text that can be saved as UTF-8
Text = Annotated[str, AfterValidator(_utf8_text)]

_SHOWN = 60  # characters of an offending value a message shows


def _path(location: tuple[int | str, ...]) -> str:
    return "".join(f"[{part}]" if isinstance(part, int) else f".{part}" for part in location).removeprefix(".")


def _join(base: str, path: str) -> str:
    if not base or not path:
        return base or path
    return base + path if path.startswith("[") else f"{base}.{path}"


def _problems(error: ValidationError) -> tuple[tuple[str, str], ...]:
    """Each broken rule as (path, rule). A rule a nested model broke arrives already described by that model's own
    InvalidError (pydantic calls its `__init__`), so its paths are placed under the nested field's."""
    found: list[tuple[str, str]] = []
    for problem in error.errors():
        base = _path(problem["loc"])
        inner = problem.get("ctx", {}).get("error")
        if isinstance(inner, InvalidError) and inner.problems:
            found.extend((_join(base, path), rule) for path, rule in inner.problems)
            continue
        shown = repr(problem["input"])
        if len(shown) > _SHOWN:
            shown = shown[:_SHOWN] + "..."
        message = str(problem["msg"]).removeprefix("Value error, ")
        found.append((base, message if isinstance(inner, InvalidError) else f"{message} (got {shown})"))
    return tuple(found)


class ValidatedModel(BaseModel):
    """A validated model. Subclasses declare fields with `Finite`, `Whole`, `Text` and `Field` constraints."""

    model_config = ConfigDict(extra="forbid", frozen=True, allow_inf_nan=False)
    error: ClassVar[type[InvalidError]] = InvalidError  # the kind of error this model raises (a subclass sets its own)

    def __init__(self, **data: Any) -> None:
        """Build the model from field values, or raise its `error` listing what breaks a rule."""
        try:
            super().__init__(**data)
        except ValidationError as error:
            problems = _problems(error)
            listed = "; ".join(f"{path}: {rule}" if path else rule for path, rule in problems)
            raise self.error(f"{type(self).__name__}: {listed}", problems) from None

    @classmethod
    def parse(cls, data: Mapping[str, object]) -> Self:
        """The model for `data` (a decoded JSON object), or its `error` listing every field that breaks a rule."""
        return cls(**data)
