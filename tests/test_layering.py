"""The package's layering: which way imports may point (import-linter, configured in pyproject.toml). Each layer may
import only the layers below it:

    app        __main__ > cli
    builtin | lib | patch | plotting   bullterrier-control's own sources and shapers (never importing each other) | all
               of them by name | a graph of them as data | signals drawn as a picture
    plugin     the author API: the names a package of sources and shapers uses
    registry   the table of sources and shapers, and the queries over it
    rendered | inputs   a signal that needs history, rendered once | the data a source takes besides parameters
    signal | params | easing | cadence | hashing | scaling | rounding
               what a signal is and how it is read | how a source or shaper declares its parameters | how progress is
               reshaped (easing curves) | where a read falls in a cycle | a seeded hash | how 0 to 1 is put on a range |
               when a time counts as a bound
    _chain     the chain methods `Signal` inherits, one per built-in shaper (generated); `signal` imports it
    formats    the formats' file input and output and `events` (the classes are the Rust extension's)
    _native    the home of the Rust extension (`_native._core`): the formats' classes and the shapers' kernels; it
               imports only `errors` (the extension imports its `FormatError` when it first raises one)
    errors     the exceptions; they import nothing from bullterrier_control, so every layer may raise them

The Rust core keeps the same rule for its built-ins: `crates/core/src/builtin/<name>.rs` never refers to another.

These are architecture tests: their subject is the package's structure (its import graph, its source files and the
modules a fresh import loads), so, unlike behaviour tests, they read source (the linter).
"""

import json
import os
import re
import shutil
import subprocess
import sys
from collections.abc import Callable
from pathlib import Path

import pytest

import bullterrier_control

PACKAGE = Path(bullterrier_control.__file__).parent
ROOT = PACKAGE.parent.parent  # the repository: pyproject.toml holds the contracts
LINT_IMPORTS = Path(sys.executable).with_name("lint-imports")  # `uv run lint-imports` from the repository root

LAYERS = (
    "Layers: app > builtin | lib | patch | plotting > plugin > registry > rendered | inputs > signal | params | easing"
    " | cadence | hashing | scaling | rounding > _chain > formats > _native > errors"
)
SOURCES_APART = "Built-in sources never import each other"
SOURCES_DOOR = "Built-in sources use only the author API"
PATCH_DOOR = "Patch imports only registry, formats, errors and _native"
CONTRACTS = [LAYERS, SOURCES_APART, SOURCES_DOOR, PATCH_DOOR]


def lint(cwd: Path, pythonpath: Path | None = None) -> subprocess.CompletedProcess:
    # a wide terminal, so that a contract's name is on one line however long it is
    environment = {**os.environ, "COLUMNS": "1000", **({"PYTHONPATH": str(pythonpath)} if pythonpath else {})}
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
        "formats/event.py",
        "import bullterrier_control.params",
        "bullterrier_control.formats.event",
        LAYERS,
    ),
    "formats to signal": (
        "formats/event.py",
        "import bullterrier_control.signal",
        "bullterrier_control.formats.event",
        LAYERS,
    ),
    "signal to params": ("signal.py", "import bullterrier_control.params", "bullterrier_control.signal", LAYERS),
    "params to signal": ("params.py", "import bullterrier_control.signal", "bullterrier_control.params", LAYERS),
    "the chain methods to signal": (
        "_chain.py",
        "import bullterrier_control.signal",
        "bullterrier_control._chain",
        LAYERS,
    ),
    "signal to rendered": ("signal.py", "import bullterrier_control.rendered", "bullterrier_control.signal", LAYERS),
    "params to rendered": ("params.py", "import bullterrier_control.rendered", "bullterrier_control.params", LAYERS),
    "rendered to registry": (
        "rendered.py",
        "import bullterrier_control.registry",
        "bullterrier_control.rendered",
        LAYERS,
    ),
    "registry to plugin": ("registry.py", "import bullterrier_control.plugin", "bullterrier_control.registry", LAYERS),
    "signal to registry": ("signal.py", "import bullterrier_control.registry", "bullterrier_control.signal", LAYERS),
    "plugin to sources": ("plugin.py", "import bullterrier_control.builtin", "bullterrier_control.plugin", LAYERS),
    "registry to sources": (
        "registry.py",
        "import bullterrier_control.builtin",
        "bullterrier_control.registry",
        LAYERS,
    ),
    "a source to the command line": (
        "builtin/clip.py",
        "import bullterrier_control.cli",
        "bullterrier_control.builtin.clip",
        LAYERS,
    ),
    "a patch to the command line": ("patch.py", "import bullterrier_control.cli", "bullterrier_control.patch", LAYERS),
    "plotting to the command line": (
        "plotting.py",
        "import bullterrier_control.cli",
        "bullterrier_control.plotting",
        LAYERS,
    ),
    "a patch to lib": ("patch.py", "import bullterrier_control.lib", "bullterrier_control.patch", LAYERS),
    "lib to a patch": ("lib.py", "import bullterrier_control.patch", "bullterrier_control.lib", LAYERS),
    "a source to a patch": (
        "builtin/clip.py",
        "import bullterrier_control.patch",
        "bullterrier_control.builtin.clip",
        LAYERS,
    ),
    "a source to the signal module": (
        "builtin/clip.py",
        "import bullterrier_control.signal",
        "bullterrier_control.builtin.clip",
        SOURCES_DOOR,
    ),
    "a source to the errors": (
        "builtin/clip.py",
        "import bullterrier_control.errors",
        "bullterrier_control.builtin.clip",
        SOURCES_DOOR,
    ),
    "a source to the registry": (
        "builtin/clip.py",
        "import bullterrier_control.registry",
        "bullterrier_control.builtin.clip",
        SOURCES_DOOR,
    ),
    "a source to the params module": (
        "builtin/clip.py",
        "import bullterrier_control.params",
        "bullterrier_control.builtin.clip",
        SOURCES_DOOR,
    ),
    "a source to the rendered module": (
        "builtin/clip.py",
        "import bullterrier_control.rendered",
        "bullterrier_control.builtin.clip",
        SOURCES_DOOR,
    ),
    "a source to the formats": (
        "builtin/clip.py",
        "import bullterrier_control.formats",
        "bullterrier_control.builtin.clip",
        SOURCES_DOOR,
    ),
    "a source to another source": (
        "builtin/clip.py",
        "import bullterrier_control.builtin.gate",
        "bullterrier_control.builtin.clip",
        SOURCES_APART,
    ),
    "inputs to registry": ("inputs.py", "import bullterrier_control.registry", "bullterrier_control.inputs", LAYERS),
    "inputs to rendered": ("inputs.py", "import bullterrier_control.rendered", "bullterrier_control.inputs", LAYERS),
    "signal to inputs": ("signal.py", "import bullterrier_control.inputs", "bullterrier_control.signal", LAYERS),
    "a source to the inputs module": (
        "builtin/clip.py",
        "import bullterrier_control.inputs",
        "bullterrier_control.builtin.clip",
        SOURCES_DOOR,
    ),
    "errors to formats": ("errors.py", "import bullterrier_control.formats", "bullterrier_control.errors", LAYERS),
    "formats to cli": (
        "formats/event.py",
        "import bullterrier_control.cli",
        "bullterrier_control.formats.event",
        LAYERS,
    ),
    "cli to main": ("cli.py", "import bullterrier_control.__main__", "bullterrier_control.cli", LAYERS),
    "the Rust extension's home to signal": (
        "_native/__init__.py",
        "import bullterrier_control.signal",
        "bullterrier_control._native",
        LAYERS,
    ),
    "the Rust extension's home to inputs": (
        "_native/__init__.py",
        "import bullterrier_control.inputs",
        "bullterrier_control._native",
        LAYERS,
    ),
    "patch to signal": ("patch.py", "import bullterrier_control.signal", "bullterrier_control.patch", PATCH_DOOR),
    "patch to inputs": ("patch.py", "import bullterrier_control.inputs", "bullterrier_control.patch", PATCH_DOOR),
    "patch to params": ("patch.py", "import bullterrier_control.params", "bullterrier_control.patch", PATCH_DOOR),
    "patch to the author API": (
        "patch.py",
        "import bullterrier_control.plugin",
        "bullterrier_control.patch",
        PATCH_DOOR,
    ),
    "errors to the Rust extension's home": (
        "errors.py",
        "from bullterrier_control._native import _core",
        "bullterrier_control.errors",
        LAYERS,
    ),
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
    "params to formats": ("params.py", "import bullterrier_control.formats"),
    "cli to params": ("cli.py", "import bullterrier_control.params"),
    "cli to signal": ("cli.py", "import bullterrier_control.signal"),
    "cli to rendered": ("cli.py", "import bullterrier_control.rendered"),
    "rendered to signal": ("rendered.py", "import bullterrier_control.signal"),
    "registry to rendered": ("registry.py", "import bullterrier_control.rendered"),
    "registry to params": ("registry.py", "import bullterrier_control.params"),
    "plugin to registry": ("plugin.py", "import bullterrier_control.registry"),
    "cli to plugin": ("cli.py", "import bullterrier_control.plugin"),
    "cli to sources": ("cli.py", "import bullterrier_control.builtin"),
    "a source to the plugin api": ("builtin/clip.py", "import bullterrier_control.plugin"),
    "patch to registry": ("patch.py", "import bullterrier_control.registry"),
    "patch to formats": ("patch.py", "import bullterrier_control.formats"),
    "patch to errors": ("patch.py", "import bullterrier_control.errors"),
    "patch to the Rust extension": ("patch.py", "from bullterrier_control._native import _core"),
    "registry to inputs": ("registry.py", "import bullterrier_control.inputs"),
    "inputs to formats": ("inputs.py", "import bullterrier_control.formats"),
    "inputs to the Rust extension": ("inputs.py", "from bullterrier_control._native import _core"),
    "rendered to errors": ("rendered.py", "import bullterrier_control.errors"),
    "signal to formats": ("signal.py", "import bullterrier_control.formats"),
    "formats to the Rust extension": ("formats/bundle.py", "from bullterrier_control._native import _core"),
    "the Rust extension's home to errors": ("_native/__init__.py", "import bullterrier_control.errors"),
    "rendered to the Rust extension": ("rendered.py", "from bullterrier_control._native import _core"),
    # the one exception to "Built-in sources use only the author API": a built-in may call the Rust core
    "a source to the Rust extension": ("builtin/clip.py", "from bullterrier_control._native import _core"),
}


@pytest.mark.parametrize(("module", "line"), ALLOWED.values(), ids=ALLOWED.keys())
def test_a_layer_may_import_the_layers_below_it(module, line, tmp_path):
    result = lint_a_copy_with(tmp_path, module, line)
    assert result.returncode == 0, result.stdout + result.stderr


# --- the Rust extension --------------------------------------------------------------------------------------------


def package_modules_loaded_by(module: str) -> list[str]:
    """The modules of bullterrier_control a fresh interpreter has loaded after importing `module`, besides `module` and
    the packages holding it. The package's own `__init__`, which imports nearly everything, is replaced by an empty
    package of the same folder, so only what `module` itself imports shows."""
    code = (
        "import importlib.util, json, sys, types\n"
        "package = types.ModuleType('bullterrier_control')\n"
        "package.__path__ = list(importlib.util.find_spec('bullterrier_control').submodule_search_locations)\n"
        "sys.modules['bullterrier_control'] = package\n"
        f"import {module}\n"
        f"loaded = [m for m in sys.modules if m.startswith('bullterrier_control.') and not {module!r}.startswith(m)]\n"
        "json.dump(sorted(loaded), sys.stdout)\n"
    )
    result = subprocess.run([sys.executable, "-c", code], capture_output=True, encoding="utf-8", check=False)
    assert result.returncode == 0, result.stderr
    return json.loads(result.stdout)


@pytest.mark.parametrize(
    ("module", "leaf"),
    [
        pytest.param("bullterrier_control._native._core", True, id="the extension is a leaf"),
        pytest.param("bullterrier_control.rendered", False, id="a module that imports others is not"),
    ],
)
def test_the_rust_extension_imports_nothing_from_the_package(module, leaf):
    """The import linter checks the extension's home (`_native`) but cannot read the compiled module in it: importing
    the module shows that it loads nothing else of the package either (it imports `errors` only when it first raises a
    `FormatError`)."""
    assert (package_modules_loaded_by(module) == []) == leaf


# --- the Rust core's built-ins -------------------------------------------------------------------------------------

CORE_CRATE = ROOT / "crates" / "core"
# a name in a path: before `::` (`beta::run`, `super::`), after it (`super::beta`, `b::beta`), or in a `::{...}` use
# group. However a built-in file reaches another (by `super`, `crate::builtin`, a glob, an alias), it names that
# module in a path; a test module's `use super::*` names none.
BEFORE_OR_AFTER_PATH = re.compile(r"\b(\w+)\s*(?=::)|(?<=::)\s*(\w+)")
USE_GROUP = re.compile(r"::\s*\{([^{}]*)\}")


def references_to_other_builtins(code: str, others: set[str]) -> list[str]:
    """The built-ins of `others` that the Rust built-in `code` (comments and strings blanked out) names in a path,
    sorted: the ways one built-in file could reach another."""
    named = {name for pair in BEFORE_OR_AFTER_PATH.findall(code) for name in pair}
    named |= {name for group in USE_GROUP.findall(code) for name in re.findall(r"\w+", group)}
    return sorted(named & others)


def builtins_referring_to_others(crate: Path, blank: Callable[[str], str]) -> dict[str, list[str]]:
    """File name -> the other built-ins it names, for each file of `crate`'s `src/builtin/` folder (none yet is none)
    that names any."""
    files = sorted((crate / "src" / "builtin").glob("*.rs"))
    names = {path.stem for path in files}
    found = {
        path.name: references_to_other_builtins(blank(path.read_text(encoding="utf-8")), names - {path.stem})
        for path in files
    }
    return {name: others for name, others in found.items() if others}


def test_no_rust_builtin_refers_to_another(rust_code):
    assert builtins_referring_to_others(CORE_CRATE, rust_code) == {}


def test_a_copy_of_the_core_in_which_one_builtin_refers_to_another_is_caught(tmp_path, rust_code):
    crate = tmp_path / "core"
    shutil.copytree(CORE_CRATE, crate, ignore=shutil.ignore_patterns("target"))
    builtin = crate / "src" / "builtin"
    builtin.mkdir(exist_ok=True)
    files = {
        "alpha.rs": "use super::beta;\n\nfn run() {}\n",
        "beta.rs": "use crate::Error;\n\nfn run() {}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n}\n",
        "gamma.rs": "// unlike super::alpha, this keeps to itself\nfn run() {}\n",
    }
    for name, source in files.items():
        (builtin / name).write_text(source, encoding="utf-8")
    assert builtins_referring_to_others(crate, rust_code) == {"alpha.rs": ["beta"]}


@pytest.mark.parametrize(
    ("source", "found"),
    [
        pytest.param("use super::follow;\n", ["follow"], id="a sibling through super"),
        pytest.param("use super::{follow, Params};\n", ["follow"], id="a sibling in a use group from super"),
        pytest.param("mod tests {\n    use super::super::follow;\n}\n", ["follow"], id="from a test module, up twice"),
        pytest.param("let out = crate::builtin::slew::run(p, &q);\n", ["slew"], id="a crate path"),
        pytest.param("use crate :: builtin :: slew;\n", ["slew"], id="spaced out"),
        pytest.param("use crate::{builtin::slew, error};\n", ["slew"], id="inside a use group"),
        pytest.param("use crate::builtin::{follow, slew};\n", ["follow", "slew"], id="a use group of built-ins"),
        pytest.param(
            "use crate::{error, builtin};\nfn f() { builtin::follow::run(); }\n",
            ["follow"],
            id="the builtin module brought in, then a built-in through it",
        ),
        pytest.param(
            "use super::*;\n\nfn f() {\n    follow::run();\n}\n", ["follow"], id="a glob from super, then one"
        ),
        pytest.param("use crate::builtin::*;\n\nfn f() {\n    slew::run();\n}\n", ["slew"], id="a glob from builtin"),
        pytest.param("use crate::builtin as b;\n\nfn f() {\n    b::slew::run();\n}\n", ["slew"], id="builtin aliased"),
        pytest.param("use crate::builtin as b;\nuse b::slew as s;\n", ["slew"], id="a built-in aliased through it"),
        pytest.param("#[cfg(test)]\nmod tests {\n    use super::*;\n}\n", [], id="a test module using its file"),
        pytest.param("let slew = 1.0;\nlet rate = slew * 2.0;\n", [], id="a local named like a built-in is none"),
        pytest.param("mod tests {\n    use super::{run, Params};\n}\n", [], id="a test module naming its file's items"),
        pytest.param("use crate::error::Error;\nuse crate::{Params, Shape};\n", [], id="the core's other modules"),
        pytest.param("// unlike super::slew, this keeps to itself\nfn f() {}\n", [], id="a line comment is none"),
        pytest.param("/* crate::builtin::slew */\nfn f() {}\n", [], id="a block comment is none"),
        pytest.param('const NAME: &str = "builtin::slew";\n', [], id="a string is none"),
        pytest.param("use super::slewing;\n", [], id="a name containing a built-in's is none"),
    ],
)
def test_the_builtin_reference_check_flags_a_path_to_another_builtin_and_only_that(source, found, rust_code):
    assert references_to_other_builtins(rust_code(source), {"follow", "slew"}) == found
