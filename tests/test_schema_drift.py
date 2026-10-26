"""The committed JSON Schema is what the models produce.

Its text depends on the version of pydantic that generates it (`const` or `enum` for a literal, which limits it adds),
so the committed file is the one the locked pydantic writes, and this test is left out of the run on the oldest
dependencies (`poe check-min-deps`). The schema's meaning is checked on every version in tests/test_cli.py.
"""

import json
from pathlib import Path

from bullterrier_control.cli import main

SCHEMA_FILE = Path(__file__).resolve().parents[1] / "schemas" / "bundle-1.schema.json"


def test_schema_prints_the_committed_schema(capsys):
    assert main(["schema"]) == 0
    assert json.loads(capsys.readouterr().out) == json.loads(SCHEMA_FILE.read_text(encoding="utf-8"))
