"""tools/rust_tools.py: the gate fails, naming the install command, when a cargo tool it runs is missing or at
another version than Cargo.toml pins, and `poe setup` installs exactly those.

The tools are stood in for by tiny programs on a PATH of the test's own (an installed program is an external boundary),
each printing a version as the real one does; a stand-in `cargo` records the install commands it is given.
"""

import importlib.util
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]


def _load_tool():
    spec = importlib.util.spec_from_file_location("rust_tools", ROOT / "tools" / "rust_tools.py")
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


rust_tools = _load_tool()

MANIFEST = '[workspace]\nmembers = []\n\n[workspace.metadata.tools]\ncargo-shear = "1.14.0"\ncargo-deny = "0.20.2"\n'
PINS = {"cargo-shear": "1.14.0", "cargo-deny": "0.20.2"}


@pytest.fixture
def programs(tmp_path, monkeypatch):
    """`programs(name=output)` puts a program `name` printing `output` on an otherwise empty PATH; `cargo` records its
    arguments in calls.txt."""
    folder = tmp_path / "bin"
    folder.mkdir()
    monkeypatch.setenv("PATH", str(folder))

    def install(name: str, output: str = "") -> None:
        script = f"import sys\nprint({output!r})\nopen({str(folder / 'calls.txt')!r}, 'a', encoding='utf-8')" + (
            ".write(' '.join(sys.argv[1:]) + '\\n')\n"
        )
        if sys.platform == "win32":
            (folder / f"{name}.py").write_text(script, encoding="utf-8")
            (folder / f"{name}.cmd").write_text(f'@"{sys.executable}" "{folder / name}.py" %*\n', encoding="utf-8")
        else:
            (folder / name).write_text(f"#!{sys.executable}\n{script}", encoding="utf-8")
            (folder / name).chmod(0o755)

    install.calls = lambda: (folder / "calls.txt").read_text(encoding="utf-8").splitlines()  # type: ignore[attr-defined]
    return install


def test_the_pins_are_read_from_the_workspace_metadata(tmp_path):
    (tmp_path / "Cargo.toml").write_text(MANIFEST, encoding="utf-8")
    assert rust_tools.pinned(tmp_path) == PINS


def test_tools_at_the_pinned_versions_are_no_problem(programs):
    programs("cargo-shear", "Version: 1.14.0")  # what cargo-shear prints
    programs("cargo-deny", "cargo-deny 0.20.2")
    assert rust_tools.problems(PINS) == []


@pytest.mark.parametrize(
    ("installed", "problem"),
    [
        pytest.param({"cargo-deny": "cargo-deny 0.20.2"}, "cargo-shear is not installed", id="a tool missing"),
        pytest.param(
            {"cargo-shear": "Version: 1.13.2", "cargo-deny": "cargo-deny 0.20.2"},
            "cargo-shear is 1.13.2, the gate needs 1.14.0",
            id="a tool at another version",
        ),
    ],
)
def test_a_missing_or_other_version_is_reported_with_the_command_that_installs_the_pin(programs, installed, problem):
    for name, output in installed.items():
        programs(name, output)
    [found] = rust_tools.problems(PINS)
    assert problem in found
    assert "`cargo install --locked cargo-shear@1.14.0`" in found


def test_with_cargo_binstall_the_command_downloads_instead_of_building(programs):
    programs("cargo-binstall")
    programs("cargo-deny", "cargo-deny 0.20.2")
    [found] = rust_tools.problems(PINS)
    assert "`cargo binstall --no-confirm --locked cargo-shear@1.14.0`" in found


@pytest.mark.parametrize(
    ("installed", "status"),
    [
        ({"cargo-shear": "Version: 1.14.0", "cargo-deny": "cargo-deny 0.20.2"}, 0),
        ({"cargo-shear": "Version: 1.14.0"}, 1),
    ],
    ids=["all pinned: passes silently", "one missing: fails"],
)
def test_check_fails_the_gate_only_on_a_problem(programs, tmp_path, capsys, installed, status):
    (tmp_path / "Cargo.toml").write_text(MANIFEST, encoding="utf-8")
    for name, output in installed.items():
        programs(name, output)
    assert rust_tools.main(["check", "--root", str(tmp_path)]) == status
    assert ("cargo-deny is not installed" in capsys.readouterr().err) == bool(status)


def test_install_installs_only_what_is_missing_or_at_another_version(programs, tmp_path):
    (tmp_path / "Cargo.toml").write_text(MANIFEST, encoding="utf-8")
    programs("cargo")
    programs("cargo-shear", "Version: 1.14.0")
    programs("cargo-deny", "cargo-deny 0.19.0")
    assert rust_tools.main(["install", "--root", str(tmp_path)]) == 0
    installs = [call for call in programs.calls() if call.startswith("install")]
    assert installs == ["install --locked cargo-deny@0.20.2"]


def test_specs_prints_every_pin_as_an_installer_takes_it_on_one_line(tmp_path, capsys):
    # CI hands this line to taiki-e/install-action (`tool:`), so the versions stay written in Cargo.toml alone
    (tmp_path / "Cargo.toml").write_text(MANIFEST, encoding="utf-8")
    assert rust_tools.main(["specs", "--root", str(tmp_path)]) == 0
    assert capsys.readouterr().out == "cargo-shear@1.14.0,cargo-deny@0.20.2\n"
