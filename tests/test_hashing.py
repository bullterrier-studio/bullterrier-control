import numpy as np
import pytest

from bullterrier_control.plugin import uniform

MASK = (1 << 64) - 1
# SplitMix64, from Sebastiano Vigna's public-domain C reference (prng.di.unimi.it/splitmix64.c), written out in
# plain Python integers: the oracle for the vectorised version
GAMMA = 0x9E3779B97F4A7C15


def reference_output(seed: int, index: int) -> int:
    """Output number `index` (from 0) of Vigna's generator started at state `seed`."""
    z = (seed + (index + 1) * GAMMA) & MASK
    z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK
    z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK
    return z ^ (z >> 31)


def top_53_bits(output: int) -> float:
    return (output >> 11) / 2**53


def indices(*values: int) -> np.ndarray:
    return np.array(values, dtype=np.uint64)


# The first outputs of splitmix64 from state 0, as published with the algorithm and in its common test vectors
# (the same four are the first values `next()` returns in the reference C code for `x = 0`)
SEED_0_OUTPUTS = [0xE220A8397B1DCDAF, 0x6E789E6AA1B965F4, 0x06C45D188009454F, 0xF88BB8A8724C81EC]


def test_seed_0_starts_with_the_published_splitmix64_outputs():
    assert uniform(0, indices(0, 1, 2, 3)).tolist() == [top_53_bits(output) for output in SEED_0_OUTPUTS]


def test_the_reference_in_this_file_gives_the_published_outputs_too():
    assert [reference_output(0, index) for index in range(4)] == SEED_0_OUTPUTS


@pytest.mark.parametrize("seed", [0, 1, 7, 4294967295], ids=str)
def test_every_value_is_the_top_53_bits_of_the_reference_output(seed):
    picked = [0, 1, 2, 999, 10**6, 2**32, 2**53, 2**63, 2**64 - 2048]
    expected = [top_53_bits(reference_output(seed, index)) for index in picked]
    assert uniform(seed, np.array(picked, dtype=np.uint64)).tolist() == expected


def test_a_value_depends_on_the_seed_and_the_index_alone():
    alone = uniform(5, indices(40)).tolist()
    among_others = uniform(5, indices(3, 40, 9, 40)).tolist()
    assert among_others[1] == among_others[3] == alone[0]
    assert uniform(5, indices(9, 40, 3)).tolist() == [among_others[2], among_others[1], among_others[0]]


def test_the_shape_of_the_indices_is_kept_and_a_single_one_works():
    grid = np.arange(12, dtype=np.uint64).reshape(3, 4)
    assert uniform(2, grid).shape == (3, 4)
    assert uniform(2, grid).ravel().tolist() == uniform(2, grid.ravel()).tolist()
    assert uniform(2, np.array(7, dtype=np.uint64)).shape == ()


def test_different_seeds_give_different_values_at_every_index():
    first = uniform(0, np.arange(1000, dtype=np.uint64))
    second = uniform(1, np.arange(1000, dtype=np.uint64))
    assert (first != second).all()


MILLION = np.arange(10**6, dtype=np.uint64)


def test_values_are_in_zero_to_one_with_the_mean_and_variance_of_a_uniform_distribution():
    values = uniform(3, MILLION)
    assert values.min() >= 0.0
    assert values.max() < 1.0
    assert values.mean() == pytest.approx(0.5, abs=0.01)
    assert values.var() == pytest.approx(1 / 12, abs=0.005)


def test_no_value_repeats_among_a_hundred_thousand_indices():
    assert len(np.unique(uniform(11, MILLION[:100_000]))) == 100_000
