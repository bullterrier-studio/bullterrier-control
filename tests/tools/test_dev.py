"""`poe new-source` writes a source, shaper or operation that passes the gate as written, in a copy of the project.

An architecture test of the tooling: it runs the developer command and then the tests and the linter, as a contributor
would, on a copy of the project, so the real one is untouched. Every kind is Rust: its copy is also built (the core's
tests, clippy and the extension) with cargo, into the target folder the project's own builds use.
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
    for folder in ("src", "tests", "tools", "crates"):
        shutil.copytree(ROOT / folder, tmp_path / folder, ignore=skip)
    for file in ("pyproject.toml", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "rustfmt.toml"):
        shutil.copy(ROOT / file, tmp_path)
    return tmp_path


def run(checkout: Path, *argv: str) -> subprocess.CompletedProcess:
    environment = {**os.environ, "PYTHONPATH": str(checkout / "src")}  # the copy's package, not the installed one
    return subprocess.run(
        [sys.executable, *argv], cwd=checkout, env=environment, capture_output=True, encoding="utf-8", check=False
    )


def check_python(checkout: Path, *files: str) -> None:
    """The new thing's tests and the contract tests over it pass, and the linter is content with its Python `files`
    (its test's too)."""
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
        *files,
        "tests/builtin/test_fresh_one.py",
    )
    assert lint.returncode == 0, lint.stdout + lint.stderr


def cargo(checkout: Path, *argv: str) -> subprocess.CompletedProcess:
    """cargo in the copy, building into the target folder of the project (or `CARGO_TARGET_DIR`): the dependencies
    are compiled there already."""
    target = os.environ.get("CARGO_TARGET_DIR", str(ROOT / "target"))
    environment = {**os.environ, "CARGO_TARGET_DIR": target, "PYO3_BUILD_EXTENSION_MODULE": "1"}
    return subprocess.run(
        ["cargo", *argv], cwd=checkout, env=environment, capture_output=True, encoding="utf-8", check=False
    )


def install_extension(checkout: Path) -> None:
    """Build the copy's extension (it has the new shaper in its table) over the one the copy came with, as maturin
    would: macOS leaves the interpreter's symbols to be found when the extension is loaded."""
    leave_to_load = ["-C", "link-arg=-undefined", "-C", "link-arg=dynamic_lookup"] if sys.platform == "darwin" else []
    built = cargo(
        checkout, "rustc", "-p", "bullterrier-control-python", "--lib", "--locked", "--quiet", "--", *leave_to_load
    )
    assert built.returncode == 0, built.stderr
    library, installed = ("lib_core.dylib" if sys.platform == "darwin" else "lib_core.so"), "_core.abi3.so"
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    shutil.copy(target / "debug" / library, checkout / "src" / "bullterrier_control" / "_native" / installed)


WINDOWS = pytest.mark.skipif(
    sys.platform == "win32", reason="the build helper copies a unix library name (lib_core.so, _core.abi3.so)"
)


@WINDOWS
@pytest.mark.parametrize(
    ("kind", "gallery", "nodes"),
    [("source", True, "['fresh_one']"), ("shaper", True, "['fresh_one', 'input']"), ("operation", False, None)],
)
def test_a_new_thing_is_found_by_the_build_and_its_rust_and_python_tests_pass_as_written(
    checkout, kind, gallery, nodes
):
    made = run(checkout, "tools/dev.py", "new-source", "fresh_one", "--kind", kind)
    assert made.returncode == 0, made.stderr
    written = sorted(line.removeprefix("wrote ") for line in made.stdout.splitlines() if line.startswith("wrote "))
    assert written == [  # no Python module: the core registers what it runs
        "crates/core/src/builtin/fresh_one.rs",
        *(["examples/gallery/fresh_one.toml"] if gallery else []),
        "tests/builtin/test_fresh_one.py",
    ]
    # the one edit the author still makes by hand, said before they run into it
    assert "crates/core/src/builtin.rs" in made.stdout
    assert "uv run poe api" in made.stdout  # lib.pyi (and a shaper's _chain.py) are generated

    formatted = cargo(checkout, "fmt", "--all", "--check")
    assert formatted.returncode == 0, formatted.stdout + formatted.stderr
    for part, passed in (("--lib", "3 passed"), ("--doc", "1 passed")):  # its three unit tests, its one doctest
        rust = cargo(checkout, "test", "-p", "bullterrier-control", part, "--locked", "--quiet", "fresh_one")
        assert rust.returncode == 0, rust.stdout + rust.stderr
        assert passed in rust.stdout, rust.stdout
    linted = cargo(
        checkout, "clippy", "-p", "bullterrier-control", "--all-targets", "--locked", "--quiet", "--", "-D", "warnings"
    )
    assert linted.returncode == 0, linted.stdout + linted.stderr

    install_extension(checkout)
    check_python(checkout)
    if nodes:
        built = run(checkout, "-c", CHECK_GALLERY_PATCH)
        assert built.returncode == 0, built.stderr
        assert built.stdout.strip() == f"{nodes} True"


@WINDOWS
def test_a_new_shaper_scales_a_signal_by_its_depth_through_the_python_binding(checkout):
    run(checkout, "tools/dev.py", "new-source", "fresh_one", "--kind", "shaper")
    install_extension(checkout)
    scaled = run(
        checkout,
        "-c",
        "from bullterrier_control import lib\n"
        "from bullterrier_control.signal import Constant\n"
        "print(lib.fresh_one(Constant(3.0), depth=0.25).sample([0.0, 9.0]).tolist())",
    )
    assert scaled.returncode == 0, scaled.stderr
    assert scaled.stdout.strip() == "[0.75, 0.75]"


CHECK_GALLERY_PATCH = """
from pathlib import Path

import numpy
from bullterrier_control import load_patch

built = load_patch(Path("examples/gallery/fresh_one.toml")).build()
print(sorted(built), all(numpy.isfinite(signal.sample([0.0, 1.0, 2.5])).all() for signal in built.values()))
"""


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
    assert not (checkout / "crates" / "core" / "src" / "builtin" / "fresh_one.rs").exists()


@pytest.mark.parametrize("kind", ["source", "shaper", "operation"])
def test_a_name_whose_rust_file_exists_is_refused_and_nothing_is_written(checkout, kind):
    taken = checkout / "crates" / "core" / "src" / "builtin" / "fresh_one.rs"
    taken.write_text("// mine\n", encoding="utf-8")
    refused = run(checkout, "tools/dev.py", "new-source", "fresh_one", "--kind", kind)
    assert refused.returncode == 1
    assert "crates/core/src/builtin/fresh_one.rs exists already" in refused.stderr
    assert taken.read_text(encoding="utf-8") == "// mine\n"
    assert not (checkout / "tests" / "builtin" / "test_fresh_one.py").exists()
    assert not (checkout / "examples" / "gallery" / "fresh_one.toml").exists()


RESERVED = ["format", "version", "events", "series", "grid", "use", "signal"]
MALFORMED = ["Bad-Name", "Upper", "9lives", "_lead", "has space", ""]


@pytest.mark.parametrize("kind", ["source", "shaper", "operation"])
@pytest.mark.parametrize(
    ("name", "said"),
    [("slew", "exists"), *((name, "lower case") for name in MALFORMED), *((name, "reserved") for name in RESERVED)],
    ids=["taken", *(f"malformed-{name or 'empty'}" for name in MALFORMED), *(f"reserved-{name}" for name in RESERVED)],
)
def test_a_name_that_is_taken_malformed_or_reserved_is_refused_and_nothing_is_written(checkout, kind, name, said):
    folders = [
        checkout / "crates/core/src/builtin",
        checkout / "tests/builtin",
        checkout / "src/bullterrier_control/builtin",
    ]
    before = [sorted(path.name for path in folder.iterdir()) for folder in folders]
    refused = run(checkout, "tools/dev.py", "new-source", name, "--kind", kind)
    assert refused.returncode == 1
    assert said in refused.stderr
    assert [sorted(path.name for path in folder.iterdir()) for folder in folders] == before
    assert not (checkout / "examples").exists()
