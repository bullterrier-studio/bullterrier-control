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


def test_the_trailer_scenario_of_the_readme_gives_the_numbers_it_describes():
    look = load_patch(ROOT / "examples" / "trailer.toml").build(load(ROOT / "examples" / "trailer.json"))
    # a jolt of 1 on each explosion, fading with a time constant of 0.15 s: e^-1 after 0.15 s, e^-2 after 0.3 s
    shake = look["shake"].sample([4.2, 4.35, 10.1, 20.0])
    assert shake.tolist() == pytest.approx([1.0, 0.36787944117144233, 0.1353352832366127, 0.0], abs=1e-9)
    # dark corners from the boss at 60 s: 0.1 before, 0.7 from then on
    assert look["vignette"].sample([0.0, 59.9, 60.0, 61.5]).tolist() == [0.1, 0.1, 0.7, 0.7]
    # a wander that stays within its range, is not flat, and is the same every time it is built
    times = np.linspace(0, 62, 621)
    drift = look["drift"].sample(times)
    assert drift.min() >= -0.3
    assert drift.max() <= 0.3
    assert drift.max() - drift.min() > 0.2
    again = load_patch(ROOT / "examples" / "trailer.toml").build(load(ROOT / "examples" / "trailer.json"))["drift"]
    assert again.sample(times).tolist() == drift.tolist()
