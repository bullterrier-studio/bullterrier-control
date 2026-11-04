"""What the native extension answers, written as JSON for the Node tests of the WebAssembly package.

`poe wasm-test` runs this beside the build: the Node tests (`package.test.mjs`) call the WebAssembly package with the
same patches, bundles and times and expect exactly these values. The two are different binaries built from one core, so
a difference is a defect in the glue, the target or the build. Usage: `python tests/wasm/oracle.py <output file>`.
"""

import json
import sys
import tempfile
from dataclasses import asdict, dataclass
from pathlib import Path

import numpy as np

import bullterrier_control as btc
from bullterrier_control import BullterrierControlError, registry

ROOT = Path(__file__).resolve().parents[2]
EXAMPLES = ROOT / "examples"
# Patches that read a bundle, with the bundle each is built against; every other example patch stands alone.
BUNDLES = {"glow.toml": "kicks.json", "trailer.toml": "trailer.json"}
# Seconds: before 0 (read as 0), on a sample time, between samples, past the end of the examples' events.
TIMES = np.concatenate([[-1.5, 0.0], np.arange(0.0, 64.0, 0.0371)])


def patches() -> list[Path]:
    """The example patches: at the root of examples/ and in examples/gallery/ (one per source and shaper)."""
    return sorted([*EXAMPLES.glob("*.toml"), *EXAMPLES.glob("gallery/*.toml")])


def case(path: Path) -> dict[str, object]:
    """The patch's text, its bundle's text if it reads one, and each signal node's values at TIMES."""
    bundle_file = BUNDLES.get(path.name)
    bundle = EXAMPLES / bundle_file if bundle_file else None
    built = btc.load_patch(path).build(btc.load(bundle) if bundle else None)
    signals = {name: node for name, node in built.items() if isinstance(node, btc.Signal)}
    return {
        "name": path.relative_to(EXAMPLES).as_posix(),
        "toml": path.read_text(encoding="utf-8"),
        "bundle": bundle.read_text(encoding="utf-8") if bundle else None,
        "nodes": list(built),
        "signals": {name: signal.sample(TIMES).tolist() for name, signal in signals.items()},
    }


HEADER = 'format = "bullterrier-control/patch"\nversion = 1\n'
GLOW = (EXAMPLES / "glow.toml").read_text(encoding="utf-8")
LEVEL = HEADER + '[level]\nuse = "constant"\nvalue = 1\n[slew]\nuse = "slew"\nsignal = "level"\nrise = {rise}\n'


@dataclass(frozen=True)
class Failure:
    """What goes wrong: a patch's text, a bundle's text, and a node to read at `times` (None is NaN). `says` is a
    substring both packages' messages hold; the native package decides `name`, the class it raises."""

    id: str
    toml: str
    says: str
    bundle: str | None = None
    node: str | None = None
    times: tuple[float | None, ...] = (0.0,)


FAILURES = [
    Failure("text that is not TOML", "format = ", "not TOML"),
    Failure("a thing that is not registered", HEADER + '[a]\nuse = "nope"\n', "nope"),
    Failure("a parameter out of its range", LEVEL.format(rise=-1), "rise", node="slew"),
    Failure("a thing that refuses its call", HEADER + '[wobble]\nuse = "lfo"\nper = "beat"\n', "grid"),
    Failure("a bundle path with no bundle", GLOW, "events.kick"),
    Failure("a bundle that is not one", HEADER, "format", bundle="{}"),
    Failure("a path the bundle lacks", GLOW, "kick", bundle=(EXAMPLES / "trailer.json").read_text(encoding="utf-8")),
    Failure("a time that is not finite", LEVEL.format(rise=1), "finite", node="slew", times=(None,)),
    Failure("a node the patch lacks", LEVEL.format(rise=1), "halo", node="halo"),
]


def raised_by_native(failure: Failure) -> BullterrierControlError:
    """The error the native package raises for `failure`, by the public API (a patch file and a bundle file)."""
    with tempfile.TemporaryDirectory() as folder:
        patch_file, bundle_file = Path(folder, "patch.toml"), Path(folder, "bundle.json")
        patch_file.write_text(failure.toml, encoding="utf-8")
        bundle_file.write_text(failure.bundle or "", encoding="utf-8")
        only = [failure.node] if failure.node else None
        try:
            built = btc.load_patch(patch_file).build(btc.load(bundle_file) if failure.bundle else None, only=only)
            if failure.node:
                built[failure.node].sample([np.nan if time is None else time for time in failure.times])
        except BullterrierControlError as error:
            return error
    message = f"{failure.id}: the native package raised nothing"
    raise AssertionError(message)


def failures() -> list[dict[str, object]]:
    """Each failure as data for the Node tests, with the class the native package raises for it."""
    rows = []
    for failure in FAILURES:
        error = raised_by_native(failure)
        if failure.says not in str(error):
            message = f"{failure.id}: {error} does not say {failure.says!r}"
            raise AssertionError(message)
        rows.append({**asdict(failure), "name": type(error).__name__})
    return rows


def main(output: Path) -> None:
    """Write the oracle to `output`: the times, every example patch's case and what `bt-control list --json` prints."""
    things = {name: found.description() for name, found in registry.all_registered().items()}
    document = {
        "times": TIMES.tolist(),
        "cases": [case(path) for path in patches()],
        "things": things,
        "failures": failures(),
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(document, allow_nan=False), encoding="utf-8")


if __name__ == "__main__":
    main(Path(sys.argv[1]))
