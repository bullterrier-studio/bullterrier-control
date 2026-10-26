"""What a signal is, how it is read, and how signals combine.

    ramp = from_series(series)                      # a Series read at any time
    both = ramp * 0.5 + Constant(0.25)              # signals combine with arithmetic
    values = both.sample([0.0, 12.4, 100.0])        # values at times in seconds (a number or an array of any shape)

Promises:
- A signal has a value at every time. A time before 0 reads as time 0; a signal made from a Series holds its first and
  last value beyond its ends.
- `sample(times)` returns float64 values in the shape of `times`. It refuses a time that is not a finite number with a
  `SignalError`. The same times give the same values, whatever was read before.
- A signal is written by implementing `compute(times)`, which is only ever called with finite times from 0 up, as a
  float64 array, and returns values of the same shape.
- Signals combine with `+`, `-`, `*` and negation (a number may stand for a constant signal) and with `minimum` and
  `maximum`; the result is a signal that can be combined again. A crossfade is `a * (1 - m) + b * m`.
"""

import math
from abc import ABC, abstractmethod
from collections.abc import Callable
from typing import Literal

import numpy as np
from numpy.typing import ArrayLike, NDArray

from .errors import SignalError
from .formats.series import Series

__all__ = ["Constant", "Signal", "from_series", "maximum", "minimum"]

# float rounding of (time - start) * rate: a time that is a sample time to within this share of a sample is that sample
_ROUNDING = 1e-9


class Signal(ABC):
    """A value that changes over time, readable at any moment in seconds. Subclasses implement `compute`."""

    def sample(self, times: ArrayLike) -> NDArray[np.float64]:
        """The values at `times` (a number, or an array of any shape; the result has its shape)."""
        try:
            moments = np.asarray(times, dtype=np.float64)
        except (TypeError, ValueError):
            raise SignalError(f"times must be numbers (got {times!r})") from None
        if not np.isfinite(moments).all():
            raise SignalError(f"times must be finite (got {moments[~np.isfinite(moments)].tolist()})")
        return np.asarray(self.compute(np.maximum(moments, 0.0)), dtype=np.float64)

    @abstractmethod
    def compute(self, times: NDArray[np.float64]) -> NDArray[np.float64]:
        """The values at `times`: finite, from 0 up, float64, any shape. Return an array of the same shape."""

    def __add__(self, other: "Signal | float") -> "Signal":
        return _combined(np.add, self, other)

    def __radd__(self, other: "Signal | float") -> "Signal":
        return _combined(np.add, other, self)

    def __sub__(self, other: "Signal | float") -> "Signal":
        return _combined(np.subtract, self, other)

    def __rsub__(self, other: "Signal | float") -> "Signal":
        return _combined(np.subtract, other, self)

    def __mul__(self, other: "Signal | float") -> "Signal":
        return _combined(np.multiply, self, other)

    def __rmul__(self, other: "Signal | float") -> "Signal":
        return _combined(np.multiply, other, self)

    def __neg__(self) -> "Signal":
        return _combined(np.negative, self)


class Constant(Signal):
    """The same finite value at every time."""

    def __init__(self, value: float) -> None:
        """A signal that is `value` everywhere; `value` must be a finite number."""
        if not math.isfinite(value):
            raise SignalError(f"a constant must be finite (got {value!r})")
        self._value = float(value)

    def compute(self, times: NDArray[np.float64]) -> NDArray[np.float64]:
        """`value`, once for every time."""
        return np.full(times.shape, self._value)


class _Combined(Signal):
    def __init__(self, operation: Callable[..., NDArray[np.float64]], operands: tuple[Signal, ...]) -> None:
        self._operation = operation
        self._operands = operands

    def compute(self, times: NDArray[np.float64]) -> NDArray[np.float64]:
        return self._operation(*(operand.compute(times) for operand in self._operands))


class _SeriesSignal(Signal):
    def __init__(self, series: Series, interpolation: Literal["linear", "hold"]) -> None:
        self._start, self._rate, self._interpolation = series.start, series.rate, interpolation
        self._values = np.asarray(series.values, dtype=np.float64)

    def compute(self, times: NDArray[np.float64]) -> NDArray[np.float64]:
        position = (times - self._start) * self._rate
        last = len(self._values) - 1
        if self._interpolation == "linear":
            return np.interp(position, np.arange(last + 1), self._values)
        return self._values[np.clip(np.floor(position + _ROUNDING).astype(np.int64), 0, last)]


def _signal_of(operand: object) -> Signal | None:
    """The operand as a signal: a signal is itself, a real number is a constant, anything else is not an operand."""
    if isinstance(operand, Signal):
        return operand
    if isinstance(operand, int | float) and not isinstance(operand, bool):
        return Constant(operand)
    return None


def _combined(operation: Callable[..., NDArray[np.float64]], *operands: object) -> Signal:
    signals = tuple(_signal_of(operand) for operand in operands)
    if any(signal is None for signal in signals):
        raise TypeError("a signal combines with signals and numbers only")
    return _Combined(operation, tuple(signal for signal in signals if signal is not None))


def minimum(first: Signal | float, second: Signal | float) -> Signal:
    """The smaller of two signals (or numbers) at every time."""
    return _combined(np.minimum, first, second)


def maximum(first: Signal | float, second: Signal | float) -> Signal:
    """The larger of two signals (or numbers) at every time."""
    return _combined(np.maximum, first, second)


def from_series(series: Series, *, interpolation: Literal["linear", "hold"] = "linear") -> Signal:
    """The signal that reads `series`: sample *i* is at `start + i / rate` seconds, the first and last values hold
    beyond the ends, and between samples the value is interpolated `linear`ly or `hold`s the earlier sample's."""
    if interpolation not in ("linear", "hold"):
        raise ValueError(f"interpolation must be 'linear' or 'hold' (got {interpolation!r})")
    return _SeriesSignal(series, interpolation)
