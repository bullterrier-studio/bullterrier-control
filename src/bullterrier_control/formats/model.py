"""The base of every format: frozen, strict about types, and one way in from outside data.

    series = Series.parse({"rate": 100, "values": [0.1, 0.2], "unit": "ratio"})   # the model, or a FormatError

Promises:
- A model's fields cannot be rebound and unknown fields are refused. Mappings a model holds are read-only views.
- Numbers and switches are checked strictly (`Finite`, `Whole`, `Switch`): a whole number is fine where a decimal is
  expected; a `bool`, a numeric string and, for `Whole`, a float are not; `NaN` and the infinities never are; a
  `Switch` is `true` or `false`, not 1. Text (`Text`) must be writable as UTF-8. Sequences may be lists or tuples.
- Building a model, directly or with `parse`, is the one boundary: it returns the model or raises one `FormatError`.
  Field rules are all listed as `path: rule (got value)`; a rule across fields reports the first it finds.
"""

from collections.abc import Mapping
from typing import Annotated, Any, Self

from pydantic import AfterValidator, BaseModel, BeforeValidator, ConfigDict, Field, Strict, ValidationError

from ..errors import FormatError

__all__ = ["Finite", "FormatModel", "Switch", "Text", "Whole", "whole_number"]

# a JSON number: not a bool, not a string, not NaN or an infinity
Finite = Annotated[float, Strict(), Field(allow_inf_nan=False)]


def whole_number(value: object) -> object:
    """A `BeforeValidator` for a JSON integer: `int` validation alone accepts `True` and `1.0`, the format does not."""
    if type(value) is not int:
        raise FormatError(f"must be a whole number (got {value!r})")
    return value


def _utf8_text(text: str) -> str:
    try:
        text.encode("utf-8")
    except UnicodeEncodeError:
        raise FormatError("text must be writable as UTF-8 (it holds a lone surrogate)") from None
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
    FormatError (pydantic calls its `__init__`), so its paths are placed under the nested field's."""
    found: list[tuple[str, str]] = []
    for problem in error.errors():
        base = _path(problem["loc"])
        inner = problem.get("ctx", {}).get("error")
        if isinstance(inner, FormatError) and inner.problems:
            found.extend((_join(base, path), rule) for path, rule in inner.problems)
            continue
        shown = repr(problem["input"])
        if len(shown) > _SHOWN:
            shown = shown[:_SHOWN] + "..."
        message = str(problem["msg"]).removeprefix("Value error, ")
        found.append((base, message if isinstance(inner, FormatError) else f"{message} (got {shown})"))
    return tuple(found)


class FormatModel(BaseModel):
    """A format of docs/formats.md. Subclasses declare fields with `Finite`, `Whole`, `Text` and `Field` constraints."""

    model_config = ConfigDict(extra="forbid", frozen=True, allow_inf_nan=False)

    def __init__(self, **data: Any) -> None:
        """Build the model from field values, or raise a `FormatError` listing what breaks a rule."""
        try:
            super().__init__(**data)
        except ValidationError as error:
            problems = _problems(error)
            listed = "; ".join(f"{path}: {rule}" if path else rule for path, rule in problems)
            raise FormatError(f"{type(self).__name__}: {listed}", problems) from None

    @classmethod
    def parse(cls, data: Mapping[str, object]) -> Self:
        """The model for `data` (a decoded JSON object), or a `FormatError` listing every field that breaks a rule."""
        return cls(**data)
