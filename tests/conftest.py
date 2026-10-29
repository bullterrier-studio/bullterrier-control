"""What the tests share: the project's root, a scratch copy of it for the tooling tests, the environment `uv run`
gives a command, the blanking of Rust comments and strings, and the running of a documented example. A helper one
file uses stays in that file."""

import os
import re
import shutil
import sys
from collections.abc import Callable
from pathlib import Path

import pytest
from hypothesis import settings

ROOT = Path(__file__).resolve().parents[1]


@pytest.fixture
def project_copy(tmp_path):
    """`project_copy(*names)` copies the project's src/ (without bytecode caches) and the files `names` at its root into
    tmp_path, and returns tmp_path: a scratch project to write one fault into, the real one untouched."""

    def copy(*names: str) -> Path:
        shutil.copytree(ROOT / "src", tmp_path / "src", ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
        for name in names:
            shutil.copy(ROOT / name, tmp_path)
        return tmp_path

    return copy


@pytest.fixture
def uv_run_env() -> dict[str, str]:
    """The environment a command gets under `uv run`: this one, with its executables first on PATH."""
    return {**os.environ, "PATH": os.pathsep.join([str(Path(sys.executable).parent), os.environ.get("PATH", "")])}


# Rust comments, strings and character literals, in the order a Rust lexer meets them (a lifetime `'a` is none of them)
_RUST_NOISE = re.compile(r'//[^\n]*|/\*.*?\*/|r(#*)".*?"\1|"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])\'', re.DOTALL)


@pytest.fixture
def rust_code() -> Callable[[str], str]:
    """`rust_code(source)`: the Rust `source` with its comments, strings and character literals blanked out (each of
    their characters but a newline becomes a space): only code is left, at the lines and columns of the source."""

    def blank(source: str) -> str:
        return _RUST_NOISE.sub(lambda match: re.sub(r"[^\n]", " ", match.group()), source)

    return blank


# property tests are deterministic (the same cases on every run, so a failure reproduces) and never time out
settings.register_profile("bullterrier", derandomize=True, deadline=None)
settings.load_profile("bullterrier")


@pytest.fixture
def documented_example(monkeypatch) -> Callable[[str], bool]:
    """`documented_example(block)` runs the Python `block` of a document in a fresh namespace, from the project's root
    (as a reader runs it). A block that shows `# array([...])` must also have a line `<name>.sample(...)`: that call
    must give the shown values, rounded to the digits shown (four if all are whole numbers). Returns whether values were
    compared."""

    def run(block: str) -> bool:
        monkeypatch.chdir(ROOT)
        namespace: dict[str, object] = {}
        exec(block, namespace)  # noqa: S102  (the document is ours: this runs the example a reader would run)
        shown = re.search(r"# array\(\[(.*?)\]\)", block)
        if shown is None:
            return False
        read = re.search(r"^(\w+\.sample\([^)]*\))", block, re.MULTILINE)
        assert read is not None, "a block that shows `# array(...)` needs a line `<name>.sample(...)`"
        values = eval(read.group(1), namespace)  # noqa: S307  (the same document line)
        numbers = [number.strip() for number in shown.group(1).split(",")]
        # the digits shown; whole numbers (`1.`) show none and are exact, so they are held to four, not to zero
        decimals = max(len(number.partition(".")[2]) for number in numbers) or 4
        assert values.round(decimals).tolist() == [float(number) for number in numbers]
        return True

    return run


@pytest.fixture
def reports():
    """`reports(error, "series.bass.rate")` fails unless the error lists that field path (`path: rule`, `path = value`
    or an index into it), so a name that merely appears in the echoed input does not count."""

    def check(error: Exception, field: str) -> None:
        pattern = rf"(?<![\w'\"]){re.escape(field)}(?:: | = |\[|\.)"
        assert re.search(pattern, str(error)), f"{field!r} is not reported as a path in: {error}"

    return check


@pytest.fixture
def throwaway_registry():
    """Whatever a test registers is removed again afterwards, so tests cannot see each other's sources and shapers."""
    from bullterrier_control import registry

    before = set(registry.all_registered())
    yield
    for name in set(registry.all_registered()) - before:
        registry.unregister(name)


@pytest.fixture
def empty_registry():
    """The registry as if nothing had been registered or loaded; afterwards it holds what it held before."""
    from bullterrier_control import registry

    with registry.isolated():
        yield


@pytest.fixture
def without_matplotlib(monkeypatch):
    """Importing matplotlib fails, as if it were not installed, whether or not an earlier test imported it."""
    loaded = [name for name in sys.modules if name.startswith("matplotlib.")]
    for name in {"matplotlib", "matplotlib.figure", *loaded}:
        monkeypatch.setitem(sys.modules, name, None)
