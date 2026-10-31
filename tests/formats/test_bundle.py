import json
import pickle
from typing import Final

import jsonschema
import pytest
from hypothesis import given
from hypothesis import strategies as st

from bullterrier_control import Bundle, Event, FormatError, Grid, Series, Unit, load, save
from bullterrier_control.formats import bundle_schema

FORMAT: Final = "bullterrier-control/bundle"
MINIMAL = {"format": FORMAT, "version": 1}
BASS = {"rate": 100, "values": [0.0, 0.5, 1.0], "unit": "fraction", "scaling": "peak"}


def test_the_smallest_bundle_is_format_and_version():
    bundle = Bundle.parse(MINIMAL)
    assert dict(bundle.series) == {}
    assert dict(bundle.events) == {}
    assert bundle.grid is None
    assert bundle.duration is None
    assert bundle.source is None


def test_a_full_bundle_keeps_its_parts_as_formats():
    bundle = Bundle.parse(
        {
            **MINIMAL,
            "source": "test 1.0 song.wav",
            "duration": 10,
            "grid": {"beats": [0.5, 1.0], "meter": 4, "first_downbeat": 0},
            "series": {"bass": BASS},
            "events": {"kick": [{"t": 0.5}, {"t": 1.0, "strength": 0.5}]},
        }
    )
    assert bundle.duration == 10.0
    assert isinstance(bundle.grid, Grid)
    assert isinstance(bundle.series["bass"], Series)
    assert bundle.series["bass"].unit is Unit.FRACTION
    assert [event.t for event in bundle.events["kick"]] == [0.5, 1.0]
    assert all(isinstance(event, Event) for event in bundle.events["kick"])


INVALID = {
    "an unknown version": ({"format": FORMAT, "version": 2}, "version"),
    "another format": ({"format": "other", "version": 1}, "format"),
    "a missing version": ({"format": FORMAT}, "version"),
    "a version that is a bool": ({"format": FORMAT, "version": True}, "version"),
    "a version that is a float": ({"format": FORMAT, "version": 1.0}, "version"),
    "a version that is a string": ({"format": FORMAT, "version": "1"}, "version"),
    "an upper case name": ({**MINIMAL, "series": {"Bass": BASS}}, "series.Bass"),
    "an empty name": ({**MINIMAL, "series": {"": BASS}}, "series"),
    "a name with a space": ({**MINIMAL, "events": {"hi hat": []}}, "events.hi hat"),
    "events out of order": ({**MINIMAL, "events": {"kick": [{"t": 1.0}, {"t": 0.5}]}}, "events.kick[1]"),
    "an event past the end": ({**MINIMAL, "duration": 10, "events": {"kick": [{"t": 10.5}]}}, "events.kick"),
    "a series starting past the end": (
        {**MINIMAL, "duration": 10, "series": {"bass": {**BASS, "start": 11}}},
        "series.bass",
    ),
    "a zero duration": ({**MINIMAL, "duration": 0}, "duration"),
    "an invalid part is named": ({**MINIMAL, "series": {"bass": {**BASS, "rate": 0}}}, "series.bass.rate"),
    "an unknown key": ({**MINIMAL, "tempo": 120}, "tempo"),
}


@pytest.mark.parametrize(("data", "named"), INVALID.values(), ids=INVALID.keys())
def test_an_invalid_bundle_is_refused_naming_the_problem(data, named, reports):
    with pytest.raises(FormatError) as refused:
        Bundle.parse(data)
    reports(refused.value, named)


def test_an_unknown_version_names_the_supported_one():
    with pytest.raises(FormatError, match="1"):
        Bundle.parse({"format": FORMAT, "version": 2})


EDGES = {
    "an event exactly at the end": {**MINIMAL, "duration": 10, "events": {"kick": [{"t": 10}]}},
    "two events at the same time": {**MINIMAL, "events": {"kick": [{"t": 1}, {"t": 1}]}},
    "a series starting exactly at the end": {**MINIMAL, "duration": 10, "series": {"bass": {**BASS, "start": 10}}},
    "an empty event list": {**MINIMAL, "events": {"kick": []}},
    "names with dots, dashes and digits": {**MINIMAL, "series": {"stem.drums-2": BASS}},
}


@pytest.mark.parametrize("data", EDGES.values(), ids=EDGES.keys())
def test_the_edges_are_valid(data):
    Bundle.parse(data)


names = st.from_regex(r"[a-z0-9][a-z0-9_.-]{0,8}", fullmatch=True)
times = st.floats(min_value=0, max_value=1000)


unscaled_series = st.builds(
    Series,
    start=times,
    rate=st.floats(min_value=0.01, max_value=1000),
    values=st.lists(st.floats(min_value=-1e6, max_value=1e6), min_size=1, max_size=5).map(tuple),
    unit=st.sampled_from([Unit.DECIBELS, Unit.HERTZ, Unit.MIDI, Unit.RATIO, Unit.SECONDS, Unit.BEATS]),
)
fraction_series = st.builds(
    Series,
    start=times,
    rate=st.floats(min_value=0.01, max_value=1000),
    values=st.lists(st.floats(min_value=0, max_value=1), min_size=1, max_size=5).map(tuple),
    unit=st.just(Unit.FRACTION),
    scaling=st.text(min_size=1, max_size=20),  # any text, accents and emoji included
)
events_of_one_kind = st.lists(
    st.builds(
        Event,
        t=times,
        strength=st.floats(min_value=0, max_value=1),
        duration=st.none() | st.floats(min_value=0.001, max_value=100),
        label=st.none() | st.text(min_size=1, max_size=10),
        pitch=st.none() | st.floats(min_value=0, max_value=127),
    ),
    max_size=4,
).map(lambda found: tuple(sorted(found, key=lambda event: event.t)))


@st.composite
def grids(draw):
    beats = tuple(sorted(draw(st.lists(times, min_size=2, max_size=6, unique=True))))
    meter = draw(st.integers(min_value=1, max_value=6))
    return Grid(beats=beats, meter=meter, first_downbeat=draw(st.integers(0, min(meter, len(beats)) - 1)))


@st.composite
def bundles(draw):
    return Bundle(
        format=FORMAT,
        version=1,
        source=draw(st.none() | st.text(max_size=20)),
        duration=draw(st.none() | st.just(2000.0)),  # past every time the strategies produce
        grid=draw(st.none() | grids()),
        series=draw(st.dictionaries(names, unscaled_series | fraction_series, max_size=3)),
        events=draw(st.dictionaries(names, events_of_one_kind, max_size=3)),
    )


@given(bundle=bundles())
def test_saving_and_loading_gives_the_same_bundle(bundle, tmp_path_factory):
    path = tmp_path_factory.mktemp("roundtrip") / "bundle.json"
    save(bundle, path)
    assert load(path) == bundle


def test_a_saved_file_is_utf8_json_ending_in_a_newline(tmp_path):
    path = tmp_path / "bundle.json"
    save(Bundle.parse({**MINIMAL, "source": "café", "series": {"bass": BASS}}), path)
    raw = path.read_bytes()
    assert raw.endswith(b"\n")
    assert json.loads(raw.decode("utf-8"))["format"] == FORMAT
    assert "café" in raw.decode("utf-8")  # written as text, not as an escape


def test_a_bundle_without_optional_parts_writes_no_nulls(tmp_path):
    path = tmp_path / "bundle.json"
    save(Bundle.parse(MINIMAL), path)
    assert "null" not in path.read_text(encoding="utf-8")


def a_bundle_file(series: str) -> str:
    return f'{{"format": "{FORMAT}", "version": 1, "series": {{{series}}}}}'


def a_series_json(values: str, rate: int = 1) -> str:
    return f'{{"rate": {rate}, "values": [{values}], "unit": "ratio"}}'


FILES = {
    "a nan": (a_bundle_file(f'"a": {a_series_json("NaN")}'), r"series\.a\.values\[0\]: expected value"),
    "an infinity": (a_bundle_file(f'"a": {a_series_json("Infinity")}'), r"series\.a\.values\[0\]: expected value"),
    "a repeated series name": (
        a_bundle_file(f'"a": {a_series_json("1")}, "a": {a_series_json("1", rate=2)}'),
        "series.a: duplicate field `a`",
    ),
    "text that is not json": ("not json", "line 1 column 2"),
    "an array at the top": ("[1, 2]", "expected an object"),
    "an empty file": ("", "EOF while parsing"),
    "text after the object": (a_bundle_file("") + " x", "trailing characters"),
}


@pytest.mark.parametrize(("text", "named"), FILES.values(), ids=FILES.keys())
def test_a_file_that_cannot_be_a_bundle_is_refused_naming_the_file(text, named, tmp_path):
    path = tmp_path / "broken.json"
    path.write_text(text, encoding="utf-8")
    with pytest.raises(FormatError, match=named) as refused:
        load(path)
    assert "broken.json" in str(refused.value)


def test_a_missing_file_is_the_os_error_not_a_format_error(tmp_path):
    with pytest.raises(FileNotFoundError):
        load(tmp_path / "absent.json")


def test_a_file_that_is_not_utf8_is_refused_naming_the_file(tmp_path):
    path = tmp_path / "latin.json"
    path.write_bytes('{"source": "caf\xe9"}'.encode("latin-1"))
    with pytest.raises(FormatError, match=r"latin\.json"):
        load(path)


def test_save_replaces_an_existing_file(tmp_path):
    path = tmp_path / "bundle.json"
    save(Bundle.parse({**MINIMAL, "source": "first"}), path)
    save(Bundle.parse({**MINIMAL, "source": "second"}), path)
    assert load(path).source == "second"


HOSTILE = {
    "a deeply nested array": "[" * 100_000,
    "an integer with thousands of digits": f'{{"format": "{FORMAT}", "version": 1, "duration": {"1" * 5000}}}',
}


@pytest.mark.parametrize("text", HOSTILE.values(), ids=HOSTILE.keys())
def test_a_hostile_file_is_a_format_error_naming_the_file_not_a_crash(text, tmp_path):
    path = tmp_path / "hostile.json"
    path.write_text(text, encoding="utf-8")
    with pytest.raises(FormatError, match=r"hostile\.json"):
        load(path)


TEXT_FIELDS = {
    "source": {**MINIMAL, "source": "\ud800"},
    "an event label": {**MINIMAL, "events": {"kick": [{"t": 1, "label": "\ud800"}]}},
    "a series scaling": {
        **MINIMAL,
        "series": {"a": {"rate": 1, "values": [0.5], "unit": "fraction", "scaling": "\ud800"}},
    },
}


@pytest.mark.parametrize("data", TEXT_FIELDS.values(), ids=TEXT_FIELDS.keys())
def test_text_that_cannot_be_written_as_utf8_is_refused_on_the_way_in(data):
    with pytest.raises(FormatError, match="UTF-8"):
        Bundle.parse(data)


def test_a_bundle_cannot_be_changed_through_its_mappings():
    bundle = Bundle.parse({**MINIMAL, "series": {"bass": BASS}, "events": {"kick": [{"t": 1}]}})
    with pytest.raises(TypeError):
        bundle.series["other"] = bundle.series["bass"]  # type: ignore[index]
    with pytest.raises(TypeError):
        bundle.events["other"] = ()  # type: ignore[index]


def test_a_bundle_built_directly_is_checked_like_a_parsed_one():
    with pytest.raises(FormatError, match="version"):
        Bundle(format=FORMAT, version=2)


def test_the_schema_refuses_names_the_format_refuses():
    schema = bundle_schema()
    for field in ("series", "events"):
        jsonschema.validate({**MINIMAL, field: {"bass": BASS if field == "series" else []}}, schema)
        with pytest.raises(jsonschema.ValidationError):
            jsonschema.validate({**MINIMAL, field: {"Bass": BASS if field == "series" else []}}, schema)


NESTED_PATHS = {
    "a field of a series": ({**MINIMAL, "series": {"bass": {**BASS, "rate": 0}}}, "Bundle: series.bass.rate: "),
    "a rule across a series' fields": (
        {**MINIMAL, "series": {"bass": {**BASS, "values": [0.5, 1.5]}}},
        'Bundle: series.bass.values[1]: must be in [0, 1] when unit is "fraction" (got 1.5)',
    ),
    "a field of an event": ({**MINIMAL, "events": {"kick": [{"t": 1}, {"t": -1}]}}, "Bundle: events.kick[1].t: "),
    "a field of the grid": (
        {**MINIMAL, "grid": {"beats": [0, 1], "meter": 0, "first_downbeat": 0}},
        "Bundle: grid.meter: ",
    ),
    "a rule across the grid's fields": (
        {**MINIMAL, "grid": {"beats": [0, 1, 0.5], "meter": 4, "first_downbeat": 0}},
        "Bundle: grid.beats[2]: 0.5 does not come after beats[1] = 1.0",
    ),
}


@pytest.mark.parametrize(("data", "start"), NESTED_PATHS.values(), ids=NESTED_PATHS.keys())
def test_a_problem_inside_a_part_is_reported_under_the_full_path_of_that_part(data, start):
    with pytest.raises(FormatError) as refused:
        Bundle.parse(data)
    assert str(refused.value).startswith(start)


KICK = Event(0.5)
SERIES = Series.parse(BASS)

BUILT_REFUSED = {
    "a grid that is a mapping": ({"grid": {"beats": [0, 1], "meter": 1, "first_downbeat": 0}}, "grid"),
    "series that are not a mapping": ({"series": [SERIES]}, "series"),
    "a series that is a mapping": ({"series": {"bass": BASS}}, "series.bass"),
    "a series name that is not text": ({"series": {1: SERIES}}, "series"),
    "events that are one event": ({"events": {"kick": KICK}}, "events.kick"),
    "an event that is a mapping": ({"events": {"kick": [KICK, {"t": 1}]}}, "events.kick[1]"),
    "events out of order": ({"events": {"kick": (Event(1), KICK)}}, "events.kick[1]"),
    "an upper case name": ({"series": {"Bass": SERIES}}, "series.Bass"),
    "an event past the end": ({"duration": 0.25, "events": {"kick": [KICK]}}, "events.kick[0].t"),
}


@pytest.mark.parametrize(("parts", "named"), BUILT_REFUSED.values(), ids=BUILT_REFUSED.keys())
def test_a_bundle_built_from_parts_that_are_not_formats_or_break_a_rule_is_refused(parts, named, reports):
    with pytest.raises(FormatError) as refused:
        Bundle(format=FORMAT, version=1, **parts)
    reports(refused.value, named)


def test_a_bundle_built_from_parts_lists_every_problem_of_the_document_and_the_parts():
    with pytest.raises(FormatError) as refused:
        Bundle(format=FORMAT, version=2, grid={}, series={"Bass": SERIES}, events={"kick": [KICK, 1]})  # type: ignore[arg-type, list-item]  # wrong on purpose: refused at run time
    assert [path for path, _ in refused.value.problems] == ["version", "grid", "events.kick[1]", "series.Bass"]


def test_a_bundle_built_from_parts_keeps_those_parts_by_name_and_events_as_tuples():
    bundle = Bundle(format=FORMAT, version=1, series={"bass": SERIES}, events={"snare": [KICK], "kick": [KICK]})
    assert bundle.series["bass"] is SERIES
    assert list(bundle.events) == ["kick", "snare"]
    assert bundle.events["kick"] == (KICK,)
    assert type(bundle.events["kick"]) is tuple


def test_to_dict_is_the_bundle_as_a_file_holds_it_and_parses_back():
    bundle = Bundle(format=FORMAT, version=1, duration=2, series={"bass": SERIES}, events={"kick": [KICK]})
    assert bundle.to_dict() == {
        "format": FORMAT,
        "version": 1,
        "duration": 2.0,
        "series": {
            "bass": {"start": 0.0, "rate": 100.0, "values": [0.0, 0.5, 1.0], "unit": "fraction", "scaling": "peak"}
        },
        "events": {"kick": [{"t": 0.5, "strength": 1.0}]},
    }
    assert Bundle.parse(bundle.to_dict()) == bundle


def test_from_json_reads_the_text_a_file_holds():
    bundle = Bundle.from_json(f'{{"format": "{FORMAT}", "version": 1, "events": {{"kick": [{{"t": 0.5}}]}}}}')
    assert bundle.events["kick"] == (KICK,)


@pytest.mark.parametrize(
    "made",
    [
        pytest.param(lambda: Bundle(format=FORMAT, version=1), id="built"),
        pytest.param(lambda: Bundle.parse(MINIMAL), id="parsed"),
        pytest.param(lambda: Bundle.from_json(json.dumps(MINIMAL)), id="read"),
        pytest.param(lambda: pickle.loads(pickle.dumps(Bundle.parse(MINIMAL))), id="unpickled"),  # noqa: S301
    ],
)
def test_a_bundle_states_its_format_and_version(made):
    bundle = made()
    assert (bundle.format, bundle.version) == ("bullterrier-control/bundle", 1)


def test_a_bundle_of_many_parts_that_are_not_events_gives_a_short_list_that_still_counts_them_all():
    with pytest.raises(FormatError) as refused:
        Bundle(format=FORMAT, version=1, events={"kick": [None] * 300_000})  # type: ignore[list-item]
    assert len(refused.value.problems) <= 100
    assert refused.value.problems[0][0] == "events.kick[0]"
    # 20 are shown, so 299_980 of the 300_000 are left out
    assert str(refused.value).endswith("; ... and 299980 more")
