import numpy as np
import pytest
from hypothesis import given, settings
from hypothesis import strategies as st

from bullterrier_control import ParameterError, Series, Unit, lib
from bullterrier_control.signal import from_series


def held(values: tuple[float, ...], rate: float = 10.0):
    """A signal that holds one of `values` per 1 / `rate` seconds (a held series)."""
    return from_series(Series(rate=rate, values=values, unit=Unit.RATIO), interpolation="hold")


def test_it_opens_at_open_and_closes_below_close_and_stays_as_it_was_in_between():
    # the defaults: open 0.5, close 0.4. Read in the middle of each tenth of a second.
    level = held((0.3, 0.55, 0.45, 0.42, 0.4, 0.38, 0.5, 0.6))
    middles = (np.arange(8) + 0.5) / 10
    #   0.3: closed, under open
    #   0.55: opens
    #   0.45, 0.42: between close and open, stays open
    #   0.4: equals close, which does not close it
    #   0.38: below close, closes
    #   0.5: equals open, which opens it
    #   0.6: stays open
    assert lib.gate(level).sample(middles).tolist() == [0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0]


def test_a_closed_gate_stays_closed_between_close_and_open():
    level = held((0.3, 0.45, 0.42, 0.49, 0.45, 0.2))
    assert lib.gate(level).sample((np.arange(6) + 0.5) / 10).tolist() == [0.0] * 6


def test_with_open_equal_to_close_it_is_a_plain_threshold_that_includes_the_threshold():
    # x >= 0.5, whatever the state before: 0.5 opens a closed gate and, after 0.9, does not close an open one
    level = held((0.1, 0.5, 0.499, 0.9, 0.5, 0.2))
    out = lib.gate(level, open=0.5, close=0.5).sample((np.arange(6) + 0.5) / 10)
    assert out.tolist() == [0.0, 1.0, 0.0, 1.0, 1.0, 0.0]


def test_a_jump_exactly_on_a_sample_is_seen_by_the_read_at_that_time():
    level = held((0.0,) * 10 + (1.0,) * 10)  # up at exactly 1.0 s, a sample of the default internal rate
    assert lib.gate(level).sample([0.999, 1.0, 1.001]).tolist() == [0.0, 1.0, 1.0]
    falling = held((1.0,) * 10 + (0.0,) * 10)
    assert lib.gate(falling).sample([0.999, 1.0, 1.001]).tolist() == [1.0, 0.0, 0.0]


def test_it_starts_open_if_the_input_is_at_open_at_time_zero():
    assert lib.gate(held((0.5, 0.45, 0.45))).sample([0.0, 0.15, 0.25]).tolist() == [1.0, 1.0, 1.0]
    assert lib.gate(held((0.49, 0.45, 0.45))).sample([0.0, 0.15, 0.25]).tolist() == [0.0, 0.0, 0.0]


def test_the_gate_changes_on_an_internal_sample_so_a_lower_rate_sees_a_jump_later():
    level = held((0.0,) * 5 + (1.0,) * 20, rate=100.0)  # up at 0.05 s, between the samples of 10 Hz (0, 0.1, 0.2, ...)
    # at 10 Hz the first sample at or after the jump is at 0.1 s; a read at 0.05 still holds the sample at 0
    assert lib.gate(level, rate=10.0).sample([0.05, 0.1]).tolist() == [0.0, 1.0]
    # at 1000 Hz 0.05 s is a sample itself
    assert lib.gate(level, rate=1000.0).sample([0.049, 0.05]).tolist() == [0.0, 1.0]


@settings(max_examples=50)
@given(
    values=st.lists(st.floats(min_value=-2, max_value=2), min_size=2, max_size=12),
    open_=st.floats(min_value=-1.5, max_value=1.5),
    width=st.floats(min_value=0, max_value=1.5),
)
def test_the_output_is_two_valued_and_changes_state_only_when_the_input_crosses_the_threshold_of_that_direction(
    values, open_, width
):
    close = open_ - width
    signal = held(tuple(values), rate=5.0)
    grid = np.arange(0, 2400) / 1000.0  # the internal samples of the default rate
    out = lib.gate(signal, open=open_, close=close).sample(grid)
    x = signal.sample(grid)
    assert set(out.tolist()) <= {0.0, 1.0}
    assert out[0] == float(x[0] >= open_)
    before, after, seen = out[:-1], out[1:], x[1:]
    # it opens exactly when it was closed and the input is at or above open, closes exactly when it was open and the
    # input is below close, and otherwise keeps its state
    assert ((before == 0) & (seen >= open_)).tolist() == ((before == 0) & (after == 1)).tolist()
    assert ((before == 1) & (seen < close)).tolist() == ((before == 1) & (after == 0)).tolist()


@pytest.mark.parametrize(
    ("open_", "close"),
    [
        pytest.param(0.5, 0.6, id="close above open"),
        pytest.param(0.5, 0.5000001, id="close just above open"),
        pytest.param(-0.2, -0.1, id="negative levels follow the same rule"),
    ],
)
def test_close_above_open_is_refused_naming_close(open_, close, reports):
    with pytest.raises(ParameterError, match="close") as error:
        lib.gate(held((0.0,)), open=open_, close=close)
    reports(error.value, "close")


def test_by_default_it_changes_on_the_samples_of_1000_hz():
    level = held((0.0,) + (1.0,) * 100, rate=1000.0)  # up at 0.001 s: a sample at 1000 Hz, between those of 500 Hz
    assert lib.gate(level).sample([0.0, 0.0009, 0.001]).tolist() == [0.0, 0.0, 1.0]
