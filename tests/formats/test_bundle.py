import json
from pathlib import Path

import pytest
from hypothesis import given
from hypothesis import strategies as st

from bullterrier_control import Bundle, Event, FormatError, Grid, Series, Unit, load, save

FORMAT = "bullterrier-control/bundle"
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
    "an upper case name": ({**MINIMAL, "series": {"Bass": BASS}}, "Bass"),
    "an empty name": ({**MINIMAL, "series": {"": BASS}}, "series"),
    "a name with a space": ({**MINIMAL, "events": {"hi hat": []}}, "hi hat"),
    "events out of order": ({**MINIMAL, "events": {"kick": [{"t": 1.0}, {"t": 0.5}]}}, "kick"),
    "an event past the end": ({**MINIMAL, "duration": 10, "events": {"kick": [{"t": 10.5}]}}, "kick"),
    "a series starting past the end": ({**MINIMAL, "duration": 10, "series": {"bass": {**BASS, "start": 11}}}, "bass"),
    "a zero duration": ({**MINIMAL, "duration": 0}, "duration"),
    "an invalid part is named": ({**MINIMAL, "series": {"bass": {**BASS, "rate": 0}}}, "series.bass.rate"),
    "an unknown key": ({**MINIMAL, "tempo": 120}, "tempo"),
}


@pytest.mark.parametrize(("data", "named"), INVALID.values(), ids=INVALID.keys())
def test_an_invalid_bundle_is_refused_naming_the_problem(data, named):
    with pytest.raises(FormatError, match=named):
        Bundle.parse(data)


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


@st.composite
def bundles(draw):
    series = draw(
        st.dictionaries(
            names,
            st.builds(
                Series,
                start=times,
                rate=st.floats(min_value=0.01, max_value=1000),
                values=st.lists(st.floats(min_value=-1e6, max_value=1e6), min_size=1, max_size=5).map(tuple),
                unit=st.sampled_from([Unit.DECIBELS, Unit.HERTZ, Unit.MIDI, Unit.RATIO, Unit.SECONDS, Unit.BEATS]),
            ),
            max_size=3,
        )
    )
    events = draw(
        st.dictionaries(
            names,
            st.lists(
                st.builds(
                    Event,
                    t=times,
                    strength=st.floats(min_value=0, max_value=1),
                    duration=st.none() | st.floats(min_value=0.001, max_value=100),
                    label=st.none() | names,
                    pitch=st.none() | st.floats(min_value=0, max_value=127),
                ),
                max_size=4,
            ).map(lambda found: tuple(sorted(found, key=lambda event: event.t))),
            max_size=3,
        )
    )
    grid = draw(
        st.none()
        | st.lists(times, min_size=2, max_size=6, unique=True).map(
            lambda beats: Grid(beats=tuple(sorted(beats)), meter=4, first_downbeat=0)
        )
    )
    return Bundle(format=FORMAT, version=1, grid=grid, series=series, events=events)  # type: ignore[arg-type]


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
    "a nan": (a_bundle_file(f'"a": {a_series_json("NaN")}'), "NaN"),
    "an infinity": (a_bundle_file(f'"a": {a_series_json("Infinity")}'), "Infinity"),
    "a repeated series name": (
        a_bundle_file(f'"a": {a_series_json("1")}, "a": {a_series_json("1", rate=2)}'),
        "repeated",
    ),
    "text that is not json": ("not json", "JSON"),
    "an array at the top": ("[1, 2]", "object"),
    "an empty file": ("", "JSON"),
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


def test_save_overwrites_and_load_reads_a_path_given_as_str(tmp_path):
    path = tmp_path / "bundle.json"
    save(Bundle.parse({**MINIMAL, "source": "first"}), path)
    save(Bundle.parse({**MINIMAL, "source": "second"}), path)
    assert load(Path(str(path))).source == "second"
