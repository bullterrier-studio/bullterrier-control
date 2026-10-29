import csv
import io
import json
import os
import re
import subprocess
import sys
import tomllib
from pathlib import Path

import jsonschema
import pytest

from bullterrier_control import Bundle, Series, Unit, load, load_patch, registry, save
from bullterrier_control.cli import main
from bullterrier_control.patch import patch_schema

ROOT = Path(__file__).resolve().parents[1]


def version_in_pyproject() -> str:
    return tomllib.loads((ROOT / "pyproject.toml").read_text(encoding="utf-8"))["project"]["version"]


def test_version_prints_the_program_name_and_the_released_version(capsys):
    with pytest.raises(SystemExit) as stop:
        main(["--version"])
    assert stop.value.code == 0
    assert capsys.readouterr().out.strip() == f"bt-control {version_in_pyproject()}"


def test_help_names_the_program_and_lists_the_exit_codes(capsys):
    with pytest.raises(SystemExit) as stop:
        main(["--help"])
    out = capsys.readouterr().out
    assert stop.value.code == 0
    assert "bt-control" in out
    assert re.search(r"exit codes:\n\s+0\s+success", out)
    assert re.search(r"\n\s+1\s+a file is not a valid bundle or patch", out)
    assert "validate" in out
    assert "schema" in out
    assert "list" in out
    assert "sample" in out


def test_no_arguments_print_the_help_and_succeed(capsys):
    assert main([]) == 0
    assert "usage: bt-control" in capsys.readouterr().out


def test_an_unknown_option_is_a_usage_error_on_stderr(capsys):
    with pytest.raises(SystemExit) as stop:
        main(["--no-such-option"])
    captured = capsys.readouterr()
    assert stop.value.code == 2
    assert "--no-such-option" in captured.err
    assert captured.out == ""


SCHEMA_FILE = ROOT / "schemas" / "bundle-1.schema.json"
BUNDLE = {"format": "bullterrier-control/bundle", "version": 1}


def a_valid_bundle(folder: Path, name: str = "good.json") -> Path:
    path = folder / name
    save(
        Bundle.parse({**BUNDLE, "series": {"bass": Series(rate=10, values=(0.5,), unit=Unit.RATIO).to_dict()}}),
        path,
    )
    return path


def test_validate_of_valid_files_succeeds_and_says_so(tmp_path, capsys):
    good = a_valid_bundle(tmp_path)
    assert main(["validate", str(good)]) == 0
    assert capsys.readouterr().out.strip() == f"{good}: ok"


def test_validate_reports_every_file_and_fails_when_one_is_invalid(tmp_path, capsys):
    good = a_valid_bundle(tmp_path)
    bad = tmp_path / "bad.json"
    bad.write_text(
        json.dumps({**BUNDLE, "series": {"bass": {"rate": 0, "values": [1], "unit": "ratio"}}}), encoding="utf-8"
    )
    assert main(["validate", str(bad), str(good)]) == 1  # the invalid one first: the valid one is still checked
    captured = capsys.readouterr()
    assert f"{good}: ok" in captured.out
    assert captured.err.startswith("error: ")
    assert "bad.json" in captured.err
    assert "series.bass.rate" in captured.err


def test_validate_of_a_missing_file_is_an_error_not_a_traceback(tmp_path, capsys):
    assert main(["validate", str(tmp_path / "absent.json")]) == 1
    assert capsys.readouterr().err.startswith("error: ")


def test_validate_needs_a_file(capsys):
    with pytest.raises(SystemExit) as stop:
        main(["validate"])
    assert stop.value.code == 2
    assert "validate" in capsys.readouterr().err


def test_the_committed_schema_accepts_a_valid_bundle_and_rejects_a_misshapen_one(tmp_path):
    schema = json.loads(SCHEMA_FILE.read_text(encoding="utf-8"))
    jsonschema.validate(json.loads(a_valid_bundle(tmp_path).read_text(encoding="utf-8")), schema)
    for misshapen in (
        {**BUNDLE, "version": 2},
        {**BUNDLE, "series": {"bass": {"rate": 0, "values": [1], "unit": "ratio"}}},
        {**BUNDLE, "series": {"bass": {"rate": 1, "values": [1], "unit": "percent"}}},
        {**BUNDLE, "events": {"kick": [{"t": -1}]}},
        {**BUNDLE, "tempo": 120},
    ):
        with pytest.raises(jsonschema.ValidationError):
            jsonschema.validate(misshapen, schema)


def test_list_gives_a_line_per_registered_thing_with_its_kind_and_summary(capsys):
    assert main(["list"]) == 0
    lines = capsys.readouterr().out.splitlines()
    assert [line.split()[0] for line in lines] == sorted(registry.all_registered())
    by_name = {line.split()[0]: line for line in lines}
    assert by_name["slew"].split()[1] == "shaper"
    assert by_name["select"].split()[1] == "operation"
    assert by_name["lfo"].split()[1] == "source"
    assert registry.get("slew").summary in by_name["slew"]


def test_list_with_names_shows_the_help_of_each(capsys):
    assert main(["list", "slew", "select"]) == 0
    out = capsys.readouterr().out
    assert "shaper slew(signal, **values) -> signal" in out
    assert "operation select(*, events, **values) -> events" in out
    assert "rise (seconds" in out


def test_list_of_an_unknown_name_is_an_error_naming_it_and_what_exists(capsys):
    assert main(["list", "slew", "nosuch"]) == 1
    captured = capsys.readouterr()
    assert captured.err.startswith("error: ")
    assert "nosuch" in captured.err
    assert "slew" in captured.err  # the available names are listed
    assert "Traceback" not in captured.err


def test_list_json_describes_every_registered_thing_for_another_tool(capsys):
    assert main(["list", "--json"]) == 0
    listing = json.loads(capsys.readouterr().out)
    assert list(listing) == sorted(registry.all_registered())
    assert listing["envelope"]["inputs"]["events"]["required"] is True
    assert listing["select"]["returns"] == "events"
    assert listing["lfo"]["parameters"]["properties"]["phase"]["cyclic"] is True
    assert listing["slew"]["kind"] == "shaper"
    assert listing["slew"]["parameters"]["properties"]["rise"]["unit"] == "seconds"


def test_list_json_with_names_describes_only_those(capsys):
    assert main(["list", "--json", "lfo", "clip"]) == 0
    assert sorted(json.loads(capsys.readouterr().out)) == ["clip", "lfo"]


def test_list_json_of_an_unknown_name_prints_nothing_on_stdout_and_fails(capsys):
    assert main(["list", "--json", "nosuch"]) == 1
    captured = capsys.readouterr()
    assert captured.out == ""
    assert "nosuch" in captured.err


# ---------------------------------------------------------------- sample

GLOW = ROOT / "examples" / "glow.toml"
KICKS = ROOT / "examples" / "kicks.json"
# kicks at 0.5, 1.0 (0.6: below the patch's threshold) and 1.5 s; a flare of time constant 0.25 s on each strong one
E = [1.0, 0.36787944117144233, 0.1353352832366127, 0.049787068367863944]  # e^0, e^-1, e^-2, e^-3


def a_patch(folder: Path, text: str, name: str = "look.toml") -> Path:
    path = folder / name
    path.write_text(text, encoding="utf-8")
    return path


LEVELS = """\
format = "bullterrier-control/patch"
version = 1

[low]
use = "constant"
value = 0.25

[high]
use = "constant"
value = 0.75
"""


def rows_of(output: str) -> list[list[str]]:
    return list(csv.reader(io.StringIO(output)))


def test_sample_csv_gives_a_row_per_time_with_the_signal_nodes_as_columns(capsys):
    argv = ["sample", str(GLOW), "--bundle", str(KICKS), "--to", "2", "--rate", "4", "--csv"]
    assert main(argv) == 0
    rows = rows_of(capsys.readouterr().out)
    assert rows[0] == ["t", "glow"]  # the node that gives events is not a column
    assert [float(row[0]) for row in rows[1:]] == [0.0, 0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0]
    # nothing before the first kick; 1 at it, then e^-1, e^-2, e^-3 a quarter second apart (the 0.6 kick is not strong
    # enough); the third kick at 1.5 s beats the first one's tail
    expected = [0.0, 0.0, E[0], E[1], E[2], E[3], E[0], E[1], E[2]]
    assert [float(row[1]) for row in rows[1:]] == pytest.approx(expected, abs=1e-9)


def test_sample_json_is_a_bundle_with_a_series_per_node_that_starts_where_the_range_does(capsys):
    argv = ["sample", str(GLOW), "--bundle", str(KICKS), "--from", "0.5", "--to", "1.5", "--rate", "4"]
    assert main(argv) == 0
    bundle = Bundle.parse(json.loads(capsys.readouterr().out))
    series = bundle.series["glow"]
    assert (series.start, series.rate, series.unit) == (0.5, 4, Unit.RATIO)
    assert list(series.values) == pytest.approx([E[0], E[1], E[2], E[3], E[0]], abs=1e-9)
    assert "glow.toml" in (bundle.source or "")
    assert str(ROOT) not in (bundle.source or "")  # the name of the patch, not a path on this machine


TIMES = {
    "ten samples a second for one second": (["--to", "1", "--rate", "10"], 11, 1.0),
    "a last time that float rounding puts a hair short": (["--to", "0.29", "--rate", "100"], 30, 0.29),
    "a range that starts later": (["--from", "0.5", "--to", "1", "--rate", "2"], 2, 1.0),
    "a last time between two samples is left out": (["--to", "1.05", "--rate", "10"], 11, 1.0),
    "an empty range is one sample": (["--from", "2", "--to", "2", "--rate", "5"], 1, 2.0),
}


@pytest.mark.parametrize(("flags", "count", "last"), TIMES.values(), ids=TIMES.keys())
def test_sample_reads_every_sample_time_from_the_start_up_to_and_including_the_end(
    flags, count, last, tmp_path, capsys
):
    patch = a_patch(tmp_path, LEVELS)
    assert main(["sample", str(patch), "low", "--csv", *flags]) == 0
    times = [float(row[0]) for row in rows_of(capsys.readouterr().out)[1:]]
    assert len(times) == count
    assert times[-1] == pytest.approx(last, abs=1e-12)


def test_sample_gives_each_chosen_node_in_the_order_asked(tmp_path, capsys):
    patch = a_patch(tmp_path, LEVELS)
    assert main(["sample", str(patch), "high", "low", "--to", "0.5", "--rate", "2", "--csv"]) == 0
    assert rows_of(capsys.readouterr().out) == [
        ["t", "high", "low"],
        ["0.0", "0.75", "0.25"],
        ["0.5", "0.75", "0.25"],
    ]
    assert main(["sample", str(patch), "--to", "0.5", "--rate", "2", "--csv"]) == 0
    assert rows_of(capsys.readouterr().out)[0] == ["t", "low", "high"]  # every signal node, in file order


NODE_ERRORS = {
    "a node that gives events": (["strong"], "strong", "events"),
    "a node that is not in the patch": (["nosuch"], "nosuch", "glow"),
}


@pytest.mark.parametrize(("nodes", "named", "also"), NODE_ERRORS.values(), ids=NODE_ERRORS.keys())
def test_sample_of_a_node_that_cannot_be_sampled_is_an_error_naming_it(nodes, named, also, capsys):
    argv = ["sample", str(GLOW), *nodes, "--bundle", str(KICKS), "--to", "1", "--rate", "4"]
    assert main(argv) == 1
    captured = capsys.readouterr()
    assert captured.out == ""
    assert captured.err.startswith("error: ")
    assert named in captured.err
    assert also in captured.err  # what is wrong, or what the patch does have


def test_sample_of_a_patch_that_needs_a_bundle_without_one_names_the_node_and_key(capsys):
    assert main(["sample", str(GLOW), "--to", "1", "--rate", "4"]) == 1
    captured = capsys.readouterr()
    assert captured.err.startswith("error: ")
    assert "strong.events" in captured.err
    assert "bundle" in captured.err


def test_sample_of_a_broken_patch_is_an_error_naming_the_file_and_the_key(tmp_path, capsys):
    patch = a_patch(tmp_path, GLOW.read_text(encoding="utf-8").replace("0.25", "-1"))
    assert main(["sample", str(patch), "--bundle", str(KICKS), "--to", "1", "--rate", "4"]) == 1
    err = capsys.readouterr().err
    assert str(patch) in err
    assert "glow.decay" in err
    assert "Traceback" not in err


USAGE = {
    "an end before the start": ["--from", "2", "--to", "1", "--rate", "4"],
    "a rate of zero": ["--to", "1", "--rate", "0"],
    "a negative rate": ["--to", "1", "--rate", "-4"],
    "a start before time 0": ["--from", "-1", "--to", "1", "--rate", "4"],
    "an end that is not a number": ["--to", "nan", "--rate", "4"],
    "an end that is infinite": ["--to", "inf", "--rate", "4"],
    "no end": ["--rate", "4"],
    "no rate": ["--to", "1"],
    "more samples than a terminal could show": ["--to", "1e12", "--rate", "1000"],
}


@pytest.mark.parametrize("flags", USAGE.values(), ids=USAGE.keys())
def test_sample_with_a_range_that_makes_no_sense_is_a_usage_error(flags, tmp_path, capsys):
    patch = a_patch(tmp_path, LEVELS)
    with pytest.raises(SystemExit) as stop:
        main(["sample", str(patch), *flags])
    assert stop.value.code == 2
    captured = capsys.readouterr()
    assert captured.out == ""
    assert "usage:" in captured.err


# ---------------------------------------------------------------- patches in validate and schema


def test_validate_checks_a_toml_file_as_a_patch_and_any_other_as_a_bundle(tmp_path, capsys):
    bad = a_patch(tmp_path, GLOW.read_text(encoding="utf-8").replace("0.25", "-1"), "bad.toml")
    assert main(["validate", str(GLOW), str(KICKS), str(bad)]) == 1
    captured = capsys.readouterr()
    assert f"{GLOW}: ok" in captured.out
    assert f"{KICKS}: ok" in captured.out
    assert "bad.toml" in captured.err
    assert "glow.decay" in captured.err


def test_schema_prints_the_bundle_schema_by_default_and_the_patch_schema_when_asked(capsys):
    assert main(["schema"]) == 0
    assert json.loads(capsys.readouterr().out)["title"] == "Bundle"
    assert main(["schema", "bundle"]) == 0
    assert json.loads(capsys.readouterr().out)["title"] == "Bundle"
    assert main(["schema", "patch"]) == 0
    assert json.loads(capsys.readouterr().out) == json.loads(json.dumps(patch_schema()))


def test_schema_of_something_else_is_a_usage_error(capsys):
    with pytest.raises(SystemExit) as stop:
        main(["schema", "recipe"])
    assert stop.value.code == 2
    assert "recipe" in capsys.readouterr().err


PATCH_SCHEMA_FILE = ROOT / "schemas" / "patch-1.schema.json"


def test_the_committed_patch_schema_accepts_a_valid_patch_and_rejects_a_misshapen_one():
    schema = json.loads(PATCH_SCHEMA_FILE.read_text(encoding="utf-8"))
    patch = tomllib.loads(GLOW.read_text(encoding="utf-8"))
    jsonschema.validate(patch, schema)
    for misshapen in (
        {**patch, "version": 2},
        {**patch, "Loud": {"use": "constant"}},
        {**patch, "level": {"value": 1}},
        {key: value for key, value in patch.items() if key != "format"},
    ):
        with pytest.raises(jsonschema.ValidationError):
            jsonschema.validate(misshapen, schema)


def test_the_example_patch_is_valid_and_builds_against_the_example_bundle():
    built = load_patch(GLOW).build(load(KICKS))
    assert built["glow"].sample(0.5) == 1.0


# ---------------------------------------------------------------- plot


def test_plot_writes_a_png_of_the_signal_nodes(tmp_path, capsys):
    out = tmp_path / "glow.png"
    assert main(["plot", str(GLOW), "--bundle", str(KICKS), "--to", "2", "--out", str(out)]) == 0
    assert out.read_bytes()[:8] == b"\x89PNG\r\n\x1a\n"
    assert capsys.readouterr().out == ""  # the picture is the result; nothing is said about it


def test_plot_of_a_node_that_cannot_be_drawn_is_an_error_and_writes_nothing(tmp_path, capsys):
    out = tmp_path / "strong.png"
    assert main(["plot", str(GLOW), "strong", "--bundle", str(KICKS), "--to", "2", "--out", str(out)]) == 1
    assert "strong: gives events" in capsys.readouterr().err
    assert not out.exists()


PLOT_USAGE = {
    "no output file": ["--to", "2"],
    "no end": ["--out", "x.png"],
    "an empty range": ["--from", "2", "--to", "2", "--out", "x.png"],
    "an end before the start": ["--from", "3", "--to", "2", "--out", "x.png"],
}


@pytest.mark.parametrize("flags", PLOT_USAGE.values(), ids=PLOT_USAGE.keys())
def test_plot_with_a_range_or_output_that_makes_no_sense_is_a_usage_error(flags, tmp_path, capsys, monkeypatch):
    monkeypatch.chdir(tmp_path)
    patch = a_patch(tmp_path, LEVELS)
    with pytest.raises(SystemExit) as stop:
        main(["plot", str(patch), *flags])
    assert stop.value.code == 2
    assert "usage:" in capsys.readouterr().err
    assert not (tmp_path / "x.png").exists()


def test_plot_without_matplotlib_is_an_error_saying_what_to_install(tmp_path, capsys, without_matplotlib):
    out = tmp_path / "glow.png"
    assert main(["plot", str(GLOW), "--bundle", str(KICKS), "--to", "2", "--out", str(out)]) == 1
    err = capsys.readouterr().err
    assert err.startswith("error: ")
    assert "bullterrier-control[plot]" in err
    assert "Traceback" not in err


# ------------------------------------------------------------ only what is asked for, and what is said when nothing is


MIXED = GLOW.read_text(encoding="utf-8") + '\n[wave]\nuse = "lfo"\n'


def test_sample_of_a_node_that_needs_no_bundle_works_without_one_whatever_else_the_patch_holds(tmp_path, capsys):
    patch = a_patch(tmp_path, MIXED)
    assert main(["sample", str(patch), "wave", "--to", "0.25", "--rate", "4", "--csv"]) == 0
    rows = rows_of(capsys.readouterr().out)
    assert rows[0] == ["t", "wave"]
    assert float(rows[-1][1]) == pytest.approx(1.0, abs=1e-9)  # a quarter of a cycle in: the top of the sine


def test_plot_of_a_node_that_needs_no_bundle_works_without_one(tmp_path):
    out = tmp_path / "wave.png"
    assert main(["plot", str(a_patch(tmp_path, MIXED)), "wave", "--to", "2", "--out", str(out)]) == 0
    assert out.read_bytes()[:8] == b"\x89PNG\r\n\x1a\n"


EVENTS_ONLY = 'format = "bullterrier-control/patch"\nversion = 1\n[strong]\nuse = "select"\nevents = [{ t = 1 }]\n'


@pytest.mark.parametrize("command", ["sample", "plot"])
def test_a_patch_with_no_signal_to_read_is_an_error_not_an_empty_result(command, tmp_path, capsys):
    patch = a_patch(tmp_path, EVENTS_ONLY)
    out = tmp_path / "empty.png"
    flags = ["--rate", "2"] if command == "sample" else ["--out", str(out)]
    assert main([command, str(patch), "--to", "1", *flags]) == 1
    captured = capsys.readouterr()
    assert captured.out == ""
    assert "no signal" in captured.err
    assert not out.exists()


def test_a_range_that_is_not_a_number_is_a_usage_error_that_does_not_show_a_private_name(tmp_path, capsys):
    with pytest.raises(SystemExit) as stop:
        main(["sample", str(a_patch(tmp_path, LEVELS)), "--to", "1", "--rate", "abc"])
    assert stop.value.code == 2
    err = capsys.readouterr().err
    assert "abc" in err
    assert "_hertz" not in err


def test_sample_writes_utf8_whatever_the_console_encoding_is(tmp_path):
    patch = a_patch(tmp_path, LEVELS, "é ü.toml")
    run = subprocess.run(
        [sys.executable, "-m", "bullterrier_control", "sample", str(patch), "--to", "1", "--rate", "2"],
        env={**os.environ, "PYTHONIOENCODING": "ascii"},
        capture_output=True,
        check=False,
    )
    assert run.returncode == 0, run.stderr.decode("utf-8", errors="replace")
    bundle = Bundle.parse(json.loads(run.stdout.decode("utf-8")))
    assert "é ü.toml" in (bundle.source or "")
