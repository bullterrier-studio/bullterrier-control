"""The README's examples run and print what the README says they print."""

import re
from pathlib import Path

import pytest

README = Path(__file__).resolve().parents[1] / "README.md"
BLOCKS = re.findall(r"```python\n(.*?)```", README.read_text(encoding="utf-8"), re.DOTALL)


def test_the_readme_has_python_examples():
    assert BLOCKS


@pytest.mark.parametrize("block", BLOCKS, ids=[f"example {number}" for number, _ in enumerate(BLOCKS, start=1)])
def test_the_python_example_runs_and_gives_the_values_the_readme_shows(block, monkeypatch):
    """Every block ends with `<signal>.sample(<times>)` and a comment `# array([...])` showing its values."""
    shown = re.search(r"# array\(\[(.*?)\]\)", block)
    read = re.search(r"^(\w+\.sample\([^)]*\))", block, re.MULTILINE)
    assert shown is not None
    assert read is not None
    monkeypatch.chdir(README.parent)  # the examples name their files from the repository's root, as a reader runs them
    namespace: dict[str, object] = {}
    exec(block, namespace)  # noqa: S102  (the README is ours: this runs the example a reader would run)
    values = eval(read.group(1), namespace)  # noqa: S307  (the same README line)
    numbers = [number.strip() for number in shown.group(1).split(",")]
    decimals = max(len(number.partition(".")[2]) for number in numbers)
    assert values.round(decimals).tolist() == [float(number) for number in numbers]
