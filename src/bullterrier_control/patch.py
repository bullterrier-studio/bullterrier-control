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
    Patch.from_signals({"glow": glow}).save(Path("again.toml"))     # a graph made in code (or built), as a patch

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
- `Patch.from_signals({"glow": glow})` saves signals made through the registry (`lib.slew(...)`, a chain method, a patch
  built) as a patch, in the order given: each thing a signal is made of becomes a node above it (one of its own named
  `<name>_<n>` when unnamed), data inputs are written inline. A signal a patch cannot hold (made with an operator, a
  `Constant`, `from_values`, a `compute` of its own, a `Rendered(run=...)`) or a name no node may have is a
  `FormatError` listing each as `<path>: reason`. `to_toml()` and `save(path)` write it as text that `load_patch` reads
  back as an equal patch, building the same signals.
- Nothing here depends on a consumer: a patch held inside another file (a video project) is read with `Patch.parse`.
- Every rule is the Rust core's (`_native._core.Patch`); a patch is read against the registry as it holds things then.
"""

from collections.abc import Iterable, Mapping
from pathlib import Path
from typing import Any, Self

from ._native import _core
from .errors import FormatError
from .formats import Bundle, read_text
from .registry import table

__all__ = ["Node", "Patch", "load_patch", "patch_schema"]

Node = _core.Node
"""One node as the patch says it: `use` the registered name, `returns` what it gives (`signal`, `events` or `series`),
and what it is built from: `signal` (a shaper's `Reference`, else `None`), `inputs` (each a `Reference` or the data
written inline) and `parameters` (the values written), both read-only mappings."""


class Patch:
    """The nodes of a patch by name, in file order. Make one with `Patch.parse`, `load_patch` or
    `Patch.from_signals`."""

    __slots__ = ("_patch",)

    def __init__(self, patch: _core.Patch, /) -> None:
        """Hold the core's `patch`; the class methods and `load_patch` make one."""
        self._patch = patch

    @classmethod
    def parse(cls, data: Mapping[str, object]) -> Self:
        """The patch in `data` (a decoded table), or a `FormatError` listing every problem as `<node>.<key>: rule`."""
        return cls(_core.Patch.parse(data, registry=table()))

    @classmethod
    def from_signals(cls, signals: Mapping[str, _core.SignalBase]) -> Self:
        """The patch that makes `signals` (a node name to a `Signal`), in their order, or a `FormatError` naming the
        path and the reason of each signal a patch cannot hold."""
        return cls(_core.Patch.from_signals(signals.items(), registry=table()))

    @property
    def nodes(self) -> Mapping[str, Node]:
        """Each node by name, in file order."""
        return self._patch.nodes

    def build(self, bundle: Bundle | None = None, only: Iterable[str] | None = None) -> dict[str, Any]:
        """Every node by name, in file order, built against `bundle` (None when the patch names nothing in one).

        A `Signal` for a source or shaper; for an operation its events or its series. With `only`, just those nodes
        come back, and only they and what they use are built, so the rest of the patch needs nothing of the bundle; a
        name the patch lacks is an `UnknownNameError`. A path into the bundle that it does not hold, or any path without
        a bundle, is a `FormatError` at `<node>.<key>`; an error a source, shaper or operation raises keeps its kind
        and starts with the node."""
        return self._patch.build(bundle, only)

    def to_toml(self) -> str:
        """The patch as TOML text: the header, then a table per node in file order, data inline."""
        return self._patch.to_toml()

    def save(self, path: Path) -> None:
        """Write the patch to `path` as UTF-8 TOML (`to_toml`), replacing the file."""
        path.write_text(self.to_toml(), encoding="utf-8")

    def __eq__(self, other: object) -> bool:
        """Equal when the nodes name the same things with the same references, inputs and values."""
        return isinstance(other, Patch) and self._patch == other._patch

    __hash__ = None  # type: ignore[assignment]  # equal by its nodes, which may change meaning with a registry: unhashed


def load_patch(path: Path) -> Patch:
    """The Patch in the TOML file at `path`, or a `FormatError` starting with the path when the file is not valid."""
    text = read_text(path)
    try:
        return Patch(_core.Patch.from_toml(text, registry=table()))
    except FormatError as error:
        raise FormatError(f"{path}: {error}", error.problems) from None


def patch_schema() -> dict[str, Any]:
    """The JSON Schema of a patch (as decoded from TOML), for tools in other languages. It states the header, the shape
    of a node and its name; what a schema cannot state is enforced by `Patch.parse`: that `use` names a registered
    thing, each key an input or parameter of it, the values, and that a reference fits and names a node above."""
    return _core.patch_schema()
