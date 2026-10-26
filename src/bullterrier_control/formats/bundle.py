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
from collections.abc import Mapping
from pathlib import Path
from types import MappingProxyType
from typing import Annotated, Literal

from pydantic import BeforeValidator, Field, field_serializer, field_validator, model_validator

from ..errors import FormatError, InvalidError
from ..validated import Finite, Text, whole_number
from .event import Event
from .grid import Grid
from .model import FormatModel, read_text
from .series import Series

__all__ = ["NAME_PATTERN", "Bundle", "bundle_schema", "dumps", "load", "save"]

# the name of a series or an event list: `bass`, `stem.drums`, `kick`
NAME_PATTERN = r"^[a-z0-9][a-z0-9_.-]*$"
Name = Annotated[str, Field(pattern=NAME_PATTERN)]


class Bundle(FormatModel):
    """Everything one producer measured of one piece of material: named Series, named Event lists, a Grid."""

    format: Literal["bullterrier-control/bundle"]
    version: Annotated[Literal[1], BeforeValidator(whole_number)]
    source: Text | None = None
    duration: Annotated[Finite, Field(gt=0)] | None = None
    grid: Grid | None = None
    series: Mapping[Name, Series] = Field(default_factory=lambda: MappingProxyType({}))
    events: Mapping[Name, tuple[Event, ...]] = Field(default_factory=lambda: MappingProxyType({}))

    @field_validator("series", "events", mode="after")
    @classmethod
    def _read_only(cls, value: Mapping[str, object]) -> Mapping[str, object]:
        # the model is frozen; its mappings must not be a way round that (an edit would skip every rule)
        return MappingProxyType(dict(value))

    @field_serializer("series", "events")
    def _as_dict(self, value: Mapping[str, object]) -> dict[str, object]:
        return dict(value)  # a read-only view is not something the serializer knows

    @model_validator(mode="after")
    def _events_are_sorted_and_everything_starts_inside_the_material(self) -> "Bundle":
        for name, found in self.events.items():
            for index in range(1, len(found)):
                if found[index].t < found[index - 1].t:
                    now, before = found[index].t, found[index - 1].t
                    raise InvalidError.at(
                        f"events.{name}[{index}]",
                        f"t = {now} comes before t = {before} of the event before it: sort by t",
                    )
            if self.duration is not None:
                late = next((event for event in found if event.t > self.duration), None)
                if late is not None:
                    raise InvalidError.at(f"events.{name}", f"t = {late.t} is after the duration of {self.duration}")
        if self.duration is not None:
            for name, series in self.series.items():
                if series.start > self.duration:
                    raise InvalidError.at(
                        f"series.{name}.start", f"{series.start} is after the duration of {self.duration}"
                    )
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
    text = read_text(path)
    try:
        data = json.loads(text, object_pairs_hook=_no_repeated_key, parse_constant=_no_constant)
        if not isinstance(data, dict):
            raise FormatError("the top level must be a JSON object")
        return Bundle.parse(data)
    except json.JSONDecodeError as error:
        raise FormatError(f"{path}: not JSON ({error.msg}, line {error.lineno} column {error.colno})") from None
    except FormatError as error:
        raise FormatError(f"{path}: {error}") from None
    # what the JSON decoder itself refuses: nesting deeper than Python can follow, an integer of thousands of digits
    except (RecursionError, ValueError) as error:
        raise FormatError(f"{path}: cannot be read as JSON ({error})") from None


def dumps(bundle: Bundle) -> str:
    """The JSON text of `bundle`, ending in a newline; absent optional fields are left out."""
    return json.dumps(bundle.model_dump(mode="json", exclude_none=True), ensure_ascii=False) + "\n"


def save(bundle: Bundle, path: Path) -> None:
    """Write `bundle` to `path` as UTF-8 JSON, replacing the file; absent optional fields are left out."""
    path.write_text(dumps(bundle), encoding="utf-8")


def bundle_schema() -> dict[str, object]:
    """The JSON Schema of a Bundle file, for tools in other languages. It states shapes, types and ranges; the rules a
    schema cannot state (sorted events, duration bounds, increasing beats, repeated keys) are enforced by `load`."""
    schema = Bundle.model_json_schema()
    for field in ("series", "events"):  # the name rule: a key outside the pattern is not allowed
        schema["properties"][field]["additionalProperties"] = False
    return schema
