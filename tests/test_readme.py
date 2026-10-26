"""The README's examples run and print what the README says they print."""

import re
from pathlib import Path

import pytest

README = Path(__file__).resolve().parents[1] / "README.md"
BLOCKS = re.findall(r"```python\n(.*?)```", README.read_text(encoding="utf-8"), re.DOTALL)

# per block: the signal it builds and the times at which its `# array(...)` comment shows the values
READS = [("clipped", [0.0, 0.1, 0.2, 0.3, 1.0]), ("ramp", [1.0, 2.5, 7.0])]


def test_every_python_example_is_read_here():
    assert len(BLOCKS) == len(READS)


@pytest.mark.parametrize(("block", "read"), list(zip(BLOCKS, READS, strict=False)), ids=[name for name, _ in READS])
def test_the_python_example_runs_and_gives_the_values_the_readme_shows(block, read):
    name, times = read
    shown = re.search(r"# array\(\[(.*?)\]\)", block)
    assert shown is not None
    namespace: dict[str, object] = {}
    exec(block, namespace)  # noqa: S102  (the README is ours: this runs the example a reader would run)
    values = namespace[name].sample(times)  # type: ignore[attr-defined]
    assert values.round(4).tolist() == [float(number) for number in shown.group(1).split(",")]
