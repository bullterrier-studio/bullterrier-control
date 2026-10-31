"""Patch: a named graph of sources, shapers and operations with their values, as data (docs/formats.md).

    format = "bullterrier-control/patch"
    version = 1

    [strong]
    use = "select"
    events = "events.kick"     # the bundle's events "kick"
    threshold = 0.7

    [glow]
    use = "envelope"
    events = "strong"          # the node above
    decay = 0.25

    patch = load_patch(Path("glow.toml"))              # a Patch, or a FormatError naming the file, the node and the key
    glow = patch.build(load(Path("kicks.json")))["glow"]    # every node, built against one bundle

A patch is a look of signals: it names what to make and with which values, never what the signals drive. The bundle is
not named in it: one patch serves every song, and the song is given when the patch is built.

Promises:
- A patch is a table with the header `format` and `version` (both required), then one table per node. A node has `use`
  (the name of a registered source, shaper or operation), `signal` (a shaper's input; refused for the others), one
  key for each input it takes and one for each parameter it declares. Anything else is refused: a patch cannot say
  what a signal drives.
- A node's input is data written inline (a list of events, of keyframes, a grid, a series) or a *reference*:
  `events.<name>` and `series.<name>` are entries of the bundle, `grid` is its grid, and any other name is a node
  written above. A reference must fit what the key takes: events for `events`, a series for `series`, the grid for
  `grid`, and for `signal` a signal or a series (a series is read linearly). `points` and `values` are written inline.
- `Patch.parse` and `load_patch` check, all at once, everything the patch and the registry decide: names, keys, types,
  ranges and references. Every problem is reported together as one `FormatError` that lists each as `<node>.<key>:
  rule`. Two things wait for `build`: the names a bundle must hold, and a rule a thing applies when it is called (the
  `lfo` with `per = "beat"` needs a `grid`), which is its own.
- `Patch.build(bundle)` gives every node by name, in file order: a `Signal` for a source or shaper, the events or the
  series for an operation (`only=` asks for some, building just those and what they use). The same patch and bundle
  give the same values. A node's `returns` says which, without building.
- Nothing here depends on a consumer: a patch held inside another file (a video project) is read with `Patch.parse`.
"""

import re
import tomllib
from collections.abc import Iterable, Mapping
from dataclasses import dataclass
from pathlib import Path
from types import MappingProxyType
from typing import Any

from ._native import _core
from .errors import FormatError, InvalidError, ParameterError, UnknownNameError
from .formats import Bundle, Series, read_text
from .inputs import INPUTS
from .registry import NAME_PATTERN, NODE_KEYS, get
from .signal import from_series

__all__ = ["Node", "Patch", "load_patch", "patch_schema"]

FORMAT = "bullterrier-control/patch"
VERSION = 1
# a node's name; `format` and `version` are the header, and `events`, `series` and `grid` start the paths into a bundle
_RESERVED = ("format", "version", "events", "series", "grid")
_NODE_NAME = r"^(?!(" + "|".join(_RESERVED) + r")$)" + NAME_PATTERN + "$"
# what a key may hold as a reference, and what each kind of result can serve
_REFERENCED = ("events", "series", "grid")
_ACCEPTS = {"signal": ("signal", "series"), "events": ("events",), "series": ("series",), "grid": ("grid",)}

type _Problems = list[tuple[str, str]]


@dataclass(frozen=True)
class _Reference:
    """A name written where data goes: a node above (`root` is "node") or an entry of the bundle (`root` is "events",
    "series" or "grid"; `name` is the entry, empty for the grid)."""

    text: str
    root: str
    name: str


@dataclass(frozen=True)
class Node:
    """One node as the patch says it: `use` the registered name, `returns` what it gives (`signal`, `events` or
    `series`), and what it is built from: `signal` and `inputs` (a `_Reference`, or data checked as the input's kind
    does) and the parameter values."""

    use: str
    returns: str
    signal: _Reference | None
    inputs: Mapping[str, object]
    parameters: Mapping[str, object]


def _reference(
    path: str, text: str, key: str, earlier: Mapping[str, str | None], later: set[str], problems: _Problems
) -> _Reference | None:
    """The reference `text` written at `path` where `key` is wanted, or None after a problem is added to `problems`.
    `earlier` maps the nodes above to what they give (None for one that is itself broken)."""
    root, _, name = text.partition(".")
    given: str | None
    if root in ("events", "series"):
        if not _core.is_bundle_name(name):
            problems.append((path, f"{text!r} is not a path into a bundle: {root}.<name>, the name in lower case"))
            return None
        given, reference = root, _Reference(text, root, name)
    elif text == "grid":
        given, reference = "grid", _Reference(text, "grid", "")
    elif text in earlier:
        given, reference = earlier[text], _Reference(text, "node", text)
    elif text in later:
        problems.append((path, f"{text!r} is a node below this one; a node uses only the nodes above it"))
        return None
    else:
        known = ", ".join(earlier) or "none"
        paths = "events.<name>, series.<name>, grid"
        problems.append(
            (path, f"{text!r} is no node above this one (nodes above: {known}) and no bundle path ({paths})")
        )
        return None
    if given is not None and given not in _ACCEPTS[key]:
        problems.append((path, f"{text!r} gives {given}, but {key} takes {' or '.join(_ACCEPTS[key])}"))
        return None
    return reference


def _node(
    name: str, table: Mapping[str, object], earlier: Mapping[str, str | None], later: set[str], problems: _Problems
) -> Node | None:
    """The node `name` written as `table`; every problem found is added to `problems`. None if `use` is unusable."""
    if "use" not in table:
        problems.append((f"{name}.use", "is required: the name of a source, shaper or operation"))
        return None
    use = table["use"]
    if not isinstance(use, str):
        problems.append((f"{name}.use", f"must be text, the name of a source, shaper or operation (got {use!r})"))
        return None
    try:
        thing = get(use)
    except UnknownNameError as error:
        problems.append((f"{name}.use", str(error)))
        return None

    signal = None
    if thing.kind == "shaper":
        if "signal" not in table:
            problems.append((f"{name}.signal", "is required: a shaper takes a signal"))
        elif isinstance(table["signal"], str):
            signal = _reference(f"{name}.signal", table["signal"], "signal", earlier, later, problems)
        else:
            problems.append((f"{name}.signal", "must be a reference: a node above, or series.<name>"))
    elif "signal" in table:
        problems.append((f"{name}.signal", f"{thing.kind} {use!r} takes no signal"))

    inputs: dict[str, object] = {}
    parameters: dict[str, object] = {}
    for key, value in table.items():
        path = f"{name}.{key}"
        if key in NODE_KEYS:
            continue
        if key in thing.inputs:
            if isinstance(value, str) and key in _REFERENCED:
                found = _reference(path, value, key, earlier, later, problems)
                if found is not None:
                    inputs[key] = found
                continue
            try:
                inputs[key] = INPUTS[key].check(value)
            except ParameterError as error:
                problems.extend((f"{name}.{where}", rule) for where, rule in error.problems)
        elif key in thing.params_model.describe():
            parameters[key] = value
        else:
            takes = [*thing.inputs, *thing.params_model.describe()]
            taken = ", ".join(takes) or "nothing"
            problems.append((path, f"is neither an input nor a parameter of {thing.kind} {use!r} (it takes {taken})"))
    for key, required in thing.inputs.items():
        if required and key not in table:
            problems.append((f"{name}.{key}", f"is required: {use!r} takes {key}"))
    try:
        thing.validate(parameters)
    except ParameterError as error:
        problems.extend((f"{name}.{where}", rule) for where, rule in error.problems)
    return Node(use, thing.returns, signal, MappingProxyType(inputs), MappingProxyType(parameters))


@dataclass(frozen=True)
class Patch:
    """The nodes of a patch by name, in file order. Make one with `Patch.parse` or `load_patch`."""

    nodes: Mapping[str, Node]

    @classmethod
    def parse(cls, data: Mapping[str, object]) -> "Patch":
        """The patch in `data` (a decoded table), or a `FormatError` listing every problem as `<node>.<key>: rule`."""
        problems: _Problems = []
        if "format" not in data:
            problems.append(("format", f"is required: {FORMAT!r}"))
        elif data["format"] != FORMAT:
            problems.append(("format", f"must be {FORMAT!r} (got {data['format']!r})"))
        if "version" not in data:
            problems.append(("version", f"is required: {VERSION}"))
        elif type(data["version"]) is not int or data["version"] != VERSION:
            problems.append(
                (
                    "version",
                    f"must be the whole number {VERSION}, the one version this reader knows (got {data['version']!r})",
                )
            )
        tables = {name: table for name, table in data.items() if name not in ("format", "version")}
        nodes: dict[str, Node] = {}
        earlier: dict[str, str | None] = {}
        for name, table in tables.items():
            if not isinstance(name, str):
                problems.append((str(name), "is not a node name: a key must be text"))
                continue
            if not re.fullmatch(_NODE_NAME, name):
                rule = "lower case letters, digits and _, starting with a letter, and not one of " + ", ".join(
                    _RESERVED
                )
                problems.append((name, f"is not a node name: {rule}"))
            if not isinstance(table, Mapping):
                problems.append((name, "must be a node: a table with `use`"))
                earlier[name] = None
                continue
            node = _node(name, table, earlier, set(tables) - set(earlier), problems)
            earlier[name] = node.returns if node is not None else None
            if node is not None:
                nodes[name] = node
        if problems:
            listed = "; ".join(f"{path}: {rule}" for path, rule in problems)
            raise FormatError(f"Patch: {listed}", tuple(problems))
        return cls(MappingProxyType(nodes))

    def build(self, bundle: Bundle | None = None, only: Iterable[str] | None = None) -> dict[str, Any]:
        """Every node by name, in file order, built against `bundle` (None when the patch names nothing in one).

        A `Signal` for a source or shaper; for an operation its events or its series. With `only`, just those nodes
        come back, and only they and what they use are built, so the rest of the patch needs nothing of the bundle; a
        name the patch lacks is an `UnknownNameError`. A path into the bundle that it does not hold, or any path without
        a bundle, is a `FormatError` at `<node>.<key>`; an error a source, shaper or operation raises keeps its kind
        and starts with the node."""
        wanted = self._needed(only) if only is not None else set(self.nodes)
        made: dict[str, Any] = {}
        for name, node in self.nodes.items():
            if name not in wanted:
                continue
            signals = () if node.signal is None else (_resolve(name, "signal", node.signal, "signal", made, bundle),)
            inputs = {
                key: _resolve(name, key, value, key, made, bundle) if isinstance(value, _Reference) else value
                for key, value in node.inputs.items()
            }
            try:
                made[name] = get(node.use)(*signals, **inputs, **node.parameters)
            except InvalidError as error:
                problems = tuple((f"{name}.{where}", rule) for where, rule in error.problems)
                raise type(error)(f"{name}: {error}", problems) from None
        return made if only is None else {name: made[name] for name in made if name in set(only)}

    def _needed(self, names: Iterable[str]) -> set[str]:
        """The nodes `names` and everything they use. A node uses only nodes above it, so one pass upward finds all."""
        needed = set(names)
        for name in needed - self.nodes.keys():
            raise UnknownNameError(f"unknown node {name!r}; the patch has: {', '.join(self.nodes) or 'none'}")
        for name in reversed(list(self.nodes)):
            if name in needed:
                node = self.nodes[name]
                uses = [node.signal, *node.inputs.values()]
                needed |= {use.name for use in uses if isinstance(use, _Reference) and use.root == "node"}
        return needed


def _resolve(
    name: str, key: str, reference: _Reference, wanted: str, made: Mapping[str, Any], bundle: Bundle | None
) -> object:
    """What `reference` (the `key` of node `name`) points at: a node already built, or an entry of `bundle`. A series
    is read linearly where a signal is wanted."""
    path = f"{name}.{key}"
    if reference.root == "node":
        found: object = made[reference.name]
    elif bundle is None:
        raise FormatError.at(path, f"{reference.text!r} is a path into a bundle, and none was given")
    elif reference.root == "grid":
        if bundle.grid is None:
            raise FormatError.at(path, "the bundle has no grid")
        found = bundle.grid
    else:
        held = bundle.events if reference.root == "events" else bundle.series
        if reference.name not in held:
            has = ", ".join(sorted(held)) or "none"
            raise FormatError.at(path, f"the bundle has no {reference.root} {reference.name!r} (it has: {has})")
        found = held[reference.name]
    return from_series(found) if wanted == "signal" and isinstance(found, Series) else found


def load_patch(path: Path) -> Patch:
    """The Patch in the TOML file at `path`, or a `FormatError` starting with the path when the file is not valid."""
    text = read_text(path)
    try:
        data = tomllib.loads(text)
    except tomllib.TOMLDecodeError as error:
        raise FormatError(f"{path}: not TOML ({error})") from None
    # what the parser itself refuses: nesting deeper than Python can follow, an integer of thousands of digits
    except (RecursionError, ValueError) as error:
        raise FormatError(f"{path}: cannot be read as TOML ({error})") from None
    try:
        return Patch.parse(data)
    except FormatError as error:
        raise FormatError(f"{path}: {error}", error.problems) from None


def patch_schema() -> dict[str, object]:
    """The JSON Schema of a patch (as decoded from TOML), for tools in other languages. It states the header, the shape
    of a node and its name; what a schema cannot state is enforced by `Patch.parse`: that `use` names a registered
    thing, each key an input or parameter of it, the values, and that a reference fits and names a node above."""
    value = {"type": ["number", "string", "boolean", "array", "object"]}
    return {
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Patch",
        "type": "object",
        "required": ["format", "version"],
        "properties": {"format": {"const": FORMAT}, "version": {"const": VERSION}},
        "patternProperties": {_NODE_NAME: {"$ref": "#/$defs/node"}},
        "additionalProperties": False,
        "$defs": {
            "node": {
                "type": "object",
                "required": ["use"],
                "properties": {
                    "use": {"type": "string", "pattern": f"^{NAME_PATTERN}$"},
                    "signal": {"type": "string"},
                },
                "additionalProperties": value,
            }
        },
    }
