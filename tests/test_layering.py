"""The package's layering: which way imports may point (import-linter, configured in pyproject.toml). Each layer may
import only the layers below it:

    app        __main__ > cli
    rendered   a signal that needs history, rendered once
    signal | params   what a signal is and how it is read | how a source or shaper declares its parameters
    formats    bundle > series | event | grid > model | units (each format builds on the base and the units only)
    errors     the exceptions; import nothing from bullterrier_control, so every layer may raise them

These are architecture tests: their subject is the package's structure (its import graph and its source files), so,
unlike behaviour tests, they read source (the linter).
"""

import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

import bullterrier_control

PACKAGE = Path(bullterrier_control.__file__).parent
ROOT = PACKAGE.parent.parent  # the repository: pyproject.toml holds the contracts
LINT_IMPORTS = Path(sys.executable).with_name("lint-imports")  # `uv run lint-imports` from the repository root

LAYERS = "Layers: app > rendered > signal | params > formats > errors"
FORMATS = "Formats: bundle > series | event | grid > model | units"
CONTRACTS = [LAYERS, FORMATS]


def lint(cwd: Path, pythonpath: Path | None = None) -> subprocess.CompletedProcess:
    # a wide terminal, so that a contract's name is on one line however long it is
    environment = {**os.environ, "COLUMNS": "200", **({"PYTHONPATH": str(pythonpath)} if pythonpath else {})}
    return subprocess.run(
        [str(LINT_IMPORTS), "--no-cache"], cwd=cwd, env=environment, capture_output=True, encoding="utf-8", check=False
    )


def lint_a_copy_with(tmp_path: Path, module: str, line: str) -> subprocess.CompletedProcess:
    """Lint a copy of the package (and its contracts) in which `module` also has the import `line`."""
    shutil.copytree(PACKAGE, tmp_path / "bullterrier_control", ignore=shutil.ignore_patterns("__pycache__"))
    shutil.copy(ROOT / "pyproject.toml", tmp_path)
    target = tmp_path / "bullterrier_control" / module
    target.write_text(target.read_text(encoding="utf-8") + f"\n{line}  # injected by the test\n", encoding="utf-8")
    return lint(tmp_path, pythonpath=tmp_path)


def test_the_package_keeps_every_import_contract():
    result = lint(ROOT)
    assert result.returncode == 0, result.stdout + result.stderr
    for name in CONTRACTS:  # every contract was really checked (a dropped or renamed one cannot pass silently)
        assert re.search(rf"{re.escape(name)} KEPT", result.stdout), f"{name!r} not reported kept:\n{result.stdout}"


# One forbidden import per rule, each in a copy of the package: the linter must fail, naming the module and the rule.
FORBIDDEN = {
    "errors to cli": ("errors.py", "import bullterrier_control.cli", "bullterrier_control.errors", LAYERS),
    "formats to params": (
        "formats/series.py",
        "import bullterrier_control.params",
        "bullterrier_control.formats.series",
        LAYERS,
    ),
    "formats to signal": (
        "formats/series.py",
        "import bullterrier_control.signal",
        "bullterrier_control.formats.series",
        LAYERS,
    ),
    "signal to params": ("signal.py", "import bullterrier_control.params", "bullterrier_control.signal", LAYERS),
    "params to signal": ("params.py", "import bullterrier_control.signal", "bullterrier_control.params", LAYERS),
    "signal to rendered": ("signal.py", "import bullterrier_control.rendered", "bullterrier_control.signal", LAYERS),
    "params to rendered": ("params.py", "import bullterrier_control.rendered", "bullterrier_control.params", LAYERS),
    "errors to formats": ("errors.py", "import bullterrier_control.formats", "bullterrier_control.errors", LAYERS),
    "formats to cli": (
        "formats/series.py",
        "import bullterrier_control.cli",
        "bullterrier_control.formats.series",
        LAYERS,
    ),
    "units to series": (
        "formats/units.py",
        "import bullterrier_control.formats.series",
        "bullterrier_control.formats.units",
        FORMATS,
    ),
    "event to grid": (
        "formats/event.py",
        "import bullterrier_control.formats.grid",
        "bullterrier_control.formats.event",
        FORMATS,
    ),
    "grid to series": (
        "formats/grid.py",
        "import bullterrier_control.formats.series",
        "bullterrier_control.formats.grid",
        FORMATS,
    ),
    "series to event": (
        "formats/series.py",
        "import bullterrier_control.formats.event",
        "bullterrier_control.formats.series",
        FORMATS,
    ),
    "series to bundle": (
        "formats/series.py",
        "import bullterrier_control.formats.bundle",
        "bullterrier_control.formats.series",
        FORMATS,
    ),
    "model to series": (
        "formats/model.py",
        "import bullterrier_control.formats.series",
        "bullterrier_control.formats.model",
        FORMATS,
    ),
    "cli to main": ("cli.py", "import bullterrier_control.__main__", "bullterrier_control.cli", LAYERS),
}


@pytest.mark.parametrize(("module", "line", "importer", "contract"), FORBIDDEN.values(), ids=FORBIDDEN.keys())
def test_a_forbidden_import_breaks_its_contract(module, line, importer, contract, tmp_path):
    result = lint_a_copy_with(tmp_path, module, line)
    assert result.returncode != 0
    assert re.search(rf"{re.escape(contract)} BROKEN", result.stdout), result.stdout + result.stderr
    assert importer in result.stdout, result.stdout


ALLOWED = {
    "cli to errors": ("cli.py", "import bullterrier_control.errors"),
    "cli to formats": ("cli.py", "import bullterrier_control.formats"),
    "params to formats": ("params.py", "import bullterrier_control.formats.model"),
    "cli to params": ("cli.py", "import bullterrier_control.params"),
    "cli to signal": ("cli.py", "import bullterrier_control.signal"),
    "cli to rendered": ("cli.py", "import bullterrier_control.rendered"),
    "rendered to signal": ("rendered.py", "import bullterrier_control.signal"),
    "rendered to errors": ("rendered.py", "import bullterrier_control.errors"),
    "signal to formats": ("signal.py", "import bullterrier_control.formats.series"),
    "series to model": ("formats/series.py", "import bullterrier_control.formats.model"),
    "event to units": ("formats/event.py", "import bullterrier_control.formats.units"),
    "bundle to grid": ("formats/bundle.py", "import bullterrier_control.formats.grid"),
    "grid to model": ("formats/grid.py", "import bullterrier_control.formats.model"),
}


@pytest.mark.parametrize(("module", "line"), ALLOWED.values(), ids=ALLOWED.keys())
def test_a_layer_may_import_the_layers_below_it(module, line, tmp_path):
    result = lint_a_copy_with(tmp_path, module, line)
    assert result.returncode == 0, result.stdout + result.stderr
