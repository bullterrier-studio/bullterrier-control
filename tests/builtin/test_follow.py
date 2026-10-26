import numpy as np
import pytest
from hypothesis import given, settings
from hypothesis import strategies as st

from bullterrier_control import Series, Unit, lib
from bullterrier_control.signal import Constant, from_series


def held(values: tuple[float, ...]):
    """A signal that holds one of `values` per tenth of a second (a held series at 10 Hz)."""
    return from_series(Series(rate=10.0, values=values, unit=Unit.RATIO), interpolation="hold")


# 0 until 1 s, 1 until 3 s, then 0: both jumps fall exactly on a sample of every internal rate used below
PULSE = held((0.0,) * 10 + (1.0,) * 20 + (0.0,) * 100)
ATTACK, RELEASE = 0.1, 0.25  # distinct, so a swap of the two shows


@pytest.mark.parametrize("rate", [1000.0, 100.0])
@pytest.mark.parametrize("n", [0, 9, 50, 99])
def test_a_rise_follows_the_step_response_of_the_filter_and_sees_the_jump_in_its_own_sample(rate, n):
    # every internal sample closes the share 1 - e^(-1 / (rate * attack)) of the gap, so after n + 1 samples from the
    # jump (the jump's own sample is the first) the gap has shrunk to e^(-(n + 1) / (rate * attack))
    out = lib.follow(PULSE, attack=ATTACK, release=RELEASE, rate=rate)
    assert out.sample([1.0 + n / rate]).tolist() == pytest.approx([1 - np.exp(-(n + 1) / (rate * ATTACK))], abs=1e-12)


@pytest.mark.parametrize("rate", [1000.0, 100.0])
@pytest.mark.parametrize("n", [0, 9, 50, 99])
def test_a_fall_decays_from_where_the_rise_got_to_at_its_own_time_constant(rate, n):
    # the input is 1 for 2 s = 2 * rate samples, so the level reached is 1 - e^(-2 * rate / (rate * attack))
    reached = 1 - np.exp(-2 * rate / (rate * ATTACK))
    out = lib.follow(PULSE, attack=ATTACK, release=RELEASE, rate=rate)
    assert out.sample([3.0 + n / rate]).tolist() == pytest.approx(
        [reached * np.exp(-(n + 1) / (rate * RELEASE))], abs=1e-12
    )


def test_a_long_rise_gets_close_to_the_input_but_does_not_arrive():
    out = lib.follow(PULSE, attack=ATTACK, release=RELEASE)
    # a second after the jump the gap is e^-10 (4.5e-5), two seconds after it e^-20 (2.1e-9)
    assert out.sample([1.999]).tolist() == pytest.approx([1.0 - np.exp(-10.0)], abs=1e-12)
    assert out.sample([2.999]).tolist() == pytest.approx([1.0 - np.exp(-20.0)], abs=1e-12)
    assert out.sample([2.999]).tolist() != [1.0]


@pytest.mark.parametrize("n", [0, 9, 99, 999])
def test_with_equal_times_a_rise_and_a_fall_of_the_same_size_mirror_each_other(n):
    up = held((0.0,) * 10 + (1.0,) * 100)
    down = held((1.0,) * 10 + (0.0,) * 100)
    t = 1.0 + n / 1000.0
    smoothed_up = lib.follow(up, attack=0.2, release=0.2).sample([t])
    smoothed_down = lib.follow(down, attack=0.2, release=0.2).sample([t])
    assert (smoothed_up + smoothed_down).tolist() == pytest.approx([1.0], abs=1e-12)
    assert smoothed_up.tolist() == pytest.approx(
        [1 - np.exp(-(n + 1) / 200)], abs=1e-12
    )  # and each is the analytic response, not 0.5


def test_a_zero_attack_takes_a_rise_at_once_and_a_zero_release_a_fall_at_once():
    assert lib.follow(PULSE, attack=0, release=RELEASE).sample([0.999, 1.0]).tolist() == [0.0, 1.0]
    out = lib.follow(PULSE, attack=ATTACK, release=0)
    assert out.sample([2.999]).tolist() == pytest.approx([1.0], abs=1e-8)
    assert out.sample([3.0]).tolist() == [0.0]


def test_with_both_times_zero_the_output_is_the_input_exactly():
    wobble = held((0.3, 0.7, -0.2, 1.1, 0.5, 0.5, -3.0, 2.25) * 12)
    times = np.arange(0, 9000) / 1000.0
    assert lib.follow(wobble, attack=0, release=0).sample(times).tolist() == wobble.sample(times).tolist()


def test_it_starts_at_the_value_the_input_has_at_time_zero_not_at_zero():
    assert lib.follow(Constant(5.0)).sample([0.0, 1.0, 100.0]).tolist() == [5.0, 5.0, 5.0]
    assert lib.follow(held((5.0, 5.0, 0.0) * 40)).sample([0.0]).tolist() == [5.0]


@settings(max_examples=50)
@given(
    values=st.lists(st.floats(min_value=-5, max_value=5), min_size=2, max_size=12),
    attack=st.floats(min_value=0.0, max_value=3),
    release=st.floats(min_value=0.0, max_value=3),
)
def test_the_output_never_leaves_the_range_of_the_input_read_so_far(values, attack, release):
    signal = from_series(Series(rate=2.0, values=tuple(values), unit=Unit.RATIO), interpolation="hold")
    grid = np.arange(0, 6000) / 1000.0  # the internal samples of the default rate
    out = lib.follow(signal, attack=attack, release=release).sample(grid)
    target = signal.sample(grid)
    # to within float rounding, which can put a mix of two numbers one step beyond them
    assert (out <= np.maximum.accumulate(target) + 1e-12).all()
    assert (out >= np.minimum.accumulate(target) - 1e-12).all()


@pytest.mark.parametrize("n", [0, 9, 49])
def test_by_default_a_rise_closes_the_gap_with_a_time_constant_of_10_ms_at_1000_samples_a_second(n):
    # attack 0.01 s at 1000 Hz: the gap shrinks by e^(-1 / 10) per sample, so by e^(-(n + 1) / 10) after n + 1 of them
    assert lib.follow(PULSE).sample([1.0 + n / 1000.0]).tolist() == pytest.approx(
        [1 - np.exp(-(n + 1) / 10)], abs=1e-12
    )


@pytest.mark.parametrize("n", [0, 249, 999])
def test_by_default_a_fall_closes_the_gap_with_a_time_constant_of_250_ms_at_1000_samples_a_second(n):
    # release 0.25 s at 1000 Hz: the gap shrinks by e^(-1 / 250) per sample
    falling = held((1.0,) * 10 + (0.0,) * 100)
    assert lib.follow(falling).sample([1.0 + n / 1000.0]).tolist() == pytest.approx([np.exp(-(n + 1) / 250)], abs=1e-12)
