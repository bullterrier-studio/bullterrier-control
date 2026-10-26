import numpy as np
import pytest

from bullterrier_control import Grid, ParameterError, SignalError
from bullterrier_control.plugin import counter

# beats of 0.5, 0.6 and 0.7 s
SLOWING = Grid(beats=(0.0, 0.5, 1.1, 1.8), meter=4, first_downbeat=0)
# beats of 0.5 s from 1.0 s on
LATE = Grid(beats=(1.0, 1.5, 2.0), meter=1, first_downbeat=0)


@pytest.mark.parametrize(
    ("rate", "per", "grid", "times", "expected"),
    [
        pytest.param(3.0, "second", None, [0.0, 0.5, 2.0], [0.0, 1.5, 6.0], id="per second: rate times t"),
        pytest.param(0.0, "second", None, [0.0, 9.0], [0.0, 0.0], id="rate 0 never moves"),
        pytest.param(2.0, "second", SLOWING, [1.0], [2.0], id="a grid is ignored per second"),
        pytest.param(1.0, "beat", SLOWING, [0.0, 0.5, 1.1, 1.8], [0.0, 1.0, 2.0, 3.0], id="a beat is one count"),
        pytest.param(2.0, "beat", SLOWING, [0.25, 0.8, 1.45], [1.0, 3.0, 5.0], id="halfway through each beat"),
        pytest.param(1.0, "beat", SLOWING, [2.5], [4.0], id="past the last beat: its length (0.7 s) goes on"),
        pytest.param(1.0, "beat", LATE, [0.0, 0.75], [-2.0, -0.5], id="before the first beat: negative"),
    ],
)
def test_the_position_counts_per_second_or_per_beat_of_the_grid(rate, per, grid, times, expected):
    assert counter(rate, per, grid)(np.array(times)).tolist() == pytest.approx(expected, abs=1e-9)


def test_per_beat_without_a_grid_is_refused_naming_per(reports):
    with pytest.raises(ParameterError) as refused:
        counter(1.0, "beat", None)
    reports(refused.value, "per")


@pytest.mark.parametrize(
    ("per", "rate"),
    [
        pytest.param("second", 1000.0, id="rate times the time overflows"),
        pytest.param("beat", 1.0, id="the grid cannot reach the time"),
    ],
)
def test_a_time_too_far_for_a_position_is_a_signal_error(per, rate):
    with pytest.raises(SignalError):
        counter(rate, per, SLOWING)(np.array([1.797e308]))


def test_a_far_time_the_position_can_hold_is_still_read():
    assert counter(1000.0, "second", None)(np.array([1e300])).tolist() == [1e303]
