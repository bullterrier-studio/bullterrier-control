"""The generated bundle schema means what the format's rules say.

One table of hand-written documents the schema can speak about: the schema and `Bundle.parse` give the same verdict on
each. A second table holds the rules a JSON Schema cannot state; the schema accepts those documents (so the table marks
the schema's limit) and the library refuses them. Which rule belongs to which table is read off the generated schema:
a row that moves when the schema learns a rule fails here, on purpose.
"""

import json
from typing import Final

import jsonschema
import pytest

from bullterrier_control import Bundle, FormatError
from bullterrier_control.formats import bundle_schema

FORMAT: Final = "bullterrier-control/bundle"
MINIMAL: Final = {"format": FORMAT, "version": 1}
SERIES: Final = {"rate": 10, "values": [0.0, 1.0], "unit": "hertz"}
FRACTION: Final = {"rate": 10, "values": [0.0, 0.5, 1.0], "unit": "fraction", "scaling": "peak of the song"}
GRID: Final = {"beats": [0.5, 1.0, 1.5, 2.0], "meter": 4, "first_downbeat": 0}
EVENT_WITH_EVERYTHING: Final = {"t": 1.5, "strength": 0.25, "duration": 0.5, "label": "A", "pitch": 60.5}


def bundle(**parts: object) -> dict[str, object]:
    """A minimal bundle with the given parts added."""
    return {**MINIMAL, **parts}


def with_series(**fields: object) -> dict[str, object]:
    """A bundle holding one hertz series `s` with the given fields changed."""
    return bundle(series={"s": {**SERIES, **fields}})


def with_event(**fields: object) -> dict[str, object]:
    """A bundle holding one event `{"t": 1.0}` in the list `k`, with the given fields changed."""
    return bundle(events={"k": [{"t": 1.0, **fields}]})


def with_grid(**fields: object) -> dict[str, object]:
    """A bundle holding a grid with the given fields changed."""
    return bundle(grid={**GRID, **fields})


@pytest.fixture(scope="module")
def schema_accepts():
    """`schema_accepts(document)`: the verdict of the generated schema, under the draft it declares."""
    schema = bundle_schema()
    validator_class = jsonschema.validators.validator_for(schema)
    validator_class.check_schema(schema)
    validator = validator_class(schema)
    return lambda document: validator.is_valid(document)


def parse(document: dict[str, object] | str) -> Bundle:
    """Read a document with the library: a dict as given, a string as JSON text."""
    return Bundle.parse(document) if isinstance(document, dict) else Bundle.from_json(document)


def as_data(document: dict[str, object] | str) -> object:
    """What a JSON tool sees of the document."""
    return json.loads(document) if isinstance(document, str) else document


# The rules the schema states: both readers agree. A row ends in None for "accepted", else in the field path the
# library's refusal names.
STATED_BY_THE_SCHEMA = {
    # accepted
    "empty bundle is format and version": (MINIMAL, None),
    "a bundle with every section": (
        bundle(
            source="test 1.0 song.wav",
            duration=10,
            grid=GRID,
            series={"bass": FRACTION, "stem.drums": SERIES},
            events={"kick": [{"t": 0.5}, {"t": 1.5, "strength": 1}], "sections": [EVENT_WITH_EVERYTHING]},
        ),
        None,
    ),
    "a minimal series": (with_series(), None),
    "a fraction series states its scaling": (bundle(series={"bass": FRACTION}), None),
    "an event with every optional field": (bundle(events={"k": [EVENT_WITH_EVERYTHING]}), None),
    "a whole number is fine where a decimal is expected": (with_series(rate=10, start=1, values=[1, 2]), None),
    "an event list may be empty": (bundle(events={"k": []}), None),
    "the allowed name characters": (bundle(events={"0": [], "a_b-c.d": [], "stem.drums": []}), None),
    "the same name in series and events": (bundle(series={"k": SERIES}, events={"k": []}), None),
    "the boundaries of strength and pitch": (
        bundle(events={"k": [{"t": 0, "strength": 0, "pitch": 0}, {"t": 0, "strength": 1, "pitch": 127}]}),
        None,
    ),
    "null means absent: bundle fields": (bundle(source=None, duration=None, grid=None), None),
    "null means absent: series fields": (with_series(start=None, scaling=None), None),
    "null means absent: event fields": (with_event(strength=None, duration=None, label=None, pitch=None), None),
    # refused
    "wrong format": (bundle(format="bullterrier-audio/bundle"), "format:"),
    "missing format": ({"version": 1}, "format:"),
    "missing version": ({"format": FORMAT}, "version:"),
    "version 2": (bundle(version=2), "version:"),
    "version as a string": (bundle(version="1"), "version:"),
    "unknown top-level field": (bundle(tempo=120), "tempo:"),
    "series name in capitals": (bundle(series={"Bass": SERIES}), "series.Bass:"),
    "series name with a leading dot": (bundle(series={".bass": SERIES}), "series..bass:"),
    "series name empty": (bundle(series={"": SERIES}), "series.:"),
    "events name in capitals": (bundle(events={"Kick": []}), "events.Kick:"),
    "events name with a leading dot": (bundle(events={".kick": []}), "events..kick:"),
    "events name empty": (bundle(events={"": []}), "events.:"),
    "series is a map, not null": (bundle(series=None), "series:"),
    "unknown unit": (with_series(unit="percent"), "series.s.unit:"),
    "series rate 0": (with_series(rate=0), "series.s.rate:"),
    "series rate negative": (with_series(rate=-1), "series.s.rate:"),
    "series rate missing": (bundle(series={"s": {"values": [1.0], "unit": "hertz"}}), "series.s.rate:"),
    "series values empty": (with_series(values=[]), "series.s.values:"),
    "series start negative": (with_series(start=-0.5), "series.s.start:"),
    "series rate as a string": (with_series(rate="10"), "series.s.rate:"),
    "series value as a boolean": (with_series(values=[True]), "series.s.values[0]:"),
    "unknown field in a series": (with_series(smoothing=1), "series.s.smoothing:"),
    "event t negative": (with_event(t=-1), "events.k[0].t:"),
    "event t missing": (bundle(events={"k": [{"strength": 1}]}), "events.k[0].t:"),
    "event strength above 1": (with_event(strength=1.5), "events.k[0].strength:"),
    "event strength below 0": (with_event(strength=-0.1), "events.k[0].strength:"),
    "event duration 0": (with_event(duration=0), "events.k[0].duration:"),
    "event pitch 128": (with_event(pitch=128), "events.k[0].pitch:"),
    "event pitch negative": (with_event(pitch=-1), "events.k[0].pitch:"),
    "event label empty": (with_event(label=""), "events.k[0].label:"),
    "event t as a string": (with_event(t="1.0"), "events.k[0].t:"),
    "unknown field in an event": (with_event(colour="red"), "events.k[0].colour:"),
    "an event given as an array": (bundle(events={"k": [[1.0]]}), "events.k[0]:"),
    "an event list given as one event": (bundle(events={"k": {"t": 1.0}}), "events.k:"),
    "bundle duration 0": (bundle(duration=0), "duration:"),
    "bundle duration as a string": (bundle(duration="10"), "duration:"),
    "grid with one beat": (with_grid(beats=[1.0]), "grid.beats:"),
    "grid beat negative": (with_grid(beats=[-1.0, 1.0]), "grid.beats[0]:"),
    "grid meter 0": (with_grid(meter=0), "grid.meter:"),
    "grid meter 4.5": (with_grid(meter=4.5), "grid.meter:"),
    "grid first_downbeat negative": (with_grid(first_downbeat=-1), "grid.first_downbeat:"),
    "grid meter missing": (bundle(grid={"beats": [0.5, 1.0], "first_downbeat": 0}), "grid.meter:"),
    "unknown field in a grid": (with_grid(tempo=120), "grid.tempo:"),
}

# The rules a JSON Schema cannot state: the schema accepts the document, the library refuses it.
BEYOND_THE_SCHEMA = {
    "version 1.0 (JSON Schema counts it as the integer 1)": (bundle(version=1.0), "version:"),
    "grid meter 4.0 (JSON Schema counts it as the integer 4)": (with_grid(meter=4.0), "grid.meter:"),
    "events not sorted by t": (bundle(events={"k": [{"t": 1.0}, {"t": 0.5}]}), "events.k[1]:"),
    "an event after the duration": (bundle(duration=1, events={"k": [{"t": 2.0}]}), "events.k[0].t:"),
    "a series starting after the duration": (
        bundle(duration=1, series={"s": {**SERIES, "start": 2}}),
        "series.s.start:",
    ),
    "beats going backwards": (with_grid(beats=[1.0, 0.5]), "grid.beats[1]:"),
    "beats repeating a time": (with_grid(beats=[0.5, 1.0, 1.0]), "grid.beats[2]:"),
    "first_downbeat not below meter": (
        with_grid(beats=[0, 1, 2, 3, 4, 5], meter=2, first_downbeat=2),
        "grid.first_downbeat:",
    ),
    "first_downbeat not below the beat count": (with_grid(beats=[0.5, 1.0], first_downbeat=2), "grid.first_downbeat:"),
    "fraction series without scaling": (with_series(unit="fraction"), "series.s.scaling:"),
    "fraction series with empty scaling": (bundle(series={"s": {**FRACTION, "scaling": ""}}), "series.s.scaling:"),
    "fraction value above 1": (bundle(series={"s": {**FRACTION, "values": [0.0, 1.5]}}), "series.s.values[1]:"),
    "fraction value below 0": (bundle(series={"s": {**FRACTION, "values": [-0.5, 1.0]}}), "series.s.values[0]:"),
    "a number that is not finite": (bundle(duration=float("nan")), "duration:"),
    "a repeated series name": (
        '{"format": "bullterrier-control/bundle", "version": 1, "series": '
        '{"s": {"rate": 1, "values": [1], "unit": "hertz"}, "s": {"rate": 2, "values": [1], "unit": "hertz"}}}',
        "series.s:",
    ),
    "a repeated top-level key": ('{"format": "bullterrier-control/bundle", "version": 1, "version": 1}', "version:"),
}


@pytest.mark.parametrize(("document", "refusal"), STATED_BY_THE_SCHEMA.values(), ids=STATED_BY_THE_SCHEMA)
def test_schema_and_library_agree_on_a_rule_the_schema_states(schema_accepts, document, refusal):
    assert schema_accepts(as_data(document)) is (refusal is None)
    if refusal is None:
        parse(document)
    else:
        with pytest.raises(FormatError) as refused:
            parse(document)
        assert refusal in str(refused.value)


@pytest.mark.parametrize(("document", "refusal"), BEYOND_THE_SCHEMA.values(), ids=BEYOND_THE_SCHEMA)
def test_a_rule_beyond_the_schema_is_enforced_by_the_library_only(schema_accepts, document, refusal):
    assert schema_accepts(as_data(document))
    with pytest.raises(FormatError) as refused:
        parse(document)
    assert refusal in str(refused.value)
