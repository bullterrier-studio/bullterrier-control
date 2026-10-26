"""`poe new-source` writes a source or shaper that passes the gate as written, in a copy of the project.

An architecture test of the tooling: it runs the developer command and then the tests and the linter, as a contributor
would, on a copy of the project, so the real one is untouched.
"""

import os
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]
pytestmark = pytest.mark.xdist_group("dev-scaffold")


@pytest.fixture
def checkout(tmp_path):
    skip = shutil.ignore_patterns("__pycache__", "*.pyc")
    for folder in ("src", "tests", "tools"):
        shutil.copytree(ROOT / folder, tmp_path / folder, ignore=skip)
    shutil.copy(ROOT / "pyproject.toml", tmp_path)
    return tmp_path


def run(checkout: Path, *argv: str) -> subprocess.CompletedProcess:
    environment = {**os.environ, "PYTHONPATH": str(checkout / "src")}  # the copy's package, not the installed one
    return subprocess.run(
        [sys.executable, *argv], cwd=checkout, env=environment, capture_output=True, encoding="utf-8", check=False
    )


@pytest.mark.parametrize("kind", ["source", "shaper"])
def test_a_new_source_or_shaper_passes_its_tests_the_contract_and_the_linter_as_written(checkout, kind):
    made = run(checkout, "tools/dev.py", "new-source", "fresh_one", "--kind", kind)
    assert made.returncode == 0, made.stderr
    assert "src/bullterrier_control/builtin/fresh_one.py" in made.stdout
    tests = run(
        checkout,
        "-m",
        "pytest",
        "-q",
        "-n",
        "0",
        "-p",
        "no:randomly",
        "-p",
        "no:cacheprovider",
        "tests/builtin/test_fresh_one.py",
        "tests/builtin/test_contract.py",
        "-k",
        "fresh_one",
    )
    assert tests.returncode == 0, tests.stdout + tests.stderr
    assert "passed" in tests.stdout
    lint = run(
        checkout,
        "-m",
        "ruff",
        "check",
        "--no-cache",
        "src/bullterrier_control/builtin/fresh_one.py",
        "tests/builtin/test_fresh_one.py",
    )
    assert lint.returncode == 0, lint.stdout + lint.stderr


@pytest.mark.parametrize(("name", "said"), [("slew", "exists"), ("Bad-Name", "lower case")], ids=["taken", "malformed"])
def test_a_name_that_is_taken_or_malformed_is_refused_and_nothing_is_written(checkout, name, said):
    before = sorted(path.name for path in (checkout / "src/bullterrier_control/builtin").iterdir())
    refused = run(checkout, "tools/dev.py", "new-source", name, "--kind", "source")
    assert refused.returncode == 1
    assert said in refused.stderr
    assert sorted(path.name for path in (checkout / "src/bullterrier_control/builtin").iterdir()) == before
