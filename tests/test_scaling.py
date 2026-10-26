import numpy as np
import pytest

from bullterrier_control import plugin


@pytest.mark.parametrize(
    ("low", "high", "fraction", "expected"),
    [
        pytest.param(-1.0, 3.0, [0.0, 0.25, 0.5, 1.0], [-1.0, 0.0, 1.0, 3.0], id="from -1 to 3: low + 4 * fraction"),
        pytest.param(3.0, -1.0, [0.0, 0.25, 0.5, 1.0], [3.0, 2.0, 1.0, -1.0], id="low above high runs downward"),
        pytest.param(2.0, 2.0, [0.0, 0.3, 1.0], [2.0, 2.0, 2.0], id="low equal to high is constant"),
        pytest.param(0.3, 0.9, [1.0], [0.9], id="the top is exactly high where 0.3 + 0.6 * 1 is 0.9000000000000001"),
        pytest.param(0.9, 0.3, [1.0, 0.0], [0.3, 0.9], id="inverted, the ends are exactly high and low"),
    ],
)
def test_a_fraction_of_zero_to_one_becomes_a_value_between_low_and_high(low, high, fraction, expected):
    values = plugin.between(low, high, np.array(fraction))
    assert values.tolist() == pytest.approx(expected, abs=1e-12)
    # exactly within the range: not an ulp beyond either end
    assert min(low, high) <= values.min()
    assert values.max() <= max(low, high)
