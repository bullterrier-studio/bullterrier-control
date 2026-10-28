import threading
from concurrent.futures import ThreadPoolExecutor

import numpy as np
import pytest
from hypothesis import given
from hypothesis import strategies as st

from bullterrier_control import Series, SignalError, Unit
from bullterrier_control.plugin import Signal
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


def test_a_constant_one_integrates_to_the_time_plus_one_step_and_holds_between_samples():
    signal = Rendered(Constant(1.0), rate=RATE, run=integrate)
    # the sum at grid point k includes sample k: (k + 1) / rate; a time between two grid points reads the earlier one
    assert signal.sample([0.0, 0.05, 0.1, 0.15, 0.3, 2.55]).tolist() == pytest.approx([0.1, 0.1, 0.2, 0.2, 0.4, 2.6])


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


def test_a_recurrence_that_changes_the_length_is_a_programming_error_naming_both_shapes():
    signal = Rendered(Constant(1.0), rate=RATE, run=lambda inputs: inputs[:-1])
    with pytest.raises(ValueError, match=r"shape \(1023,\).*shape \(1024,\)"):
        signal.sample(1.0)


@pytest.mark.parametrize("rate", [0.0, -1.0, float("inf"), float("nan")])
def test_a_rate_that_is_not_a_positive_finite_number_is_refused(rate):
    with pytest.raises(ValueError, match="rate"):
        Rendered(Constant(1.0), rate=rate, run=integrate)


def test_a_read_far_beyond_the_first_render_is_still_exact():
    signal = Rendered(Constant(1.0), rate=RATE, run=integrate)
    assert signal.sample([2000.0, 2000.05]).tolist() == pytest.approx([2000.1, 2000.1])


def test_reading_later_and_later_renders_a_few_times_not_once_per_read():
    run = Counting()
    signal = Rendered(Constant(1.0), rate=RATE, run=run)
    for second in range(1, 301):
        signal.sample(float(second))
    assert len(run.calls) <= 12  # growth by doubling: about log2 of the final length, not 300


def test_the_limit_is_exact_a_read_needing_the_limit_works_and_one_more_is_refused():
    signal = Rendered(Constant(1.0), rate=1.0, run=lambda inputs: np.cumsum(inputs), limit=100)
    assert signal.sample([99.0]).tolist() == [100.0]  # samples 0..99: 100 of them
    with pytest.raises(SignalError, match="limit"):
        signal.sample([100.0])


def test_threads_reading_together_render_once_not_once_each():
    run = Counting()
    signal = Rendered(Constant(1.0), rate=RATE, run=run)
    start = threading.Barrier(8)

    def read(_):
        start.wait()
        return signal.sample([40_000.0]).tolist()

    with ThreadPoolExecutor(8) as pool:
        assert all(values == pytest.approx([40_000.1]) for values in pool.map(read, range(8)))
    assert len(run.calls) == 1


def test_a_read_never_depends_on_the_input_after_the_time_read():
    steps = from_series(Series(rate=RATE, values=(0.0,) * 10 + (1.0,) * 50, unit=Unit.RATIO), interpolation="hold")
    follow = Rendered(steps, rate=RATE, run=lambda inputs: inputs.copy())
    # the input steps up at 1.0 s; a read at 0.95 or 0.99 must still see 0, however close the step is
    assert follow.sample([0.0, 0.5, 0.95, 0.99]).tolist() == [0.0, 0.0, 0.0, 0.0]
    assert follow.sample([1.0, 1.04, 1.5]).tolist() == [1.0, 1.0, 1.0]


def test_a_read_at_a_time_too_large_to_count_samples_for_is_a_signal_error_not_an_overflow():
    signal = Rendered(Constant(1.0), rate=RATE, run=integrate)
    for time in (1.7e306, 1e308, 1.797e308):
        with pytest.raises(SignalError, match="limit"):
            signal.sample(time)


@pytest.mark.parametrize("bad", [True, 0, -3], ids=["a bool", "zero", "negative"])
def test_a_limit_that_is_not_a_positive_whole_number_of_samples_is_refused(bad):
    with pytest.raises(ValueError, match="limit"):
        Rendered(Constant(1.0), rate=RATE, run=integrate, limit=bad)


def test_a_rate_that_is_a_bool_is_refused():
    with pytest.raises(ValueError, match="rate"):
        Rendered(Constant(1.0), rate=True, run=integrate)


class Narrow(Signal):
    """Its times as float32, every other value of a wider array: a strided float32 view, which `compute` may return."""

    def compute(self, times):
        return np.repeat(times, 2).astype(np.float32)[::2]


def test_the_recurrence_always_gets_a_1d_contiguous_float64_array_whatever_compute_returned():
    seen = []

    def run(inputs):
        seen.append((inputs.dtype, inputs.ndim, inputs.flags.c_contiguous))
        return inputs.copy()

    assert Rendered(Narrow(), rate=RATE, run=run).sample([0.5, 2.0]).tolist() == [0.5, 2.0]
    assert seen == [(np.dtype(np.float64), 1, True)]


class Misaligned(Signal):
    """Its times in float64 read from a byte buffer at an odd offset: C-contiguous, but not aligned, which a plugin's
    `compute` may legally return (a view of bytes it read from a file or the network)."""

    def __init__(self) -> None:
        self.returned: list[np.ndarray] = []

    def compute(self, times):
        raw = bytearray(times.size * 8 + 1)
        view = np.frombuffer(raw, dtype=np.float64, offset=1, count=times.size)
        view[:] = times
        self.returned.append(view)
        return view


def test_the_recurrence_gets_a_fresh_aligned_owned_array_even_if_compute_returned_a_misaligned_view():
    signal = Misaligned()
    seen = []

    def run(inputs):
        shared = any(np.shares_memory(inputs, returned) for returned in signal.returned)
        seen.append((inputs.flags.aligned, inputs.flags.c_contiguous, inputs.flags.owndata, shared))
        return inputs.copy()

    Rendered(signal, rate=RATE, run=run).sample([0.5])
    assert seen == [(True, True, True, False)]


def test_a_recurrence_that_returns_the_wrong_shape_is_named_with_both_shapes():
    signal = Rendered(Constant(1.0), rate=RATE, run=lambda inputs: np.zeros((len(inputs), 1)))
    with pytest.raises(ValueError, match=r"shape \(\d+, 1\).*shape \(\d+,\)"):
        signal.sample(1.0)
