"""The committed JSON Schemas are what the models produce.

The bundle's text depends on the version of pydantic that generates it (`const` or `enum` for a literal, which limits it
adds), so the committed file is the one the locked pydantic writes, and this test is left out of the run on the oldest
dependencies (`poe check-min-deps`). The schemas' meaning is checked on every version in tests/test_cli.py.
"""

import json
from pathlib import Path

import pytest

from bullterrier_control.cli import main

SCHEMAS = Path(__file__).resolve().parents[1] / "schemas"


@pytest.mark.parametrize(("which", "file"), [("bundle", "bundle-1.schema.json"), ("patch", "patch-1.schema.json")])
def test_schema_prints_the_committed_schema(which, file, capsys):
    assert main(["schema", which]) == 0
    assert json.loads(capsys.readouterr().out) == json.loads((SCHEMAS / file).read_text(encoding="utf-8"))
