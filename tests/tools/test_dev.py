"""`poe new-source` writes a source, shaper or operation that passes the gate as written, in a copy of the project.

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


@pytest.mark.parametrize("kind", ["source", "shaper", "operation"])
def test_a_new_source_shaper_or_operation_passes_its_tests_the_contract_and_the_linter_as_written(checkout, kind):
    made = run(checkout, "tools/dev.py", "new-source", "fresh_one", "--kind", kind)
    assert made.returncode == 0, made.stderr
    assert "src/bullterrier_control/builtin/fresh_one.py" in made.stdout
    assert "uv run poe api" in made.stdout  # lib.pyi (and a shaper's method) are generated
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
        # its temporary folders go under the copy: in the folder all pytest runs share, its cleanup races the other
        # runs' (a leaked scandir iterator then raises a ResourceWarning, an error under the gate's settings)
        f"--basetemp={checkout / 'basetemp'}",
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


CHECK_GALLERY_PATCH = """
from pathlib import Path

import numpy
from bullterrier_control import load_patch

built = load_patch(Path("examples/gallery/fresh_one.toml")).build()
print(sorted(built), all(numpy.isfinite(signal.sample([0.0, 1.0, 2.5])).all() for signal in built.values()))
"""


@pytest.mark.parametrize(("kind", "nodes"), [("source", "['fresh_one']"), ("shaper", "['fresh_one', 'input']")])
def test_a_new_source_or_shaper_also_gets_a_gallery_patch_that_builds(checkout, kind, nodes):
    made = run(checkout, "tools/dev.py", "new-source", "fresh_one", "--kind", kind)
    assert made.returncode == 0, made.stderr
    assert "examples/gallery/fresh_one.toml" in made.stdout
    built = run(checkout, "-c", CHECK_GALLERY_PATCH)
    assert built.returncode == 0, built.stderr
    assert built.stdout.strip() == f"{nodes} True"


def test_a_new_operation_gets_no_gallery_patch_because_it_gives_no_signal_to_draw(checkout):
    made = run(checkout, "tools/dev.py", "new-source", "fresh_one", "--kind", "operation")
    assert made.returncode == 0, made.stderr
    assert not (checkout / "examples" / "gallery" / "fresh_one.toml").exists()


def test_a_name_whose_gallery_patch_exists_is_refused_and_nothing_is_written(checkout):
    (checkout / "examples" / "gallery").mkdir(parents=True)
    (checkout / "examples" / "gallery" / "fresh_one.toml").write_text("# mine\n", encoding="utf-8")
    refused = run(checkout, "tools/dev.py", "new-source", "fresh_one", "--kind", "source")
    assert refused.returncode == 1
    assert "examples/gallery/fresh_one.toml exists already" in refused.stderr
    assert not (checkout / "src" / "bullterrier_control" / "builtin" / "fresh_one.py").exists()


@pytest.mark.parametrize(("name", "said"), [("slew", "exists"), ("Bad-Name", "lower case")], ids=["taken", "malformed"])
def test_a_name_that_is_taken_or_malformed_is_refused_and_nothing_is_written(checkout, name, said):
    before = sorted(path.name for path in (checkout / "src/bullterrier_control/builtin").iterdir())
    refused = run(checkout, "tools/dev.py", "new-source", name, "--kind", "source")
    assert refused.returncode == 1
    assert said in refused.stderr
    assert sorted(path.name for path in (checkout / "src/bullterrier_control/builtin").iterdir()) == before
