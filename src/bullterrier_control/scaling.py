"""Putting a value from 0 to 1 onto a range: the one place a source's `low` and `high` are applied.

    between(low, high, fraction)    # low + (high - low) * fraction, never outside the range

Promises:
- `fraction` is an array of floats in [0, 1]; the result has its shape. `low` above `high` inverts it.
- The result is within [min(low, high), max(low, high)] exactly: `low + (high - low) * 1` can land an ulp beyond `high`
  (0.3 + (0.9 - 0.3) is 0.9000000000000001), and the range is a promise a consumer relies on (`round(127 * value)`).
"""

import numpy as np
from numpy.typing import NDArray

__all__ = ["between"]


def between(low: float, high: float, fraction: NDArray[np.float64]) -> NDArray[np.float64]:
    """`fraction` (0 to 1) as a value from `low` to `high`, kept exactly inside them."""
    return np.clip(low + (high - low) * fraction, min(low, high), max(low, high))
