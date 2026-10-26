"""How a source or shaper declares its parameters: a frozen model whose every field is described, ranged and defaulted.

    class WaveParams(Params):
        \"\"\"A repeating wave.\"\"\"

        depth: Finite = param(0.5, doc="How far the wave moves the value.", unit="fraction", min=0, max=1)

Promises:
- A model is checked when its class is made: every field has a description; a number has a unit from `UNITS` and a range
  (min and max) that holds its default; any other field (a `Switch`, a `Literal` choice) takes no unit or range; no
  field has a discouraged name (`DISCOURAGED`). A broken declaration raises `ValueError` naming the model and the
  field, so it never reaches a patch.
- Values are checked when a model is built (`parse`, or the constructor), strictly, as every format is: unknown fields,
  `true` or a string for a number, a float for a whole number and values out of range are refused with a `FormatError`
  naming each field, the rule and the value.
- `fields_doc` describes the fields for docs, listings and tests: basic fields first, then the advanced ones
  (`advanced=True`: they tune the technique rather than choose the shape; settable and validated like the rest).
"""

from typing import Any, NamedTuple

from pydantic import Field

from .formats.model import FormatModel

__all__ = ["DISCOURAGED", "UNITS", "FieldDoc", "Params", "param"]

# What a number may be measured in. `beats` is turned into seconds through a Grid by whatever reads the value.
UNITS = ("seconds", "beats", "hz", "cycles", "fraction", "decibels", "ratio", "semitones", "count")

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
) -> Any:
    """Declare one field of a `Params` model.

    default: the value used when none is given; for a number it lies in the range. doc: what the field does, its unit
    and range in words, and what raising it does. unit: one of `UNITS`, required for a number, not allowed otherwise.
    min, max: the inclusive range, both required for a number, not allowed otherwise. advanced: the field tunes the
    technique rather than the shape; it is listed after the basic ones.
    """
    extra: dict[str, Any] = {"unit": unit, "min": min, "max": max, "advanced": advanced}
    limits: dict[str, Any] = {key: value for key, value in (("ge", min), ("le", max)) if value is not None}
    return Field(default, description=doc, json_schema_extra=extra, **limits)


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

    @classmethod
    def __pydantic_init_subclass__(cls, **kwargs: Any) -> None:
        """Check the declaration of every field as the class is made."""
        super().__pydantic_init_subclass__(**kwargs)
        for name, info in cls.model_fields.items():
            _check(
                f"{cls.__name__}.{name}",
                name,
                info.annotation,
                info.default,
                info.description,
                _extra(info.json_schema_extra),
            )

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


def _check(where: str, name: str, annotation: object, default: object, doc: str | None, extra: dict[str, Any]) -> None:
    unit, low, high = extra["unit"], extra["min"], extra["max"]
    if name in DISCOURAGED:
        raise ValueError(f"{where}: do not name a parameter {name!r}; use {DISCOURAGED[name]}")
    if annotation in (int, float):
        if unit not in UNITS:
            raise ValueError(f"{where}: a number needs a unit, one of {', '.join(UNITS)} (got {unit!r})")
        if low is None or high is None:
            raise ValueError(f"{where}: a number needs both min and max (got min={low!r}, max={high!r})")
        if not isinstance(default, int | float) or isinstance(default, bool) or not low <= default <= high:
            raise ValueError(f"{where}: the default {default!r} must lie in the range {low} to {high}")
    elif unit is not None or low is not None or high is not None:
        raise ValueError(f"{where}: a field that is not a number takes no unit, min or max")
    if not doc or not doc.strip():
        raise ValueError(f"{where}: needs a description (param(..., doc=...))")
