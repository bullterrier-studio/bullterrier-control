"""A signal that needs history (a smoother, a slew limiter), read at any time by rendering it once.

    smoothed = Rendered(signal, rate=1000.0, run=lambda inputs: ...)    # run: a causal function of the sampled input

Promises:
- `run` receives the input signal sampled at `rate` samples per second from time 0, and returns an array of the same
  length. It must be causal: output *i* depends only on inputs 0 to *i*. That makes the signal independent of what was
  read before: whatever the order and size of the reads, the same time gives the same value.
- The signal is rendered as far as the largest time read needs, growing by doubling, and read by linear interpolation
  between samples. A read that would need more than `limit` samples raises a `SignalError` before anything is allocated.
- Reading from several threads is safe.
- A time before 0 reads as time 0 (as for every signal).
"""

import math
import threading
from collections.abc import Callable

import numpy as np
from numpy.typing import NDArray

from .errors import SignalError
from .signal import Signal

__all__ = ["Rendered"]

_DEFAULT_LIMIT = 20_000_000  # samples: 160 MB of float64; at 1000 Hz that is about five and a half hours
_FIRST_RENDER = (
    1024  # samples: a first read of a short span renders at least this much, so small reads share one render
)


class Rendered(Signal):
    """A signal made by a causal recurrence `run` over `signal` sampled at `rate`, rendered lazily and cached."""

    def __init__(
        self,
        signal: Signal,
        *,
        rate: float,
        run: Callable[[NDArray[np.float64]], NDArray[np.float64]],
        limit: int = _DEFAULT_LIMIT,
    ) -> None:
        """`rate` is samples per second (positive, finite); `limit` the most samples one read may need."""
        if not (math.isfinite(rate) and rate > 0):
            raise ValueError(f"rate must be a positive finite number of samples per second (got {rate!r})")
        self._signal, self._rate, self._run, self._limit = signal, rate, run, limit
        self._values: NDArray[np.float64] = np.zeros(0)
        self._lock = threading.Lock()

    def compute(self, times: NDArray[np.float64]) -> NDArray[np.float64]:
        """The rendered values at `times`, interpolated linearly between samples."""
        if times.size == 0:
            return np.zeros(times.shape)
        position = times * self._rate
        needed = math.ceil(float(position.max())) + 1  # samples 0 up to the one at or after the last time read
        if needed > self._limit:
            raise SignalError(
                f"reading time {float(times.max())} s needs {needed} samples at {self._rate} Hz,"
                f" over the limit of {self._limit}"
            )
        with self._lock:
            if needed > len(self._values):
                self._render(max(needed, 2 * len(self._values), _FIRST_RENDER))
            values = self._values
        return np.interp(position, np.arange(len(values)), values)

    def _render(self, count: int) -> None:
        count = min(count, self._limit)
        inputs = self._signal.compute(np.arange(count) / self._rate)
        outputs = np.asarray(self._run(inputs), dtype=np.float64)
        if outputs.shape != inputs.shape:
            raise ValueError(
                f"the recurrence returned {outputs.size} values for {inputs.size} inputs; it must return as many"
            )
        self._values = outputs
