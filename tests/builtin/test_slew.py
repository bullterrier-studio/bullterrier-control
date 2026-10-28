import threading
from concurrent.futures import ThreadPoolExecutor

import numpy as np
import pytest
from hypothesis import given
from hypothesis import strategies as st

from bullterrier_control import Series, SignalError, Unit, registry
from bullterrier_control.plugin import Signal
from bullterrier_control.signal import Constant, from_series

slew = registry.get("slew")
RATE = 1000.0  # the default internal rate, in Hz


def step(at: float, before: float, after: float):
    """A signal that is `before` until `at` seconds and `after` from then on (a held series at 10 Hz)."""
    samples = int(at * 10)
    return from_series(
        Series(rate=10.0, values=(before,) * samples + (after,) * 200, unit=Unit.RATIO), interpolation="hold"
    )


def test_a_step_up_rises_at_exactly_one_over_rise_per_second_until_it_arrives():
    out = slew(step(1.0, 0.0, 1.0), rise=2.0, fall=2.0)  # 2 s to climb by 1: 0.5 per second
    # the step is at 1.0 s; half a second later the output is at 0.25, one second later at 0.5, then it arrives at 3.0 s
    assert out.sample([0.9, 1.5, 2.0, 2.5, 3.5, 10.0]).tolist() == pytest.approx(
        [0.0, 0.25, 0.5, 0.75, 1.0, 1.0], abs=2e-3
    )


def test_a_step_down_falls_at_its_own_rate():
    out = slew(step(1.0, 1.0, 0.0), rise=0.1, fall=4.0)  # falls by 0.25 per second
    assert out.sample([0.5, 2.0, 3.0, 5.5, 9.0]).tolist() == pytest.approx([1.0, 0.75, 0.5, 0.0, 0.0], abs=2e-3)


def test_rising_and_falling_can_differ():
    up_and_down = slew(step(1.0, 0.0, 1.0), rise=1.0, fall=10.0)
    assert up_and_down.sample([1.5]).tolist() == pytest.approx([0.5], abs=2e-3)
    down = slew(step(1.0, 1.0, 0.0), rise=1.0, fall=10.0)  # falls 0.1 per second: half a second after the step, 0.95
    assert down.sample([1.5]).tolist() == pytest.approx([0.95], abs=2e-3)


def test_a_zero_time_follows_that_direction_at_once():
    instant_up = slew(step(1.0, 0.0, 1.0), rise=0.0, fall=2.0)
    assert instant_up.sample([0.99, 1.01]).tolist() == pytest.approx([0.0, 1.0], abs=2e-3)


@given(
    values=st.lists(st.floats(min_value=-5, max_value=5), min_size=2, max_size=12),
    rise=st.floats(min_value=0.01, max_value=3),
    fall=st.floats(min_value=0.01, max_value=3),
)
def test_the_output_never_moves_faster_than_the_limits_and_never_overshoots_its_input(values, rise, fall):
    signal = from_series(Series(rate=2.0, values=tuple(values), unit=Unit.RATIO), interpolation="hold")
    out = slew(signal, rise=rise, fall=fall)
    grid = np.arange(0, 2000) / RATE
    follow = out.sample(grid)
    target = signal.sample(grid)
    moves = np.diff(follow)
    assert (moves <= 1.0 / (rise * RATE) + 1e-9).all()
    assert (moves >= -1.0 / (fall * RATE) - 1e-9).all()
    assert (follow <= np.maximum.accumulate(target) + 1e-9).all()  # never above the highest the input has been
    assert (follow >= np.minimum.accumulate(target) - 1e-9).all()


def test_a_slow_input_is_followed_exactly():
    ramp = from_series(Series(rate=1.0, values=(0.0, 1.0, 2.0, 3.0), unit=Unit.RATIO))  # 1 per second
    out = slew(ramp, rise=0.5, fall=0.5)  # may move 2 per second
    times = np.linspace(0, 3, 31)
    assert out.sample(times).tolist() == pytest.approx(ramp.sample(times).tolist(), abs=1e-9)


def test_with_no_limits_the_output_is_the_input_whatever_it_does():
    signal = step(1.0, 0.0, 1.0)
    out = slew(signal, rise=0.0, fall=0.0)
    times = np.linspace(0, 5, 51)
    assert out.sample(times).tolist() == pytest.approx(signal.sample(times).tolist(), abs=1e-9)


def test_it_starts_at_the_value_the_input_has_at_time_zero_not_at_zero():
    assert slew(Constant(5.0), rise=1.0, fall=1.0).sample([0.0, 1.0, 100.0]).tolist() == [5.0, 5.0, 5.0]


def test_a_slower_rate_still_follows_within_one_step_of_the_internal_rate():
    out = slew(step(1.0, 0.0, 1.0), rise=2.0, fall=2.0, rate=10.0)
    assert out.sample([1.5]).tolist() == pytest.approx([0.25], abs=0.1)


def test_threads_released_together_reading_one_slew_get_the_serial_values():
    times = np.linspace(0, 400, 64)
    serial = slew(step(1.0, 0.0, 1.0), rise=2.0, fall=2.0).sample(times)
    shared = slew(step(1.0, 0.0, 1.0), rise=2.0, fall=2.0)
    chunks = np.array_split(times, 8)
    start = threading.Barrier(len(chunks))

    def read(chunk):
        start.wait()
        return shared.sample(chunk)

    with ThreadPoolExecutor(len(chunks)) as pool:
        parts = list(pool.map(read, chunks[::-1]))  # the far chunks are asked for first
    assert np.concatenate(parts[::-1]).tolist() == serial.tolist()


class BrokenAfterOne(Signal):
    """1 until 1 s, then NaN: a broken upstream signal (a Series cannot hold a NaN; a signal's `compute` can)."""

    def compute(self, times):
        return np.where(times < 1.0, 1.0, np.nan)


def test_a_nan_in_its_input_makes_a_read_there_a_signal_error_not_a_crash():
    out = slew(BrokenAfterOne(), rise=0.5, fall=0.5)
    assert out.sample([0.5]).tolist() == [1.0]
    with pytest.raises(SignalError, match="not finite"):
        out.sample([1.5])
