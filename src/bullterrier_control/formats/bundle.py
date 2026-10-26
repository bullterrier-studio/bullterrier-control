"""Bundle: the one file type that carries Series, Events and a Grid (docs/formats.md), with `load` and `save`.

    bundle = load(Path("analysis.json"))       # a Bundle, or a FormatError naming the file and what is wrong
    save(bundle, Path("analysis.json"))        # UTF-8 JSON, one object, ending in a newline

Promises:
- `load` accepts exactly what the format allows: valid UTF-8 JSON whose top level is an object, with no repeated key and
  no `NaN` or infinity, and every rule of the format met. Anything else is one `FormatError` that starts with the path.
  A file that cannot be read at all raises the `OSError` the system gave (a missing file stays a `FileNotFoundError`).
- `save` writes what `load` reads back as an equal bundle, leaving out every optional field that is absent.
"""

import json
from pathlib import Path
from typing import Annotated, Literal

from pydantic import Field, field_validator, model_validator

from ..errors import FormatError
from .event import Event
from .grid import Grid
from .model import Finite, FormatModel
from .series import Series

__all__ = ["Bundle", "load", "save"]

# the name of a series or an event list: `bass`, `stem.drums`, `kick`
Name = Annotated[str, Field(pattern=r"^[a-z0-9][a-z0-9_.-]*$")]


class Bundle(FormatModel):
    """Everything one producer measured of one piece of material: named Series, named Event lists, a Grid."""

    format: Literal["bullterrier-control/bundle"]
    version: Literal[1]
    source: str | None = None
    duration: Annotated[Finite, Field(gt=0)] | None = None
    grid: Grid | None = None
    series: dict[Name, Series] = Field(default_factory=dict)
    events: dict[Name, tuple[Event, ...]] = Field(default_factory=dict)

    @field_validator("version", mode="before")
    @classmethod
    def _version_is_a_whole_number(cls, value: object) -> object:
        # a Literal accepts True and 1.0 for 1; a version is the integer written in the file
        if type(value) is not int:
            raise FormatError(f"the version is the whole number 1 (got {value!r})")
        return value

    @model_validator(mode="after")
    def _events_are_sorted_and_everything_starts_inside_the_material(self) -> "Bundle":
        for name, found in self.events.items():
            for index in range(1, len(found)):
                if found[index].t < found[index - 1].t:
                    raise FormatError(
                        f"events.{name}[{index}]: t = {found[index].t} comes before t = {found[index - 1].t}"
                        " of the event before it; events are sorted by t"
                    )
            if self.duration is not None:
                late = next((event for event in found if event.t > self.duration), None)
                if late is not None:
                    raise FormatError(f"events.{name}: t = {late.t} is after the duration of {self.duration}")
        if self.duration is not None:
            for name, series in self.series.items():
                if series.start > self.duration:
                    raise FormatError(f"series.{name}: start = {series.start} is after the duration of {self.duration}")
        return self


def _no_repeated_key(pairs: list[tuple[str, object]]) -> dict[str, object]:
    found: dict[str, object] = {}
    for key, value in pairs:
        if key in found:
            raise FormatError(f"the key {key!r} is repeated in one object")
        found[key] = value
    return found


def _no_constant(name: str) -> object:
    raise FormatError(f"{name} is not a number a file may hold")


def load(path: Path) -> Bundle:
    """The Bundle in the JSON file at `path`, or a `FormatError` starting with the path when the file is not valid."""
    text = _read(path)
    try:
        data = json.loads(text, object_pairs_hook=_no_repeated_key, parse_constant=_no_constant)
        if not isinstance(data, dict):
            raise FormatError("the top level must be a JSON object")
        return Bundle.parse(data)
    except json.JSONDecodeError as error:
        raise FormatError(f"{path}: not JSON ({error.msg}, line {error.lineno} column {error.colno})") from None
    except FormatError as error:
        raise FormatError(f"{path}: {error}") from None


def _read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except UnicodeDecodeError as error:
        raise FormatError(f"{path}: not UTF-8 text ({error.reason})") from None


def save(bundle: Bundle, path: Path) -> None:
    """Write `bundle` to `path` as UTF-8 JSON, replacing the file; absent optional fields are left out."""
    data = bundle.model_dump(mode="json", exclude_none=True)
    path.write_text(json.dumps(data, ensure_ascii=False) + "\n", encoding="utf-8")
