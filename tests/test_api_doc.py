"""docs/api.md: every Python example runs and shows the values it claims, and the Rust column is declared a target.

A documentation test of the page's rules, not of the library: the examples' own behaviour is tested where it lives.
"""

import re
from pathlib import Path

import pytest

PAGE = Path(__file__).resolve().parents[1] / "docs" / "api.md"
TEXT = PAGE.read_text(encoding="utf-8") if PAGE.is_file() else ""
RUST_NOTE = "The Rust blocks are the target API for the Rust core (not implemented yet)"
# the ten tasks of the draft, in order: a deleted example (or a section without its Python or Rust side) fails
TASKS = [
    "A curve from measured values",
    "Shape it: a chain",
    "Read it",
    "Events, and an envelope on each",
    "Keyframes with easing",
    "Combine",
    "A beat grid and a tempo-synced wobble",
    "Seeded noise",
    "Your own shaper (a plugin)",
    "The same look as a file (the extra)",
]


def sections() -> dict[str, str]:
    """The page's `## n. Title` sections by title."""
    return {title: body for _, title, body in re.findall(r"^## (\d+)\. ([^\n]*)\n(.*?)(?=^## |\Z)", TEXT, re.M | re.S)}


def blocks(language: str, text: str = TEXT) -> list[str]:
    return re.findall(rf"```{language}\n(.*?)```", text, re.DOTALL)


PYTHON = [(title, block) for title, body in sections().items() for block in blocks("python", body)]


def test_the_page_has_the_ten_tasks_in_order():
    assert list(sections()) == TASKS


@pytest.mark.parametrize("title", TASKS)
@pytest.mark.parametrize("language", ["python", "rust"])
def test_every_task_has_an_example_in_each_language(title, language):
    assert blocks(language, sections().get(title, ""))


def test_the_page_says_once_above_the_first_rust_block_that_rust_is_the_target():
    assert TEXT.count(RUST_NOTE) == 1
    assert TEXT.index(RUST_NOTE) < TEXT.index("```rust")


@pytest.mark.parametrize(
    ("title", "block"), PYTHON, ids=[f"{title}: block {number}" for number, (title, _) in enumerate(PYTHON, start=1)]
)
def test_the_python_example_runs_and_gives_the_values_the_page_shows(
    title, block, documented_example, throwaway_registry
):
    """A block that reads with `<name>.sample(...)` at the start of a line shows `# array([...])` for it."""
    compared = documented_example(block)
    assert compared or not re.search(r"^\w+\.sample\(", block, re.MULTILINE)


def names_a_path_outside_the_repository(block: str) -> bool:
    return bool(re.search(r"/[U]sers/|/home/|/tmp/|\.\./|~/|bullterrier-[d]esign|[A-Za-z]:\\", block))


@pytest.mark.parametrize("block", blocks("python") + blocks("rust"))
def test_no_example_names_a_path_outside_the_repository(block):
    assert not names_a_path_outside_the_repository(block)


@pytest.mark.parametrize(
    ("block", "outside"),
    [
        pytest.param('load("examples/a.json")', False, id="a path from the repository's root is fine"),
        pytest.param('load("/home/me/a.json")', True, id="an absolute path"),
        pytest.param('load("../a.json")', True, id="a parent folder"),
        pytest.param('load("~/a.json")', True, id="a home folder"),
        pytest.param('load("C:\\a.json")', True, id="a Windows drive"),
    ],
)
def test_the_path_check_finds_a_path_outside_the_repository(block, outside):
    assert names_a_path_outside_the_repository(block) is outside


@pytest.mark.parametrize(
    ("block", "outcome"),
    [
        pytest.param("x = 1\n", False, id="a block that shows no values compares none"),
        pytest.param(
            "import bullterrier_control as btc\ns = btc.from_values([0, 1], rate=1)\n"
            "s.sample([0.5, 1.0])  # array([0.5, 1. ])\n",
            True,
            id="values that agree are compared",
        ),
    ],
)
def test_the_checker_accepts_a_block_whose_values_agree(documented_example, block, outcome):
    assert documented_example(block) is outcome


@pytest.mark.parametrize(
    "block",
    [
        pytest.param(
            "import bullterrier_control as btc\ns = btc.from_values([0, 1], rate=1)\n"
            "s.sample([0.5, 1.0])  # array([0.6, 1. ])\n",
            id="a shown value that differs",
        ),
        pytest.param("# array([1.0])\nx = 1\n", id="values shown for a block that reads nothing"),
        pytest.param("raise KeyError('x')\n", id="a block that does not run"),
    ],
)
def test_the_checker_fails_a_block_whose_values_differ_or_that_does_not_run(documented_example, block):
    with pytest.raises((AssertionError, KeyError)):
        documented_example(block)
