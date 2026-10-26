import numpy as np
import pytest
from hypothesis import given
from hypothesis import strategies as st

from bullterrier_control import SignalError, lib
from bullterrier_control.plugin import uniform


def lattice(seed, ks, low=0.0, high=1.0):
    """The random value at each whole position k: the hashing stream of the seed put on low..high."""
    return (low + (high - low) * uniform(seed, np.asarray(ks, dtype=np.uint64))).tolist()


# rate 4 a second with low -1 and high 3: a position k + f is at time (k + f) / 4, exact in floats for f = 0, 0.25, ...
K = [0, 1, 2, 3, 7, 40]


def at(fraction):
    return [(k + fraction) / 4 for k in K]


@pytest.mark.parametrize("fraction", [0.0, 0.25, 0.5, 0.999], ids=str)
def test_hold_reads_the_value_of_the_lattice_point_it_has_passed(fraction):
    signal = lib.noise(shape="hold", rate=4, seed=9, low=-1, high=3)
    assert signal.sample(at(fraction)).tolist() == pytest.approx(lattice(9, K, -1, 3), abs=1e-12)


def test_linear_passes_through_the_lattice_values_and_is_straight_between_them():
    signal = lib.noise(shape="linear", rate=4, seed=9, low=-1, high=3)
    here, there = np.array(lattice(9, K, -1, 3)), np.array(lattice(9, [k + 1 for k in K], -1, 3))
    assert signal.sample(at(0.0)).tolist() == pytest.approx(here.tolist(), abs=1e-12)
    assert signal.sample(at(0.25)).tolist() == pytest.approx((here + 0.25 * (there - here)).tolist(), abs=1e-12)
    assert signal.sample(at(0.5)).tolist() == pytest.approx(((here + there) / 2).tolist(), abs=1e-12)


def test_smooth_eases_between_the_lattice_values_by_3x_squared_minus_2x_cubed():
    signal = lib.noise(shape="smooth", rate=4, seed=9, low=-1, high=3)
    here, there = np.array(lattice(9, K, -1, 3)), np.array(lattice(9, [k + 1 for k in K], -1, 3))
    assert signal.sample(at(0.0)).tolist() == pytest.approx(here.tolist(), abs=1e-12)
    # 3 * 0.25^2 - 2 * 0.25^3 = 0.15625; halfway is 0.5; 3 * 0.75^2 - 2 * 0.75^3 = 0.84375
    assert signal.sample(at(0.25)).tolist() == pytest.approx((here + 0.15625 * (there - here)).tolist(), abs=1e-12)
    assert signal.sample(at(0.5)).tolist() == pytest.approx(((here + there) / 2).tolist(), abs=1e-12)
    assert signal.sample(at(0.75)).tolist() == pytest.approx((here + 0.84375 * (there - here)).tolist(), abs=1e-12)


def test_the_default_is_smooth_noise_four_values_a_second_between_0_and_1():
    assert lib.noise().sample(at(0.25)).tolist() == lib.noise(shape="smooth", rate=4, seed=0).sample(at(0.25)).tolist()


def test_smooth_has_no_slope_at_a_lattice_point_where_linear_has_one():
    # seed 0: the values at 0, 1 and 2 are 0.88, 0.43 and 0.03, so a straight line has a slope on both sides of 1
    step = 1e-4
    smooth = lib.noise(shape="smooth", rate=1, seed=0)
    linear = lib.noise(shape="linear", rate=1, seed=0)
    for signal, steep in [(smooth, False), (linear, True)]:
        around = signal.sample([1.0 - step, 1.0, 1.0 + step]).tolist()
        changes = [abs(around[0] - around[1]), abs(around[2] - around[1])]
        assert all((change > 1e-5) == steep for change in changes)


def test_the_noise_is_rate_values_a_second():
    # twice the rate reads the same line in half the time
    slow = lib.noise(rate=3, seed=5, low=-2, high=2).sample([0.1, 0.9, 7.3])
    fast = lib.noise(rate=6, seed=5, low=-2, high=2).sample([0.05, 0.45, 3.65])
    assert fast.tolist() == slow.tolist()


def test_different_seeds_give_different_noise():
    times = np.arange(0.1, 10.0, 0.5)
    first, second = lib.noise(seed=0).sample(times), lib.noise(seed=1).sample(times)
    assert (first != second).all()


def test_a_seed_gives_the_same_noise_each_time_it_is_built():
    times = [0.1, 33.3, 1e6]
    assert lib.noise(seed=77).sample(times).tolist() == lib.noise(seed=77).sample(times).tolist()


def test_the_values_are_spread_evenly_over_the_range():
    # one value per lattice point, read in the middle of each: the mean of a uniform distribution
    middles = (np.arange(100_000) + 0.5) / 4
    values = lib.noise(shape="hold", rate=4, seed=2, low=-1, high=3).sample(middles)
    assert values.mean() == pytest.approx(1.0, abs=0.02)
    assert values.min() < -0.99
    assert values.max() > 2.99


def test_low_above_high_flips_the_noise():
    flipped = lib.noise(seed=4, low=3, high=-1).sample(at(0.3))
    assert flipped.tolist() == pytest.approx(
        (2.0 - np.array(lib.noise(seed=4, low=-1, high=3).sample(at(0.3)))).tolist()
    )


@pytest.mark.parametrize("seed", [0, 4294967295], ids=str)
@pytest.mark.parametrize("shape", ["hold", "linear", "smooth"])
def test_the_noise_stays_in_range_at_any_time_up_to_a_billion_seconds_at_the_ends_of_the_seed(seed, shape):
    signal = lib.noise(shape=shape, rate=1000, seed=seed, low=-1, high=3)
    values = signal.sample(np.concatenate([np.linspace(0, 100, 5000), [1e6, 12345.6789, 1e9 - 0.001, 1e9]]))
    assert values.min() >= -1.0
    assert values.max() <= 3.0


def test_a_position_beyond_a_whole_machine_integer_still_reads_the_noise():
    # 1e300 at 4 a second: a whole position, so the value is the lattice point's, in range
    value = lib.noise(shape="smooth", seed=3).sample([1e300]).item()
    assert 0.0 <= value <= 1.0


def test_a_time_too_far_for_the_position_to_be_a_number_is_a_signal_error():
    with pytest.raises(SignalError):
        lib.noise(rate=1000).sample([1.797e308])


@given(
    shape=st.sampled_from(["hold", "linear", "smooth"]),
    rate=st.floats(0.001, 1000),
    seed=st.integers(0, 4294967295),
    low=st.floats(-1e6, 1e6),
    high=st.floats(-1e6, 1e6),
    time=st.floats(0, 1e300),
)
def test_the_output_never_leaves_low_and_high_whichever_way_round(shape, rate, seed, low, high, time):
    value = lib.noise(shape=shape, rate=rate, seed=seed, low=low, high=high).sample(time).item()
    assert min(low, high) <= value <= max(low, high)
