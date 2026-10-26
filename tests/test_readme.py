"""The README's example runs and prints what the README says it prints."""

import re
from pathlib import Path

README = Path(__file__).resolve().parents[1] / "README.md"


def test_the_python_example_runs_and_gives_the_values_the_readme_shows():
    text = README.read_text(encoding="utf-8")
    code = re.search(r"```python\n(.*?)```", text, re.DOTALL)
    assert code is not None
    shown = re.search(r"# array\(\[(.*?)\]\)", code.group(1))
    assert shown is not None
    namespace: dict[str, object] = {}
    exec(code.group(1), namespace)  # noqa: S102  (the README is ours: this runs the example a reader would run)
    values = namespace["clipped"].sample([0.0, 0.1, 0.2, 0.3, 1.0])  # type: ignore[attr-defined]
    assert values.round(3).tolist() == [float(number) for number in shown.group(1).split(",")]
