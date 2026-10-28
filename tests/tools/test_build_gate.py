"""The build gate can fail: `poe build-check` builds the sdist and the wheel (maturin, with the Rust extension) and
rejects a distribution PyPI would refuse to render (twine), and a wheel with a file that cannot be imported, with
anything but the library's code (a picture, a test), or without the extension where the package has it
(check-wheel-contents), and passes the project as it is.

An architecture test of the tooling: its subject is the `build-check` task in pyproject.toml, so it runs that task, the
way `uv run poe check` does, in a copy of the project into which one fault at a time is written.
"""

import shutil
import subprocess
import sys
import tomllib
import zipfile
from pathlib import Path

import pytest
from packaging.utils import parse_sdist_filename, parse_wheel_filename
from packaging.version import Version

ROOT = Path(__file__).resolve().parents[2]
# one xdist worker runs this module, so the Rust build folder its builds share is reused across its tests
pytestmark = pytest.mark.xdist_group("build-gate")
RUST_FILES = ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "rustfmt.toml")


@pytest.fixture(scope="module")
def rust_build_folder(tmp_path_factory) -> Path:
    return tmp_path_factory.mktemp("target")


@pytest.fixture
def project(project_copy, uv_run_env, rust_build_folder) -> Path:
    copy = project_copy("pyproject.toml", "README.md", "LICENSE", *RUST_FILES)
    shutil.copytree(ROOT / "crates", copy / "crates")
    uv_run_env["CARGO_TARGET_DIR"] = str(rust_build_folder)  # the copies share one build of the dependencies
    return copy


def build_check(project: Path, env: dict[str, str]) -> subprocess.CompletedProcess:
    command = [sys.executable, "-m", "poethepoet", "build-check"]
    return subprocess.run(command, cwd=project, env=env, capture_output=True, encoding="utf-8", check=False)


def test_the_project_as_it_is_builds_clean_distributions(project, uv_run_env):
    result = build_check(project, uv_run_env)
    assert result.returncode == 0, result.stdout + result.stderr
    version = tomllib.loads((project / "pyproject.toml").read_text(encoding="utf-8"))["project"]["version"]
    [sdist] = (project / "dist" / "check").glob("*.tar.gz")
    [wheel] = (project / "dist" / "check").glob("*.whl")  # sdist and wheel, nothing else
    assert len(list((project / "dist" / "check").iterdir())) == 2
    assert parse_sdist_filename(sdist.name) == ("bullterrier-control", Version(version))
    name, wheel_version, _, tags = parse_wheel_filename(wheel.name)
    assert (name, wheel_version) == ("bullterrier-control", Version(version))
    # one wheel per platform for every Python from 3.12 (the stable ABI), with the extension in the package
    assert {(tag.interpreter, tag.abi) for tag in tags} == {("cp312", "abi3")}
    native = "bullterrier_control/_native/"
    shipped = [entry for entry in zipfile.ZipFile(wheel).namelist() if entry.startswith(f"{native}_core.")]
    assert f"{native}_core.pyi" in shipped  # its types, for the package's own type checking
    assert len([entry for entry in shipped if entry.endswith((".so", ".pyd"))]) == 1, shipped


def test_a_long_description_pypi_cannot_render_fails_the_gate(project, uv_run_env):
    (project / "README.md").unlink()
    (project / "README.rst").write_text("A title\n=====\n\n.. not-a-directive::\n\n`unclosed\n", encoding="utf-8")
    pyproject = project / "pyproject.toml"
    pyproject.write_text(
        pyproject.read_text(encoding="utf-8").replace('readme = "README.md"', 'readme = "README.rst"'), encoding="utf-8"
    )
    result = build_check(project, uv_run_env)
    assert result.returncode != 0
    assert "long_description" in result.stdout + result.stderr  # twine: "`long_description` has syntax errors"


def test_a_wheel_file_that_cannot_be_imported_fails_the_gate(project, uv_run_env):
    (project / "src" / "bullterrier_control" / "not-importable.py").write_text(
        '"""A module with a dash."""\n', encoding="utf-8"
    )
    result = build_check(project, uv_run_env)
    assert result.returncode != 0
    report = result.stdout + result.stderr
    assert "W004" in report  # check-wheel-contents: module is not located at importable path
    assert "not-importable.py" in report


def _picture_in_the_package(project: Path) -> None:
    (project / "src" / "bullterrier_control" / "picture.jpg").write_bytes(b"\xff\xd8\xff\xd9")


def _test_in_the_package(project: Path) -> None:
    (project / "src" / "bullterrier_control" / "test_probe.py").write_text("def test_x(): ...\n", encoding="utf-8")


def test_a_wheel_that_ships_more_than_the_library_code_fails_the_gate_naming_each_file(project, uv_run_env):
    # one build for the two faults (a build takes seconds); each shipped file must be named
    for fault in (_picture_in_the_package, _test_in_the_package):
        fault(project)
    result = build_check(project, uv_run_env)
    report = result.stdout + result.stderr
    assert result.returncode != 0
    assert "W102" in report  # check-wheel-contents: wheel library contains files not in the package tree
    for shipped in (
        "bullterrier_control/picture.jpg",
        "test_probe.py",
    ):
        assert shipped in report


def test_a_wheel_without_the_extension_where_the_package_has_it_fails_the_gate(project, uv_run_env):
    pyproject = project / "pyproject.toml"
    text = pyproject.read_text(encoding="utf-8")
    assert 'module-name = "bullterrier_control._native._core"' in text
    pyproject.write_text(
        text.replace("bullterrier_control._native._core", "bullterrier_control.native._core"), encoding="utf-8"
    )
    (project / "src" / "bullterrier_control" / "native").mkdir()
    (project / "src" / "bullterrier_control" / "native" / "__init__.py").write_text('"""Native."""\n', encoding="utf-8")
    result = build_check(project, uv_run_env)
    report = result.stdout + result.stderr
    assert result.returncode != 0
    assert "W101" in report  # check-wheel-contents: wheel library is missing files in the package tree
    assert "bullterrier_control/_native/_core." in report
