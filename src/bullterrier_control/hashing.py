"""A random number that is a pure function of a seed and an index: readable at any index, in any order, the same on
every platform.

    uniform(seed, indices)    # float64 in [0, 1), the shape of `indices`

Promises:
- `uniform(seed, index)` is output number `index` (from 0) of SplitMix64 (Sebastiano Vigna, public domain,
  prng.di.unimi.it/splitmix64.c) started at state `seed`, as its top 53 bits scaled to [0, 1). Integers only, wrapping
  at 2^64, so no float operation can differ between machines: the values are equal everywhere, not merely close.
- The value at an index never depends on which other indices are read with it, or on earlier reads.
- Distinct seeds below 2^32 give unrelated streams (a seed is one step of the stream, which the finaliser scrambles).
"""

import numpy as np
from numpy.typing import NDArray

__all__ = ["uniform"]

# SplitMix64's constants: the state step (2^64 divided by the golden ratio, rounded to odd) and the finaliser's two
# multipliers and shifts, as published
_STEP = np.uint64(0x9E3779B97F4A7C15)
_FIRST_MULTIPLIER = np.uint64(0xBF58476D1CE4E5B9)
_SECOND_MULTIPLIER = np.uint64(0x94D049BB133111EB)
_TOP_53_BITS = np.uint64(11)  # 64 - 53
_UNIT = 2.0**-53


def uniform(seed: int, index: NDArray[np.uint64]) -> NDArray[np.float64]:
    """The value at each `index` of the stream of `seed`: a float64 in [0, 1) with the shape of `index`."""
    with np.errstate(over="ignore"):  # a product that wraps at 2^64 is the algorithm, not an overflow
        state = np.uint64(seed) + (index + np.uint64(1)) * _STEP
        state = (state ^ (state >> np.uint64(30))) * _FIRST_MULTIPLIER
        state = (state ^ (state >> np.uint64(27))) * _SECOND_MULTIPLIER
        state = state ^ (state >> np.uint64(31))
    return (state >> _TOP_53_BITS).astype(np.float64) * _UNIT
