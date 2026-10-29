"""The committed JSON Schemas are what the code produces: the bundle's is generated from the Rust core's types
(schemars), the patch's is written in patch.py. The schemas' meaning is checked in tests/test_cli.py."""

import json
from pathlib import Path

import pytest

from bullterrier_control.cli import main

SCHEMAS = Path(__file__).resolve().parents[1] / "schemas"


@pytest.mark.parametrize(("which", "file"), [("bundle", "bundle-1.schema.json"), ("patch", "patch-1.schema.json")])
def test_schema_prints_the_committed_schema(which, file, capsys):
    assert main(["schema", which]) == 0
    assert json.loads(capsys.readouterr().out) == json.loads((SCHEMAS / file).read_text(encoding="utf-8"))
