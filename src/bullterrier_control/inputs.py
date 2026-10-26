"""The data a source takes besides its parameters, and the one place that says what each kind is and how it is checked.

    @source(params=TrackParams)
    def track(params: TrackParams, *, points: tuple[Keyframe, ...]) -> Signal: ...

    track(points=[{"t": 0, "value": 0}, {"t": 2, "value": 1, "ease": "in-out"}])

A parameter is a number or a choice; an input is a structure (keyframes, later events, a grid). A function declares the
inputs it takes as keyword-only arguments named in `INPUTS`; the registry knows no kind by name: it looks the argument
up here and calls `check`. Adding a kind is one entry in `INPUTS`.

Promises:
- `INPUTS[name].check(value)` returns the value in its checked form, or raises `ParameterError` listing each broken rule
  as `path: rule` with the path from the input's name (`points[1].ease`). Mappings are parsed into their model.
- `points`: a non-empty list or tuple of `Keyframe` or mappings; `t` strictly increases. Checked, it is a
  `tuple[Keyframe, ...]`.
"""

from collections.abc import Callable, Mapping
from itertools import pairwise
from types import MappingProxyType
from typing import Annotated, ClassVar, NamedTuple

from pydantic import AfterValidator, BeforeValidator, Field, model_validator

from .easing import EASES, curve
from .errors import InvalidError, ParameterError
from .validated import Finite, Text, ValidatedModel

__all__ = ["INPUTS", "Input", "Keyframe"]


def _known_ease(name: str) -> str:
    curve(name)  # raises InvalidError listing the valid forms
    return name


class Keyframe(ValidatedModel):
    """One point of a keyframed signal: the value at a time in seconds, and the ease of the segment that starts here
    (ignored on the last point)."""

    error: ClassVar[type[InvalidError]] = ParameterError
    t: Annotated[Finite, Field(ge=0)]
    value: Finite
    ease: Annotated[Text, AfterValidator(_known_ease)] = "linear"


def _list_or_tuple(value: object) -> object:
    # pydantic would also take a set, a generator or (iterating its fields) a single model: the order must be given
    if not isinstance(value, list | tuple):
        raise InvalidError(f"must be a list or tuple of keyframes (got {type(value).__name__})")
    return value


class _Points(ValidatedModel):
    """The `points` input as a model, so that a problem in point 1 is reported at `points[1].ease` by the base."""

    error: ClassVar[type[InvalidError]] = ParameterError
    points: Annotated[tuple[Keyframe, ...], BeforeValidator(_list_or_tuple)]

    @model_validator(mode="after")
    def _non_empty_and_increasing(self) -> "_Points":
        # here, not as a length constraint on the field: pydantic then adds "at least 1 item" when only items are bad
        if not self.points:
            raise InvalidError.at("points", "needs at least one keyframe")
        for index, (before, after) in enumerate(pairwise(self.points), start=1):
            if after.t <= before.t:
                raise InvalidError.at(f"points[{index}].t", f"{after.t} is not after the previous t = {before.t}")
        return self


class Input(NamedTuple):
    """One kind of input: what `help()` says about it, and how a value is checked."""

    doc: str
    check: Callable[[object], object]


INPUTS: Mapping[str, Input] = MappingProxyType(
    {
        "points": Input(
            doc=(
                "Keyframes: a list of {t, value, ease}, t in seconds from 0 and strictly increasing, ease the curve"
                " of the segment that starts there (ignored on the last point; default linear): one of"
                f" {', '.join(EASES)}, or cubic-bezier(x1, y1, x2, y2)."
            ),
            check=lambda value: _Points.parse({"points": value}).points,
        )
    }
)
