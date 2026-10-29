"""The data a source takes besides its parameters, and the one place that says what each kind is and how it is checked.

    @source(params=TrackParams)
    def track(params: TrackParams, *, points: tuple[Keyframe, ...]) -> Signal: ...

    track(points=[{"t": 0, "value": 0}, {"t": 2, "value": 1, "ease": "in-out"}])

A parameter is a number or a choice; an input is a structure (keyframes, events, a grid). A function declares the
inputs it takes as keyword-only arguments named in `INPUTS`; the registry knows no kind by name: it looks the argument
up here and calls `check`. Adding a kind is one entry in `INPUTS`.

Promises:
- `INPUTS[name].check(value)` returns the value in its checked form, or raises `ParameterError` listing each broken rule
  as `path: rule` with the path from the input's name (`points[1].ease`). Mappings are parsed into their model.
- `points`: a non-empty list or tuple of `Keyframe` or mappings; `t` strictly increases. Checked, it is a
  `tuple[Keyframe, ...]`.
- `events`: a list or tuple of `Event` or mappings, possibly empty, in any order. Checked, it is a `tuple[Event, ...]`
  sorted by `t`; events at the same time keep the order they were given in.
- `grid`: a `Grid` or a mapping parsed into one; a bad grid is a `ParameterError` (not a `FormatError`) naming
  `grid.<field>`.
- `values`: a non-empty list or tuple of finite numbers (no bool, no text). Checked, it is a `tuple[float, ...]`.
- `series`: a `Series` or a mapping parsed into one; a bad one is a `ParameterError` (not a `FormatError`) naming
  `series.<field>`.
- `matching(events, label)`: the events a `label` parameter picks, shared by everything that has one.
"""

from collections.abc import Callable, Mapping, Sequence
from functools import partial
from itertools import pairwise
from types import MappingProxyType
from typing import Annotated, Any, ClassVar, NamedTuple

from pydantic import AfterValidator, BeforeValidator, Field, model_validator

from .easing import EASES, curve
from .errors import FormatError, InvalidError, ParameterError
from .formats import Event, Grid, Series
from .validated import Finite, Text, ValidatedModel

__all__ = ["INPUTS", "Input", "Keyframe", "matching"]


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
        raise InvalidError(f"must be a list or tuple (got {type(value).__name__})")
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


class _Values(ValidatedModel):
    """The `values` input as a model, for the same reason as `_Points`."""

    error: ClassVar[type[InvalidError]] = ParameterError
    values: Annotated[tuple[Finite, ...], BeforeValidator(_list_or_tuple)]

    @model_validator(mode="after")
    def _non_empty(self) -> "_Values":
        if not self.values:
            raise InvalidError.at("values", "needs at least one value")
        return self


def matching(events: tuple[Event, ...], label: str) -> tuple[Event, ...]:
    """The events whose label is `label`, in their order; every event, unlabelled ones too, when `label` is empty."""
    return tuple(event for event in events if label in ("", event.label))


def _checked(holder: type[ValidatedModel], field: str, value: object) -> Any:
    """`value` as the holder model's `field`, or a `ParameterError` listing each broken rule as `path: rule`."""
    try:
        return getattr(holder.parse({field: value}), field)
    except ParameterError as error:
        # the model's own message starts with its private name; the registry names the thing that refused instead
        raise _refused(error.problems) from None


def _refused(problems: Sequence[tuple[str, str]]) -> ParameterError:
    """The `ParameterError` listing `problems` as `path: rule`."""
    return ParameterError("; ".join(f"{path}: {rule}" for path, rule in problems), tuple(problems))


def _under(prefix: str, path: str) -> str:
    """`path` (of a format's field: `t`, `beats[1]`, or empty for the whole) under `prefix` (`events[2]`, `grid`)."""
    return f"{prefix}.{path}" if path else prefix


def _format[F: (Grid, Series)](kind: type[F], field: str, value: object) -> F:
    """`value` as the format `kind` (itself, or a mapping parsed into one), or a `ParameterError` with each problem
    under `field` (`grid.beats[1]`)."""
    if isinstance(value, kind):
        return value
    try:
        return kind.parse(value)  # type: ignore[arg-type]  # anything else is refused by the format, naming it
    except FormatError as error:
        raise _refused([(_under(field, path), rule) for path, rule in error.problems]) from None


def _events(value: object) -> tuple[Event, ...]:
    """`value`, a list or tuple of `Event` or mappings, as events sorted by `t` (equal times keep the order given), or
    a `ParameterError` listing every problem under its place (`events[1].strength`)."""
    if not isinstance(value, list | tuple):  # a set or a generator has no order to keep
        raise _refused([("events", f"must be a list or tuple (got {type(value).__name__})")])
    found: list[Event] = []
    problems: list[tuple[str, str]] = []
    for index, item in enumerate(value):
        try:
            found.append(item if isinstance(item, Event) else Event.parse(item))
        except FormatError as error:
            problems.extend((_under(f"events[{index}]", path), rule) for path, rule in error.problems)
    if problems:
        raise _refused(problems)
    return tuple(sorted(found, key=lambda event: event.t))  # stable: equal times keep the order given


class Input(NamedTuple):
    """One kind of input: what `help()` says about it, how a value is checked, and the type text (`annotation`) a stub
    writes for its keyword."""

    doc: str
    check: Callable[[object], object]
    annotation: str


INPUTS: Mapping[str, Input] = MappingProxyType(
    {
        "points": Input(
            doc=(
                "Keyframes: a list of {t, value, ease}, t in seconds from 0 and strictly increasing, ease the curve"
                " of the segment that starts there (ignored on the last point; default linear): one of"
                f" {', '.join(EASES)}, or cubic-bezier(x1, y1, x2, y2)."
            ),
            check=partial(_checked, _Points, "points"),
            annotation="Sequence[Keyframe | Mapping[str, object]]",
        ),
        "events": Input(
            doc=(
                "Events: a list of {t, strength, duration, label, pitch}, as in a bundle, in any order (checked into"
                " time order)."
            ),
            check=_events,
            annotation="Sequence[Event | Mapping[str, object]]",
        ),
        "grid": Input(
            doc=(
                "A beat grid: {beats, meter, first_downbeat} as in a bundle (beats in seconds, strictly increasing,"
                " at least two). Converts seconds to beats, so a source can follow tempo changes."
            ),
            check=partial(_format, Grid, "grid"),
            annotation="Grid | Mapping[str, object]",
        ),
        "values": Input(
            doc="Values: a non-empty list of numbers, in the order a source cycles through them.",
            check=partial(_checked, _Values, "values"),
            annotation="Sequence[float]",
        ),
        "series": Input(
            doc=(
                "A series: {start, rate, values, unit, scaling} as in a bundle (values at a fixed rate in samples per"
                " second from a start time in seconds)."
            ),
            check=partial(_format, Series, "series"),
            annotation="Series | Mapping[str, object]",
        ),
    }
)
