"""What a signal is, how it is read, and how signals combine.

    ramp = from_series(series)                      # a Series read at any time
    both = ramp * 0.5 + Constant(0.25)              # signals combine with arithmetic
    values = both.sample([0.0, 12.4, 100.0])        # values at times in seconds (a number or an array of any shape)

A signal is a graph of the Rust core (`_native._core.SignalBase` holds it): reading one made of the package's signals
runs in Rust, without the GIL. A plugin's signal is a subclass implementing `compute`, read through a call back.

Promises:
- A signal has a value at every time. A time before 0 reads as time 0; a signal made from a Series holds its first and
  last value beyond its ends.
- `sample(times)` returns float64 values in the shape of `times`. It refuses a time that is not a finite number (text,
  a bool, NaN, an infinity) and a value that is not finite (an overflow) with a `SignalError`. The same times give the
  same values, whatever was read before.
- A signal is written by implementing `compute(times)`, which is only ever called with a 1-D float64 array of finite
  times from 0 up, and returns as many values (a `compute` written for times of any shape works). An exception it
  raises reaches the caller of `sample` unchanged. A subclass that does not implement it is a `TypeError` naming it
  when read.
- `signal.pipe(shaper, **params)` is `shaper(signal, **params)` and must give a signal (`TypeError` naming the
  callable if not): any registered shaper (`lib.clip`) or plain function shapes a signal, and the result can be piped
  again.
- Signals combine with `+`, `-`, `*` and negation (a number may stand for a constant signal) and with `minimum` and
  `maximum`; the result is a signal that can be combined again. A crossfade is `a * (1 - m) + b * m`.
"""

from collections.abc import Callable, Sequence
from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal, cast

import numpy as np
from numpy.typing import NDArray

from ._chain import Chain
from ._native import _core
from .formats import Series

__all__ = ["Constant", "Signal", "connect_shapers", "from_series", "from_values", "maximum", "minimum"]


def _not_connected(name: str) -> Callable[..., "Signal"]:
    raise RuntimeError("no shaper lookup connected: import bullterrier_control (its registry connects it)")


@dataclass
class _Shapers:
    """The lookup that finds a shaper by name, connected by the registry (this module cannot import it)."""

    find: Callable[[str], Callable[..., "Signal"]] = _not_connected


_SHAPERS = _Shapers()


def connect_shapers(lookup: Callable[[str], Callable[..., "Signal"]]) -> None:
    """Connect the function that returns the registered shaper called a name (raising for an unknown name or a name
    that is not a shaper). Called once by `bullterrier_control.registry` when it is imported; nothing else calls it."""
    _SHAPERS.find = lookup


class Signal(_core.SignalBase, Chain["Signal"]):
    """A value that changes over time, readable at any moment in seconds. Subclasses implement `compute`."""

    def pipe(self, shaper: Callable[..., "Signal"], /, **params: object) -> "Signal":
        """`shaper(self, **params)`: shape this signal with a registered shaper (`lib.clip`) or any function that takes
        a signal and keywords and returns a signal. A result that is not a signal is a `TypeError` naming `shaper`."""
        shaped = shaper(self, **params)
        if not isinstance(shaped, Signal):
            raise TypeError(f"{getattr(shaper, '__name__', shaper)!r} gave {type(shaped).__name__}, not a Signal")
        return shaped

    def _apply(self, name: str, /, **params: object) -> "Signal":
        """The registered shaper `name` applied to this signal with `params`. Only the generated chain methods call it
        (hence the underscore); `pipe` is the public way."""
        return _SHAPERS.find(name)(self, **params)

    if TYPE_CHECKING:  # the extension makes its results as `Signal` (`make_signals_as` below): typed as such here

        def __add__(self, other: "_core.SignalBase | float", /) -> "Signal": ...
        def __radd__(self, other: "_core.SignalBase | float", /) -> "Signal": ...
        def __sub__(self, other: "_core.SignalBase | float", /) -> "Signal": ...
        def __rsub__(self, other: "_core.SignalBase | float", /) -> "Signal": ...
        def __mul__(self, other: "_core.SignalBase | float", /) -> "Signal": ...
        def __rmul__(self, other: "_core.SignalBase | float", /) -> "Signal": ...
        def __neg__(self) -> "Signal": ...


_core.make_signals_as(Signal)


class Constant(_core.Constant, Signal):
    """The same finite value at every time: `Constant(value)`, `value` a finite number (else `SignalError`)."""


def from_series(series: Series, *, interpolation: Literal["linear", "hold"] = "linear") -> Signal:
    """The signal that reads `series`: sample *i* is at `start + i / rate` seconds, the first and last values hold
    beyond the ends, and between samples the value is interpolated `linear`ly or `hold`s the earlier sample's."""
    return cast("Signal", _core.from_series(series, interpolation=interpolation))


def from_values(
    values: Sequence[float] | NDArray[np.float64],
    rate: float,
    *,
    start: float = 0.0,
    interpolation: Literal["linear", "hold"] = "linear",
) -> Signal:
    """The signal that reads `values` as plain ratios, sample *i* at `start + i / rate` seconds (`from_series` of a
    `Series` in unit `ratio`). A `FormatError` names the field that breaks a `Series` rule."""
    return cast("Signal", _core.from_values(values, rate, start=start, interpolation=interpolation))


def minimum(first: Signal | float, second: Signal | float) -> Signal:
    """The smaller of two signals (or numbers) at every time."""
    return cast("Signal", _core.minimum(first, second))


def maximum(first: Signal | float, second: Signal | float) -> Signal:
    """The larger of two signals (or numbers) at every time."""
    return cast("Signal", _core.maximum(first, second))
