"""The extension's primitives (`ease` and `EASES`, `uniform`, `at_least`, `more_than`,
`TOLERANCE`): what the package's easing, hashing and rounding modules are built on.

An architecture test of the extension's own interface; every expected number is worked out by hand or taken from
the algorithm's published reference.
"""

import numpy as np
import pytest

from bullterrier_control._native import _core  # noqa: PLC2701  architecture: the boundary the package's helpers are
from bullterrier_control.errors import InvalidError

# ---- ease ----


def test_the_named_eases_are_the_nine_curves():
    assert _core.EASES == (
        "linear", "hold", "in", "out", "in-out", "cubic-in", "cubic-out", "cubic-in-out", "overshoot"
    )  # fmt: skip


@pytest.mark.parametrize(
    ("name", "progress", "expected"),
    [
        pytest.param("linear", 0.25, 0.25, id="linear is the identity"),
        pytest.param("hold", 0.999, 0.0, id="hold stays at 0 until the end"),
        pytest.param("in", 0.5, 0.25, id="in squares"),
        pytest.param("out", 0.5, 0.75, id="out: 1 - (1 - p)^2"),
        pytest.param("in-out", 0.25, 0.15625, id="in-out is smoothstep"),
        pytest.param("cubic-in", 0.5, 0.125, id="cubic-in cubes"),
        pytest.param("cubic-out", 0.5, 0.875, id="cubic-out: 1 - (1 - p)^3"),
        pytest.param("cubic-in-out", 0.25, 0.0625, id="cubic-in-out below the middle: 4 p^3"),
        pytest.param("cubic-in-out", 0.75, 0.9375, id="cubic-in-out above the middle: 1 - (2 - 2p)^3 / 2"),
        pytest.param("overshoot", 0.5, 1.0876975, id="overshoot passes 1"),
        pytest.param("in", -1.0, 0.0, id="progress before 0 is 0"),
        pytest.param("overshoot", 2.0, 1.0, id="progress after 1 is 1"),
        pytest.param(" cubic-bezier( 0, 0 ,1, 1 ) ", 0.3, 0.3, id="a bezier on the diagonal is linear"),
    ],
)
def test_an_ease_reshapes_progress(name, progress, expected):
    assert _core.ease(name)(progress).tolist() == pytest.approx(expected, abs=1e-12)


def test_an_ease_keeps_the_shape_of_its_progress_and_its_ends_exact():
    bezier = _core.ease("cubic-bezier(0.42, 0, 0.58, 1)")
    eased = bezier(np.array([[0.0, 1.0], [0.5, 1.0]]))
    assert eased.dtype == np.float64
    assert eased.shape == (2, 2)
    assert eased[0].tolist() == [0.0, 1.0]
    assert eased[1, 0] == pytest.approx(0.5, abs=1e-12)  # a symmetric curve passes its middle
    assert bezier(0.0).shape == ()


@pytest.mark.parametrize(
    ("name", "named"),
    [
        pytest.param("wobble", "unknown ease 'wobble'", id="an unknown name"),
        pytest.param("cubic-bezier(1.5, 0, 0.5, 1)", "x1 and x2 must be in 0..1", id="a handle beyond 0..1"),
        pytest.param("cubic-bezier(0, nan, 1, 1)", "unknown ease", id="a y that is not a number"),
        pytest.param("IN", "unknown ease", id="names are lowercase"),
    ],
)
def test_a_text_that_names_no_ease_is_an_invalid_error_listing_the_forms(name, named):
    with pytest.raises(InvalidError, match=named) as raised:
        _core.ease(name)
    assert "cubic-bezier(x1, y1, x2, y2)" in str(raised.value)


# ---- uniform ----

# the first three outputs of SplitMix64 from state 0, as published (prng.di.unimi.it/splitmix64.c), top 53 bits
SPLITMIX64_FROM_0 = [0xE220A8397B1DCDAF, 0x6E789E6AA1B965F4, 0x06C45D188009454F]


def test_uniform_is_splitmix64_scaled_to_the_unit_interval_in_the_shape_of_the_indices():
    expected = [(output >> 11) * 2.0**-53 for output in SPLITMIX64_FROM_0]
    assert _core.uniform(0, np.arange(3, dtype=np.uint64)).tolist() == expected
    shaped = _core.uniform(0, np.array([[2], [0]], dtype=np.uint64))
    assert shaped.shape == (2, 1)
    assert shaped.tolist() == [[expected[2]], [expected[0]]]
    # a seed is a state: one step (SplitMix64's 0x9E3779B97F4A7C15) back from 0, index 1 is seed 0's index 0
    assert _core.uniform(2**64 - 0x9E3779B97F4A7C15, np.array([1], dtype=np.uint64)).tolist() == [expected[0]]


@pytest.mark.parametrize(
    ("seed", "index", "error"),
    [
        pytest.param(-1, np.zeros(1, dtype=np.uint64), OverflowError, id="a negative seed"),
        pytest.param(2**64, np.zeros(1, dtype=np.uint64), OverflowError, id="a seed beyond 64 bits"),
        pytest.param(0, np.zeros(1), TypeError, id="float indices"),
        pytest.param(0, np.array([1, -1]), ValueError, id="a negative index"),
    ],
)
def test_uniform_takes_a_64_bit_seed_and_whole_non_negative_indices(seed, index, error):
    with pytest.raises(error):
        _core.uniform(seed, index)


def test_uniform_takes_the_indices_of_any_integer_dtype_as_the_same_whole_numbers():
    # `np.arange(3)` is int64: the indices a caller has at hand
    expected = _core.uniform(5, np.arange(3, dtype=np.uint64))
    for dtype in (np.int64, np.int32, np.uint8):
        assert _core.uniform(5, np.arange(3, dtype=dtype)).tolist() == expected.tolist()
    assert _core.uniform(5, np.array([[2], [0]])).tolist() == [[expected[2]], [expected[0]]]
    assert _core.uniform(5, np.array([], dtype=np.int64)).shape == (0,)


# ---- at_least, more_than ----


@pytest.mark.parametrize(
    ("decide", "value", "bound", "expected"),
    [
        pytest.param(_core.at_least, 0.3 - 0.1, 0.2, True, id="a rounding error short of the bound reaches it"),
        pytest.param(_core.at_least, -1e-6, 0.0, False, id="a millionth short does not"),
        pytest.param(_core.at_least, 5.0, 5.0, True, id="the bound reaches itself"),
        pytest.param(_core.more_than, 0.1 + 0.2 - 0.3, 0.0, False, id="a rounding error over is not beyond"),
        pytest.param(_core.more_than, 1e-6, 0.0, True, id="a millionth over is"),
        pytest.param(_core.more_than, 5.0, 5.0, False, id="the bound is not beyond itself"),
    ],
)
def test_a_time_within_the_tolerance_of_a_bound_is_the_bound(decide, value, bound, expected):
    assert decide(value, bound) is expected
    assert decide(np.array([[value]]), bound).tolist() == [[expected]]


def test_the_decisions_give_an_array_of_bools_for_an_array():
    decided = _core.at_least(np.array([0.2 - 1e-10, 0.1, 0.2 - 2e-9]), 0.2)
    assert decided.dtype == np.bool_
    assert decided.tolist() == [True, False, False]
    assert _core.TOLERANCE == 1e-9


@pytest.mark.parametrize(
    ("decide", "value", "bound", "expected"),
    [
        pytest.param(_core.at_least, np.array([0.1, 0.5]), np.array([0.2, 0.2]), [False, True], id="paired"),
        pytest.param(_core.at_least, np.array([0.1, 0.5]), np.float64(0.2), [False, True], id="a numpy scalar bound"),
        pytest.param(_core.at_least, 0.5, np.array([0.2, 0.9]), [True, False], id="a number against an array bound"),
        pytest.param(
            _core.more_than,
            np.array([[0.5], [0.0]]),
            np.array([0.0, 0.5]),
            [[True, False], [False, False]],
            id="a column against a row broadcasts",
        ),
        pytest.param(
            _core.more_than,
            np.array([0.5, 0.0]),
            np.array([0.5 - 1e-10, -1e-6]),
            [False, True],
            id="the tolerance applies per bound",
        ),
    ],
)
def test_the_bound_broadcasts_against_the_values_as_numpy_does(decide, value, bound, expected):
    assert decide(value, bound).tolist() == expected
