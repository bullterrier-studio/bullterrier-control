"""The README's examples run and print what the README says they print."""

import re
from pathlib import Path

import pytest

README = Path(__file__).resolve().parents[1] / "README.md"
BLOCKS = re.findall(r"```python\n(.*?)```", README.read_text(encoding="utf-8"), re.DOTALL)


def test_the_readme_has_python_examples():
    assert BLOCKS


def relative_targets(markdown: str) -> list[str]:
    """The link and image targets that only work next to the file: PyPI shows the README on its own page."""
    prose = re.sub(r"```.*?```", "", markdown, flags=re.DOTALL)
    targets = re.findall(r"\]\(([^)\s]+)\)", prose) + re.findall(r'(?:src|href)="([^"]+)"', prose)
    return [target for target in targets if not target.startswith(("https://", "#"))]


@pytest.mark.parametrize(
    ("markdown", "expected"),
    [
        pytest.param("[a](https://example.org/a.md)", [], id="an absolute link is fine"),
        pytest.param("[a](#install)", [], id="an anchor in the page is fine"),
        pytest.param('<img src="https://example.org/a.svg">', [], id="an absolute image is fine"),
        pytest.param("[a](docs/a.md)", ["docs/a.md"], id="a relative link breaks on PyPI"),
        pytest.param("[a](../a.md)", ["../a.md"], id="a parent path breaks on PyPI"),
        pytest.param('<img src="docs/a.svg">', ["docs/a.svg"], id="a relative image breaks on PyPI"),
        pytest.param('<a href="LICENSE">x</a>', ["LICENSE"], id="a relative href breaks on PyPI"),
        pytest.param("```python\nvalues[0](1)\n```", [], id="code is not prose"),
    ],
)
def test_a_relative_target_is_found(markdown, expected):
    assert relative_targets(markdown) == expected


def test_every_link_and_image_in_the_readme_is_absolute():
    assert relative_targets(README.read_text(encoding="utf-8")) == []


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
