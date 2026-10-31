"""A signal that needs history (a smoother, a slew limiter), read at any time by rendering it once.

    smoothed = Rendered(signal, rate=1000.0, run=lambda inputs: ...)    # run: a causal function of the sampled input

The history is the Rust core's (`_native._core.Rendered`): it is kept in Rust and read without the GIL; only `run`
(and the input, if it is a plugin's) calls back into Python.

Promises:
- `rate` is samples per second (positive, finite); `limit` the most samples one read may need (a whole number from
  1, default 20 000 000). Anything else is a `ValueError`.
- `run` receives the input signal sampled at `rate` samples per second from time 0, always as a fresh 1-D,
  C-contiguous float64 array it owns (whatever the input's `compute` returned, and whoever else holds that), and
  returns an array of the same length (another shape is a `ValueError` naming both). It must be causal: output *i*
  depends only on inputs 0 to *i*. That makes the signal independent of what was read before: whatever the order and
  size of the reads, the same time gives the same value. An exception it raises reaches the caller of `sample`.
- The signal is rendered as far as the largest time read needs, growing by doubling. A time between two samples reads
  the earlier one, so a read at time t depends on the input only up to t (a float rounding of a billionth of a sample
  aside). A read that would need more than `limit` samples raises a `SignalError` before anything is allocated.
- Reading from several threads is safe.
- A time before 0 reads as time 0 (as for every signal).
"""

from ._native import _core
from .signal import Signal

__all__ = ["Rendered"]


class Rendered(_core.Rendered, Signal):
    """A signal made by a causal recurrence `run` over `signal` sampled at `rate`, rendered lazily and cached:
    `Rendered(signal, *, rate, run, limit=20_000_000)`."""
