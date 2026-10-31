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
  The rules are the Rust core's (`bullterrier_control._native._core.check_input`); this module only names them.
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

from collections.abc import Callable, Mapping
from functools import partial
from types import MappingProxyType
from typing import NamedTuple

from ._native import _core
from .formats import Event

__all__ = ["INPUTS", "Input", "Keyframe", "matching"]

Keyframe = _core.Keyframe
"""One point of a keyframed signal: the value at a time in seconds, and the ease of the segment that starts here
(ignored on the last point). `Keyframe(t=0, value=1, ease="in-out")` or `Keyframe.parse(mapping)`; frozen."""


def matching(events: tuple[Event, ...], label: str) -> tuple[Event, ...]:
    """The events whose label is `label`, in their order; every event, unlabelled ones too, when `label` is empty."""
    return tuple(event for event in events if label in ("", event.label))


class Input(NamedTuple):
    """One kind of input: what `help()` says about it, how a value is checked, and the type text (`annotation`) a stub
    writes for its keyword."""

    doc: str
    check: Callable[[object], object]
    annotation: str


# the type text a stub writes for each kind's keyword: Python-only knowledge, so it is not in the core
_ANNOTATIONS = {
    "points": "Sequence[Keyframe | Mapping[str, object]]",
    "events": "Sequence[Event | Mapping[str, object]]",
    "grid": "Grid | Mapping[str, object]",
    "values": "Sequence[float]",
    "series": "Series | Mapping[str, object]",
}

INPUTS: Mapping[str, Input] = MappingProxyType(
    {name: Input(doc, partial(_core.check_input, name), _ANNOTATIONS[name]) for name, doc in _core.input_docs().items()}
)
