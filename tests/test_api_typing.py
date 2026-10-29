"""mypy reads the typed API: chain methods and `lib` take what each one's parameters allow and refuse a misspelt keyword
or a wrong type, before anything runs.

Runs mypy, as the gate does (`pyproject.toml`'s settings), on snippets written into a temporary folder, against the
package in src: one run for all of them, each snippet a file, its findings told apart by file name.
"""

import os
import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
# one xdist worker runs this module, so mypy (with a cold cache) runs once
pytestmark = [pytest.mark.xdist_group("api-typing"), pytest.mark.typecheck]

PRELUDE = """import bullterrier_control as btc
from bullterrier_control import lib

energy = btc.from_values([0.0, 1.0, 0.2, 0.0], rate=10)
"""

ACCEPTED = """
glow: btc.Signal = energy.slew(rise=0.2, fall=0.5).clip(low=0.1, high=0.8)
wobble: btc.Signal = lib.lfo(rate=2.0, shape="saw-down")
flare: btc.Signal = lib.envelope(events=[{"t": 0.5}, {"t": 1.0, "strength": 0.5}], decay=0.25)
held: btc.Signal = energy.hold(events=btc.events([0.5, 1.0]))
kicks: tuple[btc.Event, ...] = lib.select(events=btc.events([0.5]), threshold=0.5)
piped: btc.Signal = energy.pipe(lib.slew, rise=0.2)
folded = lib.some_plugin(energy, depth=0.3)
folded_too = energy.pipe(lib.some_plugin, depth=0.3)
reveal_type(energy.slew(rise=0.2))
"""

# each refused line, and the code of mypy's finding
REJECTED = {
    "a misspelt keyword of a lib function": ("lib.slew(energy, rize=0.2)", "[call-arg]"),
    "a misspelt keyword of a chain method": ("energy.slew(rize=0.2)", "[call-arg]"),
    "text for a number": ('energy.clip(low="a")', "[arg-type]"),
    "a choice that is not one": ('lib.lfo(shape="sawtooth")', "[arg-type]"),
}


@pytest.fixture(scope="module")
def findings(tmp_path_factory) -> dict[str, list[str]]:
    """mypy's output lines for each snippet file, by its name without `.py`."""
    folder = tmp_path_factory.mktemp("snippets")
    (folder / "accepted.py").write_text(PRELUDE + ACCEPTED, encoding="utf-8")
    for number, (line, _) in enumerate(REJECTED.values()):
        (folder / f"rejected_{number}.py").write_text(f"{PRELUDE}\n{line}\n", encoding="utf-8")
    command = [
        sys.executable,
        "-m",
        "mypy",
        "--config-file",
        str(ROOT / "pyproject.toml"),
        "--cache-dir",
        str(folder / ".mypy_cache"),
        *sorted(path.name for path in folder.glob("*.py")),
    ]
    environment = {**os.environ, "MYPYPATH": str(ROOT / "src")}  # the package of this checkout
    result = subprocess.run(command, cwd=folder, env=environment, capture_output=True, encoding="utf-8", check=False)
    found: dict[str, list[str]] = {path.stem: [] for path in folder.glob("*.py")}
    for line in result.stdout.splitlines():
        name = line.split(".py:", 1)[0]
        if name in found:
            found[name].append(line)
    return found


def test_the_typed_api_accepts_a_pipeline_lib_calls_and_a_plugin_by_any_name(findings):
    errors = [line for line in findings["accepted"] if ": error:" in line]
    assert errors == []


def test_a_chain_method_gives_a_signal(findings):
    revealed = [line for line in findings["accepted"] if "Revealed type" in line]
    assert len(revealed) == 1
    assert revealed[0].endswith('Revealed type is "bullterrier_control.signal.Signal"')


@pytest.mark.parametrize(("number", "code"), [(n, code) for n, (_, code) in enumerate(REJECTED.values())], ids=REJECTED)
def test_mypy_refuses_a_wrong_keyword_or_value(findings, number, code):
    errors = [line for line in findings[f"rejected_{number}"] if ": error:" in line]
    assert len(errors) == 1, errors
    assert errors[0].endswith(code)
