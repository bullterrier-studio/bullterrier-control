"""The Rust core's home in the package: `_core` here is the extension built from crates/python, which runs the shapers
implemented in Rust (`_core.pyi` is its stub).

Internal, and a leaf: it imports nothing from the rest of the package, so every layer may use it. Callers write
`from bullterrier_control._native import _core`; the import linter sees this package where it cannot see a compiled
module.
"""
