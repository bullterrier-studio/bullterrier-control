"""How a source or shaper declares its parameters: a frozen model whose every field is described, ranged and defaulted.

    class WaveParams(Params):
        \"\"\"A repeating wave.\"\"\"

        depth: Finite = param(0.5, doc="How far the wave moves the value.", unit="fraction", min=0, max=1)

Promises:
- A field is a number (`float` or `int`), a `bool`, a `str` or a `Literal` choice of strings; nothing else. A model is
  checked when its class is made: every field has a default, a description and no alias; the defaults are valid; a
  number has a unit from `UNITS` and a finite range (min and max) that holds its default; any other field takes no
  unit or range; no field has a discouraged name (`DISCOURAGED`) or is named after a unit. A broken declaration
  raises `ValueError` naming the model and the field, so it never reaches a patch.
- Values are checked when a model is built (`parse`, or the constructor), strictly whatever the annotation: unknown
  fields, `true` or a string for a number, a float for a whole number, 1 for a switch and values out of range are
  refused with a `FormatError` naming each field, the rule and the value. A validator a subclass adds that changes
  values is the subclass's own business and is not checked.
- `fields_doc` describes the fields for docs, listings and tests: basic fields first, then the advanced ones
  (`advanced=True`: they tune the technique rather than choose the shape; settable and validated like the rest).
"""

import math
from typing import Any, Literal, NamedTuple, get_args, get_origin

from pydantic import ConfigDict, Field

from .errors import FormatError
from .formats.model import FormatModel

__all__ = ["DISCOURAGED", "UNITS", "FieldDoc", "Params", "param"]

# What a number may be measured in. `beats` is turned into seconds through a Grid by whatever reads the value.
# (A Series' `Unit` says what its values measure; this says what a parameter's number is in. They share names.)
UNITS = ("seconds", "beats", "hertz", "cycles", "fraction", "decibels", "ratio", "semitones", "count")

# Names a parameter must not have, with what to say instead: the same idea drifts into many names otherwise.
DISCOURAGED = {
    "strength": "depth or amount",
    "intensity": "depth or amount",
    "factor": "ratio, depth or amount",
    "val": "value",
    "k": "a name that says what it is",
}


def param(
    default: Any,
    *,
    doc: str,
    unit: str | None = None,
    min: float | None = None,  # noqa: A002  (the documented name of the range's lower bound)
    max: float | None = None,  # noqa: A002  (the documented name of the range's upper bound)
    advanced: bool = False,
    **field_kwargs: Any,
) -> Any:
    """Declare one field of a `Params` model.

    default: the value used when none is given; for a number it lies in the range. doc: what the field does, its unit
    and range in words, and what raising it does. unit: one of `UNITS`, required for a number, not allowed otherwise.
    min, max: the inclusive range, both required for a number, not allowed otherwise. advanced: the field tunes the
    technique rather than the shape; it is listed after the basic ones. Other keywords go to pydantic's `Field`.
    """
    extra: dict[str, Any] = {"unit": unit, "min": min, "max": max, "advanced": advanced}
    limits: dict[str, Any] = {key: value for key, value in (("ge", min), ("le", max)) if value is not None}
    return Field(default, description=doc, json_schema_extra=extra, **limits, **field_kwargs)


class FieldDoc(NamedTuple):
    """The documentation of one parameter, in parts (see `Params.fields_doc`)."""

    name: str
    unit: str | None
    minimum: float | None
    maximum: float | None
    default: Any
    doc: str
    advanced: bool


class Params(FormatModel):
    """The parameters of one source or shaper. Subclass it, with a docstring, and declare each field with `param`."""

    # strict: a plain float or int is as strict as Finite and Whole; validate_default: defaults obey the same rules
    model_config = ConfigDict(strict=True, validate_default=True)

    @classmethod
    def __pydantic_init_subclass__(cls, **kwargs: Any) -> None:
        """Check the declaration of every field as the class is made."""
        super().__pydantic_init_subclass__(**kwargs)
        for name, info in cls.model_fields.items():
            _check(f"{cls.__name__}.{name}", name, info, _extra(info.json_schema_extra))
        try:
            cls.parse({})
        except FormatError as error:
            raise ValueError(f"{cls.__name__}: a default is not valid: {error}") from None

    @classmethod
    def fields_doc(cls) -> tuple[FieldDoc, ...]:
        """Every field described, basic ones first, then the advanced ones, each group in declaration order."""
        docs = [
            FieldDoc(
                name, extra["unit"], extra["min"], extra["max"], info.default, info.description or "", extra["advanced"]
            )
            for name, info in cls.model_fields.items()
            for extra in [_extra(info.json_schema_extra)]
        ]
        return tuple(sorted(docs, key=lambda field: field.advanced))


def _extra(declared: object) -> dict[str, Any]:
    """What `param` recorded for a field; a field not declared with `param` has nothing recorded."""
    nothing: dict[str, Any] = {"unit": None, "min": None, "max": None, "advanced": False}
    return {**nothing, **declared} if isinstance(declared, dict) else nothing


def _check(where: str, name: str, info: Any, extra: dict[str, Any]) -> None:
    unit, low, high = extra["unit"], extra["min"], extra["max"]
    annotation = info.annotation
    if name in DISCOURAGED:
        raise ValueError(f"{where}: do not name a parameter {name!r}; use {DISCOURAGED[name]}")
    if name in UNITS:
        raise ValueError(f"{where}: do not name a parameter after its unit ({name!r}); say what it is")
    if info.alias is not None or info.validation_alias is not None or info.serialization_alias is not None:
        raise ValueError(f"{where}: a parameter has one name; remove the alias")
    if info.is_required():
        raise ValueError(f"{where}: needs a default (param(<default>, doc=...))")
    if not _supported(annotation):
        raise ValueError(
            f"{where}: this type is not supported (got {annotation!r}); a parameter is a number, a bool, a str or a"
            " Literal choice of strings"
        )
    if annotation in (int, float):
        if unit not in UNITS:
            raise ValueError(f"{where}: a number needs a unit, one of {', '.join(UNITS)} (got {unit!r})")
        if low is None or high is None:
            raise ValueError(f"{where}: a number needs both min and max (got min={low!r}, max={high!r})")
        if not (math.isfinite(low) and math.isfinite(high)):
            raise ValueError(f"{where}: min and max must be finite numbers (got min={low!r}, max={high!r})")
        default = info.default
        if not isinstance(default, int | float) or isinstance(default, bool) or not low <= default <= high:
            raise ValueError(f"{where}: the default {default!r} must lie in the range {low} to {high}")
    elif unit is not None or low is not None or high is not None:
        raise ValueError(f"{where}: a field that is not a number takes no unit, min or max")
    doc = info.description
    if not doc or not doc.strip():
        raise ValueError(f"{where}: needs a description (param(..., doc=...))")


def _supported(annotation: object) -> bool:
    """Whether a field of this type is one a parameter may be: a number, a bool, a str, or a Literal of strings."""
    if annotation in (int, float, bool, str):
        return True
    return get_origin(annotation) is Literal and all(isinstance(choice, str) for choice in get_args(annotation))
