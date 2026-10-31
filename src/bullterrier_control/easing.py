"""Easing curves: how progress from 0 to 1 is reshaped, by name or as a cubic Bezier.

    ease = curve("in-out")                          # or "cubic-bezier(0.42, 0, 0.58, 1)"
    ease([0.0, 0.25, 1.0])                          # array([0.    , 0.15625, 1.    ])

The named curves are Robert Penner's easing equations; the Bezier form and its meaning are CSS Easing Level 1
(`cubic-bezier(x1, y1, x2, y2)`: the handles of a curve from (0, 0) to (1, 1)). The curves are the Rust core's
(`easing::Ease`): a Bezier is solved by 50 bisections, every curve gives the same bits on every machine.

Promises:
- `curve(name)` takes a name in `EASES` or a `cubic-bezier(x1, y1, x2, y2)` (whitespace is free; x1 and x2 in 0..1, y1
  and y2 any finite numbers) and returns a function of progress. Anything else raises `InvalidError`, listing the valid
  forms. The name is parsed once, when the curve is made.
- The function is vectorised and keeps the shape of its input: a number or an array of any shape in, float64 out.
- Progress is clipped to 0..1 (a number or an infinity; NaN is undefined). The ends are exact for every curve: 0 gives
  0.0 and 1 gives 1.0.
"""

from ._native import _core

__all__ = ["EASES", "curve"]

EASES = _core.EASES
curve = _core.ease
