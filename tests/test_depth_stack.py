"""A graph the library accepts can be walked on the smallest thread stack it supports: read, saved as a patch and
built again, shown and dropped, on a thread started under `threading.stack_size(STACK)`, without overflowing it.

Each chain is grown in such a thread to the deepest the library accepts (a deeper one is refused with a `SignalError`
saying it is too deep, at build or at read), then walked there. Each runs in a Python of its own: an overflow kills only
that process, and the test reports the signal that killed it and the walk it was in."""

import importlib.util
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]


def _load_tool():
    spec = importlib.util.spec_from_file_location("check_installed", ROOT / "tools" / "check_installed.py")
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


check_installed = _load_tool()


@pytest.mark.parametrize("chain", check_installed.CHAINS)
def test_the_deepest_chain_the_library_accepts_is_walked_on_the_smallest_thread_stack(chain):
    check_installed.walk_deepest(chain)
