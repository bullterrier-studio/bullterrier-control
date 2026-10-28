"""The Rust extension's boundary, `_native._core`: what the core implements, as the package registers it.

An architecture test: the extension is internal (nothing else of the package reaches it from a test), and only it can
say which shapers the Rust core implements.
"""

from bullterrier_control import registry
from bullterrier_control._native import _core  # noqa: PLC2701  architecture: the shapers the Rust core implements


def test_every_shaper_of_the_rust_core_is_a_registered_shaper():
    assert set(_core.shapers()) <= set(registry.shapers())
