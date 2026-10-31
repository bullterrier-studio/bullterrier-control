"""normalise: scale a series between two of its own percentiles: a measured loudness onto 0 to 1 for the whole song."""

import numpy as np

from ..plugin import Params, Series, Unit, operation


class NormaliseParams(Params, builtin="normalise"):
    """The percentiles of the series that become 0 and 1."""


@operation(params=NormaliseParams)
def normalise(params: NormaliseParams, *, series: Series) -> Series:
    """A `fraction` series of the same start and rate, with the `lower` percentile of the values at 0 and the `upper`
    one at 1, clipped to 0..1 beyond them.

    A value v becomes (v - low) / (high - low), where low and high are the percentiles read by linear interpolation
    between the sorted values (Hyndman and Fan, type 7, the default of numpy.quantile). When the two percentiles are
    equal (a flat series, or a stretch of equal values holding both) every value is 0. The series states how it was
    scaled in its `scaling`.
    """
    values = np.asarray(series.values)
    # scaled by a power of two to within -1..1, which is exact, so no difference below can overflow however large the
    # values are
    _, exponent = np.frexp(np.abs(values).max())
    scaled = np.ldexp(values, -exponent)
    low, high = np.quantile(scaled, [params.lower, params.upper], method="linear")
    if high == low:
        normalised = np.zeros_like(scaled)
    else:
        # a value far above a tiny span divides to beyond the floats; clipping makes it the 1 it should be
        with np.errstate(over="ignore"):
            normalised = np.clip((scaled - low) / (high - low), 0.0, 1.0)
    return Series(
        start=series.start,
        rate=series.rate,
        values=normalised,
        unit=Unit.FRACTION,
        # 12 digits keep any percentage typed and drop the noise of multiplying by 100
        # (0.29 * 100 is 28.999999999999996)
        scaling=f"percentile {params.lower * 100:.12g} to {params.upper * 100:.12g} of the series, clipped to 0..1",
    )
