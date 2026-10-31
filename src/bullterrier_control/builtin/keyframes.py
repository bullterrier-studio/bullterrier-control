"""keyframes: values at chosen times, with an ease between each two."""

import numpy as np
from numpy.typing import NDArray

from ..plugin import Keyframe, Params, Signal, curve, source


class KeyframesParams(Params, builtin="keyframes"):
    """Keyframes have no parameters: the points, with their eases, say everything."""


class _Keyframed(Signal):
    """The signal through the points, with the ends held."""

    def __init__(self, points: tuple[Keyframe, ...]) -> None:
        # A read falls in one of n + 1 segments for n points: segment s lies between point s - 1 and point s. The first
        # and the last are flat, so the first value holds before the first point and the last value from the last
        # point on, and no read needs a branch for either end. A flat segment is given a span of 1 to divide by.
        values = np.array([point.value for point in points])
        self._starts = np.array([points[0].t, *(point.t for point in points)])
        self._span = np.array([1.0, *np.diff(self._starts[1:]), 1.0])
        self._from = np.concatenate([values[:1], values])
        self._to = np.concatenate([values, values[-1:]])
        # the ease of the segment that starts at a point; the last point's is ignored
        ease_names = ["linear", *(point.ease for point in points[:-1]), "linear"]
        distinct, self._ease_of_segment = np.unique(ease_names, return_inverse=True)
        self._curves = [curve(name) for name in distinct]

    def compute(self, times: NDArray[np.float64]) -> NDArray[np.float64]:
        # "right": a read exactly at a point belongs to the segment that starts there, so it gives that point's value
        segment = np.searchsorted(self._starts[1:], times, side="right")
        progress = (times - self._starts[segment]) / self._span[segment]
        # one call per distinct ease, not per point: a curve can be a Bezier solved by iteration. That costs one mask
        # over the reads per distinct ease: fine until there are thousands of distinct eases.
        eased = np.empty_like(progress)
        ease_of_read = self._ease_of_segment[segment]
        for code, ease in enumerate(self._curves):
            chosen = ease_of_read == code
            eased[chosen] = ease(progress[chosen])
        # a + (b - a) * e would overflow for values near the float limit; (1 - e) * a + e * b is exact at e = 0 and
        # e = 1 but not when a == b (a held value must read back exactly), so that case is taken as it is
        start, end = self._from[segment], self._to[segment]
        return np.where(start == end, start, (1 - eased) * start + eased * end)


@source(params=KeyframesParams)
def keyframes(params: KeyframesParams, *, points: tuple[Keyframe, ...]) -> Signal:
    """A signal that passes through the given values at the given times, easing from each point to the next.

    Before the first point it holds the first value, from the last point on the last value.
    """
    return _Keyframed(points)
