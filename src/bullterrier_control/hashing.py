"""A random number that is a pure function of a seed and an index: readable at any index, in any order, the same on
every platform.

    uniform(seed, indices)    # float64 in [0, 1), the shape of `indices` (an integer array, from 0)

Promises:
- `uniform(seed, index)` (the Rust core's `hashing::uniform`; `seed` 0 to 2^64 - 1) is output number `index` (from
  0) of SplitMix64 (Sebastiano Vigna, public domain, prng.di.unimi.it/splitmix64.c) started at state `seed`, as its
  top 53 bits scaled to [0, 1). Integers only, wrapping at 2^64, so no float operation can differ between machines:
  the values are equal everywhere, not merely close.
- The value at an index never depends on which other indices are read with it, or on earlier reads.
- Distinct seeds below 2^32 give unrelated streams (a seed is one step of the stream, which the finaliser scrambles).
"""

from ._native import _core

__all__ = ["uniform"]

uniform = _core.uniform
