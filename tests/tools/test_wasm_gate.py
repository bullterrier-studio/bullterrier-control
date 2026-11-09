"""The WebAssembly gate can fail: `poe wasm-test` builds the package, runs it in Node against what the native extension
answers, passes the workspace as it is, and fails, naming the test, for a defect in the glue (the times read in another
order, the bundle left out, a wrong name on an error, an array accepted as the times, a thing missing from the list),
for an expected value that differs from the package's by one unit in the last place or in the name of an error, and for
a browser build of a target a page cannot import.

An architecture test of the tooling: its subject is the `wasm-test` task in pyproject.toml, so it runs that task, the
way `uv run poe check` does, in a copy of the workspace into which one offence at a time is written. The copies share
one build folder, so the core is compiled once.
"""

import os
import shutil
import subprocess
import sys
from collections.abc import Callable, Iterator
from dataclasses import dataclass
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]
# one xdist worker runs this module, so the copy and its build folder are made once
pytestmark = [pytest.mark.gate, pytest.mark.node]
WORKSPACE_FILES = ["pyproject.toml", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "rustfmt.toml", "deny.toml"]
GLUE = "crates/wasm/src/lib.rs"
ORACLE = "tests/wasm/oracle.py"


@dataclass(frozen=True)
class Verdict:
    returncode: int
    report: str  # what the tools printed, both streams


@pytest.fixture(scope="module")
def workspace(tmp_path_factory) -> Path:
    root = tmp_path_factory.mktemp("wasm-workspace")
    for name in WORKSPACE_FILES:
        shutil.copy(ROOT / name, root)
    ignore = shutil.ignore_patterns("__pycache__")
    shutil.copytree(ROOT / "crates", root / "crates", ignore=ignore)
    shutil.copytree(ROOT / "tests" / "wasm", root / "tests" / "wasm", ignore=ignore)
    # the oracle reads the example patches and bundles
    shutil.copytree(ROOT / "examples", root / "examples", ignore=ignore)
    return root


@pytest.fixture(scope="module")
def wasm_env(tmp_path_factory) -> dict[str, str]:
    """`uv run`'s environment (the oracle imports the built package), offline, with one build folder for every copy."""
    return {
        **os.environ,
        "PATH": os.pathsep.join([str(Path(sys.executable).parent), os.environ.get("PATH", "")]),
        "CARGO_TARGET_DIR": str(tmp_path_factory.mktemp("target")),
        # `uv run` (the oracle) uses this environment as it is: no sync, no build of the copy
        "UV_PROJECT_ENVIRONMENT": sys.prefix,
        "UV_NO_SYNC": "1",
        "CARGO_NET_OFFLINE": "true",
        "CARGO_TERM_COLOR": "never",
        "NO_COLOR": "1",
    }


def wasm_test(workspace: Path, env: dict[str, str]) -> Verdict:
    command = [sys.executable, "-m", "poethepoet", "--executor", "simple", "wasm-test"]
    result = subprocess.run(command, cwd=workspace, env=env, capture_output=True, encoding="utf-8", check=False)
    return Verdict(result.returncode, result.stdout + result.stderr)


@pytest.fixture
def edit(workspace) -> Iterator[Callable[[str, str, str], None]]:
    """`edit(relative, old, new)` replaces `old` by `new` in one file of the copy for one test; afterwards the copy is
    as it was."""
    touched: dict[str, str] = {}

    def change(relative: str, old: str, new: str) -> None:
        path = workspace / relative
        text = path.read_bytes().decode("utf-8")
        assert old in text
        touched.setdefault(relative, text)
        path.write_bytes(text.replace(old, new).encode("utf-8"))

    yield change
    for relative, text in touched.items():
        (workspace / relative).write_bytes(text.encode("utf-8"))


def test_the_workspace_as_it_is_passes(workspace, wasm_env):
    verdict = wasm_test(workspace, wasm_env)
    assert verdict.returncode == 0, verdict.report
    assert "ℹ fail 0" in verdict.report


# One offence each: (file, the text to replace, its replacement, what the report must name: the failing tests)
OFFENCES = {
    "the times read in reverse order": (
        GLUE,
        "signal.sample(&floats_of(times))",
        "signal.sample(&floats_of(times).into_iter().rev().collect::<Vec<_>>())",
        ["✖ glow.toml: the nodes and the values are the native ones"],
    ),
    "the bundle left out": (
        GLUE,
        "self.0\n            .build(bundle.as_ref())",
        "self.0\n            .build(None)",
        ["✖ glow.toml: the nodes and the values are the native ones"],
    ),
    "a wrong name on an error": (
        GLUE,
        '"FormatError"',
        '"FormatFailure"',
        ["✖ text that is not TOML: refused with the native FormatError"],
    ),
    "an array accepted as the times": (
        GLUE,
        "if !times.is_instance_of::<Floats>() {",
        "if false {",
        ["✖ times that are an array, not a Float64Array: refused with a TypeError"],
    ),
    "a thing's own error named for another kind": (
        GLUE,
        'control::Error::Params { .. } | control::Error::Input { .. } => "ParameterError"',
        'control::Error::Params { .. } | control::Error::Input { .. } => "FormatError"',
        ["✖ a thing that refuses its call: refused with the native ParameterError"],
    ),
    "a thing left out of the list": (
        GLUE,
        ".things()\n        .map(|thing|",
        ".things()\n        .skip(1)\n        .map(|thing|",
        ["✖ things() describes what the native registry holds, parameters included"],
    ),
    "an expected value that differs by one unit in the last place": (
        ORACLE,
        "signal.sample(TIMES).tolist()",
        "(signal.sample(TIMES) * (1 + 2**-52)).tolist()",
        ["✖ gallery/slew.toml: the nodes and the values are the native ones"],
    ),
    "an expected error of another class": (
        ORACLE,
        '"name": type(error).__name__',
        '"name": "InvalidError"',
        ["✖ text that is not TOML: refused with the native InvalidError"],
    ),
    "a browser build of another target": (
        "pyproject.toml",
        "--target web --out-dir pkg/web",
        "--target no-modules --out-dir pkg/web",
        ["✖ the web build loads too and gives the same values"],
    ),
}


@pytest.mark.parametrize(("relative", "old", "new", "named"), list(OFFENCES.values()), ids=list(OFFENCES))
def test_a_defect_in_the_glue_or_a_different_expectation_fails_naming_the_test(
    workspace, wasm_env, edit, relative, old, new, named
):
    edit(relative, old, new)
    verdict = wasm_test(workspace, wasm_env)
    assert verdict.returncode != 0, verdict.report
    for name in named:
        assert name in verdict.report, verdict.report
