"""A graph the library accepts can be walked on the smallest thread stack it supports: read, saved as a patch and
built again, shown and dropped, on a thread started under `threading.stack_size(STACK)`, without overflowing it.

Each chain is grown in such a thread to the deepest the library accepts (a deeper one is refused with a `SignalError`
saying it is too deep, at build or at read), then walked there. Each runs in a Python of its own: an overflow kills only
that process, and the test reports the signal that killed it and the walk it was in."""

import signal
import subprocess
import sys
import textwrap

import pytest

STACK = 256 * 1024  # bytes: the smallest thread stack a graph the library accepts must fit
TIMEOUT = 300.0  # seconds a scenario may take before it fails as hung

SCENARIO = """
import gc
import sys
import threading
import tomllib

import numpy as np

from bullterrier_control import Patch, Signal, lib
from bullterrier_control.errors import SignalError
from bullterrier_control.plugin import Params, shaper

STACK, LINK, STEPS, SAVES = int(sys.argv[1]), sys.argv[2], int(sys.argv[3]), sys.argv[4] == "saves"
TIMES = np.array([0.0, 0.5, 1.0])


class IdentityParams(Params):
    \"\"\"No parameters.\"\"\"


@shaper(params=IdentityParams)
def identity(signal: Signal, params: IdentityParams) -> Signal:
    \"\"\"Its input as it is: a call that adds no node of its own.\"\"\"
    return signal


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

# (a link of the chain, the steps it is grown to at most, whether it saves as a patch); every link keeps a constant 0.5
CHAINS = [
    pytest.param("signal.slew(rise=1.0, fall=1.0, rate=10.0)", 2000, True, id="slew"),
    pytest.param("signal.follow(attack=0.01, release=0.01, rate=10.0)", 2000, True, id="follow"),
    pytest.param("signal.clip(low=0.0, high=1.0)", 2000, True, id="clip"),
    pytest.param("lib.delay(signal, lag=0.1)", 2000, True, id="delay"),
    pytest.param("signal.pipe(identity)", 2000, True, id="a Python shaper that adds no node"),
    pytest.param("signal + 0.0", 2000, False, id="a sum"),
    pytest.param("Link(signal)", 400, False, id="Python computes, each reading the one before"),
    pytest.param(
        "Link(signal) if step % 1000 == 0 else signal + 0.0",
        200_000,
        False,
        id="Python computes over sums as deep as allowed",
    ),
]


@pytest.mark.parametrize(("link", "steps", "saves"), CHAINS)
def test_the_deepest_chain_the_library_accepts_is_walked_on_the_smallest_thread_stack(link, steps, saves):
    done = subprocess.run(
        [sys.executable, "-c", SCENARIO, str(STACK), link, str(steps), "saves" if saves else "-"],
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=False,
        timeout=TIMEOUT,
    )
    phases = done.stdout.split()
    if done.returncode < 0:
        killed = signal.Signals(-done.returncode).name
        pytest.fail(f"killed by {killed} in {' '.join(phases[-3:])} on a {STACK // 1024} KiB thread")
    assert done.returncode == 0, textwrap.shorten(done.stderr, 2000)
    assert phases[-1] == "done", textwrap.shorten(done.stderr, 2000)
