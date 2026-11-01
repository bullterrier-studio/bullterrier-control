"""Which file of the release artifact the installed bullterrier-control came from, and that the deepest graph it
accepts is walked on a small thread stack: the build workflow's smoke install runs it with the fresh environment's own
Python, so a pass proves the files about to be published work, not a release from PyPI and not a build made on the fly.

    python check_installed.py ARTIFACT --from wheel|sdist

- `wheel`: the installed extension is, byte for byte, the one in exactly one wheel of ARTIFACT; that wheel is named.
- `sdist`: the package was built from ARTIFACT's sdist: its direct_url.json (PEP 610) names that file.
- Then each of `CHAINS`, grown to the deepest the installed package accepts, is read, saved as a patch and built
  again, shown and dropped on a thread of `STACK` bytes, each in a Python of its own, so an overflow is a failure
  naming the chain, how its process ended and the walk it was in (`tests/test_depth_stack.py` runs them on the
  development install).

Exit code 0 and the file's name on stdout; 1 and the reason on stderr otherwise. Standard library only: the
environment under test holds the package and its dependencies, nothing else.

Interface:
    installed_from(distribution, artifact, kind) -> the artifact's file; raises InstallError saying why not
    walk_deepest(chain, stack)                   -> None; raises WalkError naming the chain and how it failed
    main(argv)                                   -> exit code
"""

import argparse
import hashlib
import json
import signal
import subprocess
import sys
import zipfile
from importlib.metadata import Distribution, distribution
from pathlib import Path, PurePosixPath
from urllib.parse import urlparse
from urllib.request import url2pathname

EXTENSION = PurePosixPath("bullterrier_control/_native")  # where the extension `_core.<platform suffix>` lives
STACK = 256 * 1024  # bytes: the smallest thread stack a graph the library accepts must fit
TIMEOUT = 300.0  # seconds a chain's walk may take before it fails as hung

# Grows a chain on a thread of argv[1] bytes to the deepest the library accepts (a deeper one is refused with a
# `SignalError` saying it is too deep, at build or at read), then walks it there; prints each phase, "done" last.
SCENARIO = """
import gc
import sys
import threading
import tomllib

import numpy as np

from bullterrier_control import Patch, Signal, lib
from bullterrier_control.errors import SignalError
from bullterrier_control.plugin import Params, Whole, param, shaper, source

STACK, LINK, STEPS, SAVES = int(sys.argv[1]), sys.argv[2], int(sys.argv[3]), sys.argv[4] == "saves"
TIMES = np.array([0.0, 0.5, 1.0])


class IdentityParams(Params):
    \"\"\"No parameters.\"\"\"


@shaper(params=IdentityParams)
def identity(signal: Signal, params: IdentityParams) -> Signal:
    \"\"\"Its input as it is: a call that adds no node of its own.\"\"\"
    return signal


class NestParams(Params):
    \"\"\"How many bodies to nest.\"\"\"

    levels: Whole = param(
        0, doc="How many calls of itself the body makes, one inside another.", unit="count", min=0, max=1000
    )


@source(params=NestParams)
def nest(params: NestParams) -> Signal:
    \"\"\"A constant 0.5, made inside `levels` calls of itself, each inside the last.\"\"\"
    return lib.constant(value=0.5) if params.levels == 0 else nest(levels=params.levels - 1)


class Link(Signal):
    \"\"\"A Python signal reading the one before it.\"\"\"

    def __init__(self, before):
        self.before = before

    def compute(self, times):
        return self.before.sample(times)


link = eval("lambda signal, step: " + LINK)


def phase(name):
    print(name, flush=True)


def too_deep(error):
    return isinstance(error, SignalError) and "deep" in str(error)


def chain(steps):
    \"\"\"A constant 0.5 through `steps` links; None if the library refuses to build it.\"\"\"
    made = lib.constant(value=0.5)
    for step in range(steps):
        try:
            made = link(made, step)
        except SignalError as error:
            if too_deep(error):
                return None
            raise
    return made


def readable(made):
    try:
        made.sample(TIMES)
    except SignalError as error:
        if too_deep(error):
            return False
        raise
    return True


def deepest(accepts):
    \"\"\"The most steps, up to STEPS, whose chain the library builds and `accepts` (a refusal grows with depth, so
    this bisects).\"\"\"
    accepted, refused = 0, STEPS + 1
    while refused - accepted > 1:
        middle = (accepted + refused) // 2
        phase(f"grow:{middle}")
        made = chain(middle)
        if made is not None and accepts(made):
            accepted = middle
        else:
            refused = middle
        del made
    return accepted


def walk(steps, read):
    \"\"\"Every walk over the chain of `steps`: reading it (giving 0.5 if `read`, else read or refused), saving it as
    a patch and building that again, showing it and dropping it.\"\"\"
    made = chain(steps)
    phase(f"{steps}:read")
    if read:
        assert made.sample(TIMES).tolist() == [0.5, 0.5, 0.5]
    else:
        readable(made)  # read, or refused as too deep: either, but not an overflow
    if SAVES:
        phase(f"{steps}:save")
        text = Patch.from_signals({"out": made}).to_toml()
        phase(f"{steps}:build-patch")
        built = Patch.parse(tomllib.loads(text)).build()["out"]
        phase(f"{steps}:read-patch")
        assert readable(built) == readable(made)
        phase(f"{steps}:drop-patch")
        del built
        gc.collect()
    phase(f"{steps}:show")
    assert len(repr(made)) + len(str(made)) > 0
    phase(f"{steps}:drop")
    del made
    gc.collect()


def walks():
    read = deepest(readable)
    walk(read, read=True)
    built = deepest(lambda made: True)  # deeper than `read` where only a read refuses (Python calls nest too deep)
    walk(built, read=False)
    phase("done")


threading.stack_size(STACK)
walker = threading.Thread(target=walks)
walker.start()
walker.join()
"""


# a chain's name -> (a link of it, the steps it is grown to at most, whether it saves as a patch); every link keeps a
# constant 0.5
CHAINS = {
    "slew": ("signal.slew(rise=1.0, fall=1.0, rate=10.0)", 2000, True),
    "follow": ("signal.follow(attack=0.01, release=0.01, rate=10.0)", 2000, True),
    "clip": ("signal.clip(low=0.0, high=1.0)", 2000, True),
    "remap": ('signal.remap(low=0.0, high=1.0, ease="linear")', 2000, True),
    "delay": ("lib.delay(signal, lag=0.1)", 2000, True),
    "hold": ('signal.hold(events=[{"t": 0.25}])', 2000, True),
    "a Python shaper that adds no node": ("signal.pipe(identity)", 2000, True),
    "a sum": ("signal + 0.0", 2000, False),
    "Python computes, each reading the one before": ("Link(signal)", 400, False),
    "Python bodies, each calling a thing inside the last": ("nest(levels=step)", 200, False),
    "Python computes over sums as deep as allowed": (
        "Link(signal) if step % 1000 == 0 else signal + 0.0",
        200_000,
        False,
    ),
}


class InstallError(Exception):
    """The installed package did not come from the artifact."""


class WalkError(Exception):
    """A chain the installed package accepts was not walked on the thread stack given."""


def _is_extension(name: str) -> bool:
    path = PurePosixPath(name)
    return path.parent == EXTENSION and path.name.startswith("_core.") and path.suffix != ".pyi"


def installed_from(installed: Distribution, artifact: Path, kind: str) -> Path:
    """The file of `artifact` that `installed` came from (a wheel, or the sdist, as `kind` says); else InstallError."""
    if kind == "sdist":
        direct_url = installed.read_text("direct_url.json")
        if direct_url is None:
            raise InstallError("no direct_url.json: it was not installed from a file given by its path")
        source = Path(url2pathname(urlparse(json.loads(direct_url)["url"]).path))
        [sdist] = artifact.glob("*.tar.gz")
        if not (source.is_file() and source.samefile(sdist)):
            raise InstallError(f"it was built from {source}, not the artifact's sdist {sdist}")
        return sdist
    built = [file for file in installed.files or [] if _is_extension(file.as_posix())]
    if len(built) != 1:
        raise InstallError(f"{len(built)} builds of the extension are recorded as installed, not one")
    digest = hashlib.sha256(Path(str(installed.locate_file(built[0]))).read_bytes()).digest()
    matches = []
    for wheel in sorted(artifact.glob("*.whl")):
        with zipfile.ZipFile(wheel) as archive:
            names = [name for name in archive.namelist() if _is_extension(name)]
            if any(hashlib.sha256(archive.read(name)).digest() == digest for name in names):
                matches.append(wheel)
    if len(matches) != 1:
        raise InstallError(f"its extension {built[0]} is in {len(matches)} wheels of the artifact, not one: no wheel")
    return matches[0]


def walk_deepest(chain: str, stack: int = STACK) -> None:
    """Walk the deepest `chain` (a name of `CHAINS`) the package accepts on a thread of `stack` bytes, in a Python of
    its own; WalkError naming the chain, how that Python ended and its last phases if it did not finish."""
    link, steps, saves = CHAINS[chain]
    done = subprocess.run(  # noqa: S603  this interpreter, running SCENARIO with arguments of CHAINS
        [sys.executable, "-c", SCENARIO, str(stack), link, str(steps), "saves" if saves else "-"],
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=False,
        timeout=TIMEOUT,
    )
    phases = done.stdout.split()
    if done.returncode == 0 and phases[-1:] == ["done"]:
        return
    ended = signal.Signals(-done.returncode).name if done.returncode < 0 else f"exit code {done.returncode:#x}"
    where = f"{chain}: {ended} in {' '.join(phases[-3:])} on a {stack // 1024} KiB thread"
    raise WalkError(f"{where}\n{done.stderr[-2000:]}".rstrip())


def main(argv: list[str] | None = None) -> int:
    """Name the artifact's file this environment's bullterrier-control came from, then walk each of `CHAINS`; 1 with
    the reason if it came from none or a walk fails."""
    parser = argparse.ArgumentParser(
        description="Name the artifact's file the installed bullterrier-control came from; walk its deepest chains."
    )
    parser.add_argument("artifact", type=Path, help="the folder of the release's distributions")
    parser.add_argument("--from", dest="kind", choices=["wheel", "sdist"], required=True, help="how it was installed")
    arguments = parser.parse_args(argv)
    try:
        source = installed_from(distribution("bullterrier-control"), arguments.artifact, arguments.kind)
    except InstallError as error:
        print(
            f"check-installed: bullterrier-control is not installed from {arguments.artifact}: {error}", file=sys.stderr
        )
        return 1
    print(f"check-installed: bullterrier-control is installed from {source.name}")
    try:
        for chain in CHAINS:
            walk_deepest(chain)
    except WalkError as error:
        print(f"check-installed: the deepest chain is not walked: {error}", file=sys.stderr)
        return 1
    print(f"check-installed: the deepest chain of each of {len(CHAINS)} kinds walks on a {STACK // 1024} KiB thread")
    return 0


if __name__ == "__main__":
    sys.exit(main())
