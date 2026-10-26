"""The worked examples in docs/examples.md run and give the numbers their tables show.

Each `## ` section has one Python block that builds `signal`, a table whose first two columns are a time and the value
worked out by hand, and optionally a **Needs:** line naming the sources it uses; until those are registered the example
is skipped with their names, so the skips are the to-do list. The example files in examples/ are valid bundles (JSON)
and patches (TOML).
"""

import re
from pathlib import Path

import numpy as np
import pytest

from bullterrier_control import load, load_patch, registry

ROOT = Path(__file__).resolve().parents[1]
PAGE = (ROOT / "docs" / "examples.md").read_text(encoding="utf-8")
SECTIONS = re.split(r"^## ", PAGE, flags=re.MULTILINE)[1:]


def example(section: str) -> tuple[str, list[str], str, list[tuple[float, float]]]:
    title = section.splitlines()[0]
    needs = (
        re.findall(r"`(\w+)`", needs_line.group(1))
        if (needs_line := re.search(r"\*\*Needs:\*\* (.*)", section))
        else []
    )
    code = re.search(r"```python\n(.*?)```", section, re.DOTALL)
    rows = re.findall(r"^\| ([0-9.]+) \| ([0-9.]+) \|", section, re.MULTILINE)
    assert code is not None, title
    assert rows, title
    return title, needs, code.group(1), [(float(time), float(value)) for time, value in rows]


EXAMPLES = [example(section) for section in SECTIONS]


def test_the_page_has_its_examples():
    assert len(EXAMPLES) == 8


@pytest.mark.parametrize(("title", "needs", "code", "rows"), EXAMPLES, ids=[title for title, *_ in EXAMPLES])
def test_each_example_gives_the_numbers_its_table_shows(title, needs, code, rows, monkeypatch):
    missing = [name for name in needs if name not in registry.all_registered()]
    if missing:
        pytest.skip(f"needs {', '.join(missing)}")
    monkeypatch.chdir(ROOT)  # the examples name their files from the repository's root, as a reader runs them
    namespace: dict[str, object] = {}
    exec(code, namespace)  # noqa: S102  (the page is ours: this runs the example a reader would run)
    times = np.array([time for time, _ in rows])
    expected = [value for _, value in rows]
    assert namespace["signal"].sample(times).tolist() == pytest.approx(expected, abs=1e-9)  # type: ignore[attr-defined]


@pytest.mark.parametrize("path", sorted((ROOT / "examples").glob("*.json")), ids=lambda path: path.name)
def test_every_example_file_is_a_valid_bundle(path):
    load(path)


@pytest.mark.parametrize("path", sorted((ROOT / "examples").glob("*.toml")), ids=lambda path: path.name)
def test_every_example_patch_is_valid(path):
    load_patch(path)
