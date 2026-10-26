from concurrent.futures import ThreadPoolExecutor

import numpy as np
import pytest
from hypothesis import given
from hypothesis import strategies as st

from bullterrier_control import Series, SignalError, Unit
from bullterrier_control.rendered import Rendered
from bullterrier_control.signal import Constant, from_series

RATE = 10.0


def integrate(inputs):
    """A causal recurrence: the running sum of the input, in seconds (so a constant 1 integrates to time)."""
    return np.cumsum(inputs) / RATE


class Counting:
    """`integrate`, keeping the length of every input it was asked to run on."""

    def __init__(self) -> None:
        self.calls: list[int] = []

    def __call__(self, inputs):
        self.calls.append(len(inputs))
        return integrate(inputs)


def accumulate(values, *, rate=RATE) -> Rendered:
    return Rendered(from_series(Series(rate=3.0, values=tuple(values), unit=Unit.RATIO)), rate=rate, run=integrate)


def test_a_constant_one_integrates_to_the_time_plus_one_step():
    signal = Rendered(Constant(1.0), rate=RATE, run=integrate)
    # the sum at grid point k includes sample k: (k + 1) / rate, and between grid points the line is straight
    assert signal.sample([0.0, 0.05, 0.3, 2.55]).tolist() == pytest.approx([0.1, 0.15, 0.4, 2.65])


def test_a_time_before_zero_reads_as_time_zero():
    signal = accumulate([1.0, 2.0, 3.0])
    assert signal.sample(-7.0).tolist() == signal.sample(0.0).tolist()


def test_nothing_is_rendered_for_no_times():
    run = Counting()
    signal = Rendered(Constant(1.0), rate=RATE, run=run)
    assert signal.sample([]).shape == (0,)
    assert run.calls == []


def test_reading_far_ahead_and_then_near_gives_the_values_a_near_read_gave_first():
    signal = accumulate([0.5, 1.5, -1.0, 2.0])
    near = signal.sample([0.4, 1.3]).tolist()
    signal.sample(900.0)
    assert signal.sample([0.4, 1.3]).tolist() == near


@given(
    values=st.lists(st.floats(min_value=-5, max_value=5), min_size=1, max_size=8),
    times=st.lists(st.floats(min_value=0, max_value=300), min_size=1, max_size=12),
    seed=st.integers(0, 1000),
)
def test_a_stateful_signal_gives_the_same_values_whatever_the_order_and_chunks_of_the_reads(values, times, seed):
    expected = accumulate(values).sample(times)
    shuffled = np.random.default_rng(seed).permutation(len(times))
    reader = accumulate(values)
    one_at_a_time = np.array([reader.sample(times[index]) for index in shuffled])
    assert one_at_a_time[np.argsort(shuffled)].tolist() == expected.tolist()
    chunked = accumulate(values)
    halves = np.concatenate([chunked.sample(times[: len(times) // 2]), chunked.sample(times[len(times) // 2 :])])
    assert halves.tolist() == expected.tolist()


def test_reading_from_several_threads_gives_the_serial_values():
    times = np.linspace(0, 400, 64)
    expected = accumulate([1.0, -2.0, 0.5]).sample(times)
    shared = accumulate([1.0, -2.0, 0.5])
    chunks = np.array_split(times, 16)
    with ThreadPoolExecutor(8) as pool:
        parts = list(pool.map(shared.sample, chunks[::-1]))  # the far chunks are asked for first
    assert np.concatenate(parts[::-1]).tolist() == expected.tolist()


def test_a_read_that_needs_more_samples_than_the_limit_is_refused_before_anything_is_rendered():
    run = Counting()
    signal = Rendered(Constant(1.0), rate=RATE, run=run, limit=1000)
    with pytest.raises(SignalError, match="limit"):
        signal.sample(1e12)
    assert run.calls == []


def test_the_default_limit_stops_a_read_at_an_absurd_time():
    with pytest.raises(SignalError, match="limit"):
        accumulate([1.0]).sample(1e12)


def test_a_read_just_inside_the_limit_works():
    signal = Rendered(Constant(1.0), rate=RATE, run=integrate, limit=1000)
    assert signal.sample([50.0]).tolist() == pytest.approx([50.1])


def test_a_recurrence_that_changes_the_length_is_a_programming_error_naming_both_lengths():
    signal = Rendered(Constant(1.0), rate=RATE, run=lambda inputs: inputs[:-1])
    with pytest.raises(ValueError, match=r"\d+ values .* \d+"):
        signal.sample(1.0)


@pytest.mark.parametrize("rate", [0.0, -1.0, float("inf"), float("nan")])
def test_a_rate_that_is_not_a_positive_finite_number_is_refused(rate):
    with pytest.raises(ValueError, match="rate"):
        Rendered(Constant(1.0), rate=rate, run=integrate)
