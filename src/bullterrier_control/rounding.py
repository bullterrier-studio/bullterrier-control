"""Deciding at a boundary when times are decimals: a time that is a bound to within float rounding is the bound.

    at_least(0.3 - 0.1, 0.2)      # True: 0.3 - 0.1 is 0.19999999999999998
    more_than(0.1 + 0.2 - 0.3, 0)  # False: that is 5.5e-17, a rounding error, not a time after 0

Promises:
- Times and durations are in seconds. A value within `TOLERANCE` (a nanosecond) of a bound counts as the bound; anything
  farther is on its side of it. Both functions (the Rust core's `rounding`) take a number or an array and give a bool
  or an array of bools.
- A nanosecond is far above the error of adding and subtracting times under a day (the spacing of floats near 1e5 s is
  1.5e-11 s) and far below anything a person or a beat tracker means by a different time, so a bound a decimal lands on
  is held, and one a millionth of a second off is not.
- Time at the edge of the floats is not covered: beyond about 1e7 s the spacing of floats exceeds the tolerance, and
  a comparison is as exact as the numbers are.
- Not for positions in samples: `signal` and `rendered` have a tolerance in shares of a sample for those.
"""

from ._native import _core

__all__ = ["TOLERANCE", "at_least", "more_than"]

TOLERANCE = _core.TOLERANCE  # seconds; see the promises above
at_least = _core.at_least
more_than = _core.more_than
