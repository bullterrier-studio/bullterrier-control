import numpy as np
import pytest
from hypothesis import given
from hypothesis import strategies as st

from bullterrier_control import ParameterError, Series, Unit, registry
from bullterrier_control.signal import from_series

clip = registry.get("clip")


def series_signal(values):
    return from_series(Series(rate=1.0, values=tuple(values), unit=Unit.RATIO))


def test_values_below_low_and_above_high_are_pulled_in_and_the_rest_stay():
    signal = series_signal([-2.0, 0.25, 0.5, 3.0])
    assert clip(signal, low=0.0, high=1.0).sample([0.0, 1.0, 2.0, 3.0]).tolist() == [0.0, 0.25, 0.5, 1.0]


def test_the_defaults_clip_to_zero_and_one():
    assert clip(series_signal([-1.0, 2.0])).sample([0.0, 1.0]).tolist() == [0.0, 1.0]


def test_between_samples_the_clip_applies_to_the_signal_not_to_the_samples():
    # a line from -1 to 3 crosses the range 0..1 between the samples
    assert clip(series_signal([-1.0, 3.0])).sample([0.25, 0.5, 0.75]).tolist() == pytest.approx([0.0, 1.0, 1.0])


def test_low_may_equal_high():
    assert clip(series_signal([0.0, 5.0]), low=2.0, high=2.0).sample([0.0, 0.5, 1.0]).tolist() == [2.0, 2.0, 2.0]


def test_a_low_above_the_high_is_refused_naming_the_field(reports):
    with pytest.raises(ParameterError) as refused:
        clip(series_signal([0.0]), low=1.0, high=0.5)
    reports(refused.value, "high")


@given(
    values=st.lists(st.floats(min_value=-50, max_value=50), min_size=1, max_size=10),
    low=st.floats(min_value=-20, max_value=0),
    high=st.floats(min_value=0, max_value=20),
    times=st.lists(st.floats(min_value=0, max_value=12), min_size=1, max_size=15),
)
def test_the_output_never_leaves_the_range_and_leaves_what_is_inside_unchanged(values, low, high, times):
    original = series_signal(values)
    clipped = clip(original, low=low, high=high).sample(times)
    before = original.sample(times)
    assert ((clipped >= low) & (clipped <= high)).all()
    inside = (before >= low) & (before <= high)
    assert np.array_equal(clipped[inside], before[inside])
