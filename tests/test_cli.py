import json
import re
import tomllib
from pathlib import Path

import jsonschema
import pytest

from bullterrier_control import Bundle, Series, Unit, registry, save
from bullterrier_control.cli import main

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
    assert re.search(r"\n\s+1\s+a file is not a valid bundle", out)
    assert "validate" in out
    assert "schema" in out
    assert "list" in out


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
        Bundle.parse(
            {**BUNDLE, "series": {"bass": Series(rate=10, values=(0.5,), unit=Unit.RATIO).model_dump(mode="json")}}
        ),
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
    assert listing["slew"] == registry.get("slew").description()


def test_list_json_with_names_describes_only_those(capsys):
    assert main(["list", "--json", "lfo", "clip"]) == 0
    assert sorted(json.loads(capsys.readouterr().out)) == ["clip", "lfo"]


def test_list_json_of_an_unknown_name_prints_nothing_on_stdout_and_fails(capsys):
    assert main(["list", "--json", "nosuch"]) == 1
    captured = capsys.readouterr()
    assert captured.out == ""
    assert "nosuch" in captured.err
