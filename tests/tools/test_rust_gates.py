"""The Rust gates can fail: each Rust task of `poe check` rejects a known-bad change, naming the rule, and passes the
workspace as it is.

An architecture test of the tooling: its subject is the configuration (the poe tasks in pyproject.toml, the lints in
Cargo.toml, rustfmt.toml, deny.toml), so it runs those tasks, the way `uv run poe check` does, in a copy of the Rust
workspace into which one offence at a time is written. The copies share one build folder, so the dependencies are
compiled once, and nothing needs the network.
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
# one xdist worker runs this module, so its copy of the workspace and its build folder are made once
pytestmark = pytest.mark.xdist_group("rust-gates")
WORKSPACE_FILES = ["pyproject.toml", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "rustfmt.toml", "deny.toml"]
CORE = "crates/core/src/lib.rs"
CORE_MANIFEST = "crates/core/Cargo.toml"
MACROS = "crates/macros/src/lib.rs"
MACROS_MANIFEST = "crates/macros/Cargo.toml"
CARGO_TASKS = ["rust-fmt", "rust-lint", "rust-doc", "rust-shear", "rust-deny", "rust-wasm", "rust-test"]


@dataclass(frozen=True)
class Verdict:
    returncode: int
    report: str  # what the tool printed, both streams


@pytest.fixture(scope="module")
def workspace(tmp_path_factory) -> Path:
    root = tmp_path_factory.mktemp("workspace")
    for name in WORKSPACE_FILES:
        shutil.copy(ROOT / name, root)
    shutil.copytree(ROOT / "crates", root / "crates")
    return root


@pytest.fixture(scope="module")
def cargo_env(tmp_path_factory) -> dict[str, str]:
    """`uv run`'s environment (PyO3 finds this Python), offline, uncoloured, with one build folder for every copy."""
    return {
        **os.environ,
        "PATH": os.pathsep.join([str(Path(sys.executable).parent), os.environ.get("PATH", "")]),
        "CARGO_TARGET_DIR": str(tmp_path_factory.mktemp("target")),
        "CARGO_NET_OFFLINE": "true",
        "CARGO_TERM_COLOR": "never",
        "NO_COLOR": "1",
    }


def poe(workspace: Path, task: str, env: dict[str, str]) -> Verdict:
    # the simple executor: the commands run in this environment, as under `uv run`, without making one for the copy
    command = [sys.executable, "-m", "poethepoet", "--executor", "simple", task]
    result = subprocess.run(command, cwd=workspace, env=env, capture_output=True, encoding="utf-8", check=False)
    # cargo prints paths with the platform's separator; the cases name them with `/`
    return Verdict(result.returncode, (result.stdout + result.stderr).replace("\\", "/"))


def read(path: Path) -> str:
    """The text of `path` as its bytes spell it: no newline translation, so a write restores the file exactly."""
    return path.read_bytes().decode("utf-8")


def write(path: Path, text: str) -> None:
    """`text` as UTF-8 with its newlines as they are: rustfmt wants Unix ones (`newline_style`) on every platform."""
    path.write_bytes(text.encode("utf-8"))


@pytest.fixture
def edit(workspace, cargo_env) -> Iterator[Callable[[str, Callable[[str], str]], None]]:
    """`edit(relative, change)` rewrites one file of the copy with `change(text)` for one test; afterwards the copy is
    as it was, Cargo.lock included. A manifest edit updates the lock offline, as `cargo add` would."""
    touched = {"Cargo.lock": read(workspace / "Cargo.lock")}

    def change(relative: str, rewrite: Callable[[str], str]) -> None:
        path = workspace / relative
        touched.setdefault(relative, read(path))
        write(path, rewrite(read(path)))
        if relative.endswith("Cargo.toml"):
            subprocess.run(
                ["cargo", "metadata", "--offline", "--format-version", "1"],
                cwd=workspace,
                env=cargo_env,
                capture_output=True,
                check=True,
            )

    yield change
    for relative, text in touched.items():
        write(workspace / relative, text)


def appended(source: str) -> Callable[[str], str]:
    return lambda text: text + source


def replaced(old: str, new: str) -> Callable[[str], str]:
    def rewrite(text: str) -> str:
        assert old in text
        return text.replace(old, new)

    return rewrite


@pytest.mark.parametrize("task", CARGO_TASKS)
def test_the_workspace_as_it_is_passes_each_rust_gate(workspace, cargo_env, task):
    verdict = poe(workspace, task, cargo_env)
    assert verdict.returncode == 0, verdict.report


# One offence each: (task, file, change, what the report must name: the rule, and where it was broken)
OFFENCES = {
    "unformatted code": ("rust-fmt", CORE, appended("pub  fn  spaced( )->u8{1}\n"), ["Diff in", CORE]),
    "an unwrap": (
        "rust-lint",
        CORE,
        appended("\n/// Probe.\n#[must_use]\npub fn probe(text: &str) -> u8 {\n    text.parse().unwrap()\n}\n"),
        ["unwrap_used", CORE],
    ),
    "an expect": (
        "rust-lint",
        CORE,
        appended(
            '\n/// Probe.\n#[must_use]\npub fn probe(text: &str) -> u8 {\n    text.parse().expect("a number")\n}\n'
        ),
        ["expect_used", CORE],
    ),
    "a panic": (
        "rust-lint",
        CORE,
        appended('\n/// Probe.\npub fn probe() {\n    panic!("no");\n}\n'),
        ["clippy::panic", CORE],
    ),
    "printing": (
        "rust-lint",
        CORE,
        appended('\n/// Probe.\npub fn probe() {\n    println!("hello");\n}\n'),
        ["print_stdout", CORE],
    ),
    "dead code": ("rust-lint", CORE, appended("\nfn never_called() {}\n"), ["dead-code", "never_called", CORE]),
    "a public function without documentation": (
        "rust-lint",
        CORE,
        appended("\npub fn undocumented() {}\n"),
        ["missing-docs", CORE],
    ),
    "pub where no user of the crate reaches it": (
        "rust-lint",
        CORE,
        appended("\nmod inside {\n    /// Probe.\n    pub fn reachable_only_inside() {}\n}\n"),
        ["unreachable-pub", CORE],
    ),
    "unsafe code in the core": (
        "rust-lint",
        CORE,
        appended(
            "\n/// Probe.\n#[must_use]\npub fn probe(value: &u8) -> u8 {\n    unsafe { *std::ptr::from_ref(value) }\n}"
        ),
        ["unsafe_code", CORE],
    ),
    "unsafe code allowed on a function in the core": (
        "rust-lint",
        CORE,
        replaced(
            "#[cfg(test)]\nmod tests {",
            "/// Probe.\n#[must_use]\n#[allow(unsafe_code)]\npub fn probe(value: &u8) -> u8 {\n"
            "    // SAFETY: a reference is valid to read.\n    unsafe { *std::ptr::from_ref(value) }\n}\n\n"
            "#[cfg(test)]\nmod tests {",
        ),
        ["allow(unsafe_code)", "forbid", CORE],
    ),
    "an unwrap in the derives": (
        "rust-lint",
        MACROS,
        appended("\nfn probe(text: &str) -> u8 {\n    text.parse().unwrap()\n}\n"),
        ["unwrap_used", MACROS],
    ),
    "a pedantic lint": (
        "rust-lint",
        CORE,
        appended("\n/// Probe.\n#[must_use]\npub fn probe(value: u32) -> u8 {\n    value as u8\n}\n"),
        ["cast_possible_truncation", CORE],
    ),
    "a broken link in the documentation": (
        "rust-doc",
        CORE,
        appended("\n/// Like [`Nowhere`].\npub fn probe() {}\n"),
        ["broken-intra-doc-links", "Nowhere"],
    ),
    "a dependency no code uses": (
        "rust-shear",
        CORE_MANIFEST,
        replaced("[dependencies]\n", '[dependencies]\nlibc = "0.2"\n'),
        ["unused dependency `libc`", CORE_MANIFEST],
    ),
    "a dependency the derives do not use": (
        "rust-shear",
        MACROS_MANIFEST,
        replaced("[dependencies]\n", '[dependencies]\nlibc = "0.2"\n'),
        ["unused dependency `libc`", MACROS_MANIFEST],
    ),
    "PyO3 in the pure-Rust core": (
        "rust-deny",
        CORE_MANIFEST,
        replaced("[dependencies]\n", "[dependencies]\npyo3.workspace = true\n"),
        ["banned", "pyo3", "bullterrier-control"],
    ),
    "PyO3's raw bindings in the pure-Rust core": (
        "rust-deny",
        CORE_MANIFEST,
        replaced("[dependencies]\n", '[dependencies]\npyo3-ffi = "0.30"\n'),
        ["banned", "pyo3-ffi", "bullterrier-control"],
    ),
    "a licence outside the allow-list": (
        "rust-deny",
        CORE_MANIFEST,
        replaced("license.workspace = true", 'license = "GPL-3.0-only"'),
        ["rejected", "GPL-3.0-only"],
    ),
    "an API the browser build lacks": (
        "rust-wasm",
        CORE,
        appended("\nuse std::os::unix::ffi::OsStrExt;\n"),
        ["unix", CORE],
    ),
    "a failing Rust test": (
        "rust-test",
        CORE,
        appended(
            "\n#[cfg(test)]\nmod probe {\n    #[test]\n    fn fails() {\n        assert_eq!(1 + 1, 3);\n    }\n}\n"
        ),
        ["test result: FAILED", "probe::fails"],
    ),
}


@pytest.mark.parametrize(("task", "relative", "change", "named"), OFFENCES.values(), ids=OFFENCES.keys())
def test_each_offence_fails_its_gate_naming_the_rule(workspace, cargo_env, edit, task, relative, change, named):
    edit(relative, change)
    verdict = poe(workspace, task, cargo_env)
    assert verdict.returncode != 0, verdict.report
    for name in named:
        assert name in verdict.report, verdict.report


# --- stubtest: the stub against the built extension ---------------------------------------------------------------


def stubtest(checkout: Path, env: dict[str, str]) -> Verdict:
    # the gate's own task; the stub is read from the copy (mypy_path = "src" in its pyproject.toml)
    command = [sys.executable, "-m", "poethepoet", "stubtest"]
    result = subprocess.run(command, cwd=checkout, env=env, capture_output=True, encoding="utf-8", check=False)
    return Verdict(result.returncode, result.stdout + result.stderr)


STUB = "src/bullterrier_control/_native/_core.pyi"
EXTENSION = "bullterrier_control._native._core"


@pytest.mark.typecheck
def test_the_stub_as_it_is_matches_the_extension(project_copy, copy_env):
    verdict = stubtest(project_copy("pyproject.toml"), copy_env)
    assert verdict.returncode == 0, verdict.report


@pytest.mark.typecheck
@pytest.mark.parametrize(
    ("old", "new", "named"),
    [
        pytest.param(
            "def builtins() -> list[str]: ...\n",
            "def builtins() -> list[str]: ...\ndef ghost() -> int: ...\n",
            f"{EXTENSION}.ghost",
            id="a function the extension lacks",
        ),
        pytest.param("def builtins() -> list[str]: ...\n", "", f"{EXTENSION}.builtins", id="a function left out"),
        pytest.param(
            "def builtins() -> list[str]: ...\n",
            "def builtins(names: int) -> list[str]: ...\n",
            f"{EXTENSION}.builtins",
            id="a parameter the extension lacks",
        ),
    ],
)
def test_a_stub_that_disagrees_with_the_extension_fails_naming_the_function(project_copy, copy_env, old, new, named):
    checkout = project_copy("pyproject.toml")
    stub = checkout / STUB
    text = stub.read_text(encoding="utf-8")
    assert old in text
    stub.write_text(text.replace(old, new), encoding="utf-8")
    verdict = stubtest(checkout, copy_env)
    assert verdict.returncode != 0, verdict.report
    assert named in verdict.report, verdict.report
