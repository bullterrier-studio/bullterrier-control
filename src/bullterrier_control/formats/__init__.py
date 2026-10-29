"""The plain-data formats this package owns (docs/formats.md): `Series`, `Event`, `Grid` and the `Bundle` that carries
them, and `Unit`. The classes are the Rust core's (`_native._core`), which holds every rule once; this package adds
the file input and output (`load`, `save`) and `events`, which makes events from times."""

from .._native._core import Bundle, Event, Grid, Series, Unit, bundle_schema, dumps
from .bundle import load, read_text, save
from .event import events

__all__ = [
    "Bundle",
    "Event",
    "Grid",
    "Series",
    "Unit",
    "bundle_schema",
    "dumps",
    "events",
    "load",
    "read_text",
    "save",
]
