"""Easing curves: how progress from 0 to 1 is reshaped, by name or as a cubic Bezier.

    ease = curve("in-out")                          # or "cubic-bezier(0.42, 0, 0.58, 1)"
    ease([0.0, 0.25, 1.0])                          # array([0.    , 0.15625, 1.    ])

The named curves are Robert Penner's easing equations; the Bezier form and its meaning are CSS Easing Level 1
(`cubic-bezier(x1, y1, x2, y2)`: the handles of a curve from (0, 0) to (1, 1)).

Promises:
- `curve(name)` takes a name in `EASES` or a `cubic-bezier(x1, y1, x2, y2)` (whitespace is free; x1 and x2 in 0..1, y1
  and y2 any finite numbers) and returns a function of progress. Anything else raises `InvalidError`, listing the valid
  forms. The name is parsed once, when the curve is made.
- The function is vectorised and keeps the shape of its input: a number or an array of any shape in, float64 out.
- Progress is clipped to 0..1, and the ends are exact for every curve: 0 gives 0.0 and 1 gives 1.0.
"""

import re
from collections.abc import Callable

import numpy as np
from numpy.typing import ArrayLike, NDArray

from .errors import InvalidError

__all__ = ["EASES", "curve"]

type Shape = Callable[[NDArray[np.float64]], NDArray[np.float64]]

# Penner's overshoot constant: it makes the back-out rise about 10 % past the end before settling.
_PENNER_OVERSHOOT = 1.70158

_SHAPES: dict[str, Shape] = {
    "linear": lambda p: p,
    "hold": lambda p: np.where(p < 1.0, 0.0, 1.0),
    "in": lambda p: p**2,
    "out": lambda p: 1 - (1 - p) ** 2,
    "in-out": lambda p: p**2 * (3 - 2 * p),  # smoothstep
    "cubic-in": lambda p: p**3,
    "cubic-out": lambda p: 1 - (1 - p) ** 3,
    "cubic-in-out": lambda p: np.where(p < 0.5, 4 * p**3, 1 - (-2 * p + 2) ** 3 / 2),
    "overshoot": lambda p: 1 + (_PENNER_OVERSHOOT + 1) * (p - 1) ** 3 + _PENNER_OVERSHOOT * (p - 1) ** 2,
}

EASES = tuple(_SHAPES)

_NUMBER = r"\s*([-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?)\s*"
_BEZIER = re.compile(rf"\s*cubic-bezier\({','.join([_NUMBER] * 4)}\)\s*")
_FORMS = f"{', '.join(EASES)}, or cubic-bezier(x1, y1, x2, y2) with x1 and x2 in 0..1 and finite y1 and y2"


def _bezier(name: str) -> Shape | None:
    """The shape for a `cubic-bezier(...)` name, or None when the name is not a valid one."""
    match = _BEZIER.fullmatch(name)
    if match is None:
        return None
    x1, y1, x2, y2 = (float(group) for group in match.groups())
    if not (np.isfinite([y1, y2]).all() and 0.0 <= x1 <= 1.0 and 0.0 <= x2 <= 1.0):
        return None

    def coordinate(a: float, b: float, t: NDArray[np.float64]) -> NDArray[np.float64]:
        return 3 * a * (1 - t) ** 2 * t + 3 * b * (1 - t) * t**2 + t**3

    def shape(p: NDArray[np.float64]) -> NDArray[np.float64]:
        # x(t) rises with t (its handles lie in 0..1), so bisection finds the t whose x is the progress; 50 halvings
        # leave less than 1e-15, below float rounding
        low, high = np.zeros_like(p), np.ones_like(p)
        for _ in range(50):
            middle = (low + high) / 2
            below = coordinate(x1, x2, middle) < p
            low, high = np.where(below, middle, low), np.where(below, high, middle)
        return coordinate(y1, y2, (low + high) / 2)

    return shape


def curve(name: str) -> Callable[[ArrayLike], NDArray[np.float64]]:
    """The easing function for `name`: a name in `EASES` or a `cubic-bezier(x1, y1, x2, y2)`."""
    shape = _SHAPES.get(name) or _bezier(name)
    if shape is None:
        raise InvalidError(f"unknown ease {name!r}: use {_FORMS}")

    def ease(progress: ArrayLike) -> NDArray[np.float64]:
        p = np.clip(np.asarray(progress, dtype=np.float64), 0.0, 1.0)
        # float arithmetic misses the ends by a rounding (overshoot at 1, a bezier at 0), so they are forced
        return np.where(p == 0.0, 0.0, np.where(p == 1.0, 1.0, shape(p)))

    return ease
