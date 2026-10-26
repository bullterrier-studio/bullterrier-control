import numpy as np
import pytest

from bullterrier_control.rounding import at_least, more_than


@pytest.mark.parametrize(
    ("value", "bound", "expected"),
    [
        pytest.param(0.3 - 0.1, 0.2, True, id="0.3 - 0.1 is 0.19999999999999998: at least 0.2"),
        pytest.param(0.2, 0.2, True, id="equal"),
        pytest.param(0.25, 0.2, True, id="above"),
        pytest.param(0.199999, 0.2, False, id="a millionth under"),
        pytest.param(-2.2e-16, 0.0, True, id="a rounding error under zero is at least zero"),
        pytest.param(-1e-6, 0.0, False, id="a millionth under zero is not"),
    ],
)
def test_at_least_holds_a_value_that_is_the_bound_to_within_float_rounding(value, bound, expected):
    assert bool(at_least(value, bound)) is expected


@pytest.mark.parametrize(
    ("value", "bound", "expected"),
    [
        pytest.param(0.1 + 0.2 - 0.3, 0.0, False, id="5.5e-17 is not more than zero"),
        pytest.param(0.0, 0.0, False, id="equal"),
        pytest.param(1e-6, 0.0, True, id="a millionth over zero is"),
        pytest.param(-1e-6, 0.0, False, id="below"),
        pytest.param(0.5, 0.2, True, id="well over"),
    ],
)
def test_more_than_needs_a_value_beyond_the_bound_by_more_than_float_rounding(value, bound, expected):
    assert bool(more_than(value, bound)) is expected


def test_both_work_elementwise_on_arrays():
    values = np.array([-1e-6, -2.2e-16, 0.0, 1e-6])
    assert np.asarray(at_least(values, 0.0)).tolist() == [False, True, True, True]
    assert np.asarray(more_than(values, 0.0)).tolist() == [False, False, False, True]
