"""A signal that needs history (a smoother, a slew limiter), read at any time by rendering it once.

    smoothed = Rendered(signal, rate=1000.0, run=lambda inputs: ...)    # run: a causal function of the sampled input

Promises:
- `run` receives the input signal sampled at `rate` samples per second from time 0, always as a 1-D, C-contiguous
  float64 array (whatever the input's `compute` returned), and returns an array of the same length. It must be
  causal: output *i* depends only on inputs 0 to *i*. That makes the signal independent of what was read before:
  whatever the order and size of the reads, the same time gives the same value.
- The signal is rendered as far as the largest time read needs, growing by doubling. A time between two samples reads
  the earlier one, so a read at time t depends on the input only up to t (a float rounding of a billionth of a sample
  aside). A read that would need more than `limit` samples raises a `SignalError` before anything is allocated.
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
# samples: a first read of a short span renders at least this much, so small reads share one render
_FIRST_RENDER = 1024
# float rounding of time * rate: a time within a billionth of a sample of a sample time is that sample
_ROUNDING = 1e-9


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
        if isinstance(rate, bool) or not (math.isfinite(rate) and rate > 0):
            raise ValueError(f"rate must be a positive finite number of samples per second (got {rate!r})")
        if isinstance(limit, bool) or not isinstance(limit, int) or limit < 1:
            raise ValueError(f"limit must be a whole number of samples, at least 1 (got {limit!r})")
        self._signal, self._rate, self._run, self._limit = signal, rate, run, limit
        self._values: NDArray[np.float64] = np.zeros(0)
        self._lock = threading.Lock()

    def compute(self, times: NDArray[np.float64]) -> NDArray[np.float64]:
        """The rendered value at each time: that of the latest sample at or before it."""
        if times.size == 0:
            return np.zeros(times.shape)
        largest = float(times.max())
        if largest * self._rate + 1 > self._limit:  # in floats: a time near the top of the range would overflow a count
            raise SignalError(
                f"reading time {largest} s needs more than {self._limit} samples at {self._rate} Hz: over the limit"
            )
        needed = math.floor(largest * self._rate + _ROUNDING) + 1
        with self._lock:
            if needed > len(self._values):
                self._render(max(needed, 2 * len(self._values), _FIRST_RENDER))
            values = self._values
        index = np.floor(times * self._rate + _ROUNDING).astype(np.int64)
        return values[index]

    def _render(self, count: int) -> None:
        count = min(count, self._limit)
        # the one place a recurrence is called: it gets the array its promise names (a float32 or strided `compute`
        # result is copied once, here), so no shaper copes with another and the Rust core reads it in place
        inputs = np.ascontiguousarray(self._signal.compute(np.arange(count) / self._rate), dtype=np.float64)
        outputs = np.asarray(self._run(inputs), dtype=np.float64)
        if outputs.shape != inputs.shape:
            raise ValueError(
                f"the recurrence returned an array of shape {outputs.shape} for an input of shape {inputs.shape}"
            )
        self._values = outputs
