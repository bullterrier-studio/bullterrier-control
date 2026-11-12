"""The committed JSON Schemas are what the code produces: the bundle's is generated from the Rust core's types
(schemars), the patch's is written in patch.py. The schemas' meaning is checked in tests/test_cli.py."""

import json
from pathlib import Path

import pytest

from bullterrier_control.cli import main

SCHEMAS = Path(__file__).resolve().parents[1] / "schemas"


@pytest.mark.parametrize(
    ("arguments", "file"),
    [
        pytest.param(["bundle", "--bundle-version", "1"], "bundle-1.schema.json", id="bundle 1"),
        pytest.param(["bundle", "--bundle-version", "2"], "bundle-2.schema.json", id="bundle 2"),
        pytest.param(["bundle"], "bundle-2.schema.json", id="bundle, the newest by default"),
        pytest.param([], "bundle-2.schema.json", id="no argument, the newest bundle"),
        pytest.param(["patch"], "patch-1.schema.json", id="patch"),
    ],
)
def test_schema_prints_the_committed_schema(arguments, file, capsys):
    assert main(["schema", *arguments]) == 0
    assert json.loads(capsys.readouterr().out) == json.loads((SCHEMAS / file).read_text(encoding="utf-8"))


def test_schema_of_a_bundle_version_nobody_reads_is_an_error_not_a_traceback(capsys):
    assert main(["schema", "--bundle-version", "3"]) == 1
    assert "must be 1 or 2" in capsys.readouterr().err
