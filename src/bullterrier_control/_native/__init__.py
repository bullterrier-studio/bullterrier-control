"""The Rust core's home in the package: `_core` here is the extension built from crates/python (`_core.pyi` is its
stub). It holds the signal graph (`SignalBase`, which the package's `Signal` inherits; the shapers implemented in Rust,
through `shape`), the primitives (easing, hashing, scaling, cadence, rounding) and the formats' classes (`Event`,
`Series`, `Grid`, `Bundle`, `Unit`), which the package exports as its own.

Internal, and below every layer but `errors`: the extension imports `bullterrier_control.errors` (its `FormatError`,
`ParameterError`, `SignalError`, ...) when it first raises one, and nothing else of the package, so every layer above
may use it. Callers write `from bullterrier_control._native import _core`; the import linter sees this package where
it cannot see a compiled module.
"""
