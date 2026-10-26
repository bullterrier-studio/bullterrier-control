import numpy as np
import pytest

from bullterrier_control import ParameterError, lib

RAMP = lib.keyframes(points=[{"t": 0, "value": 0}, {"t": 1, "value": 1}])  # 0 at 0 s, 1 at 1 s, straight between


@pytest.mark.parametrize(
    ("time", "expected"),
    [
        pytest.param(0.0, 0.0, id="at 0: the input's first value"),
        pytest.param(0.25, 0.0, id="at the lag: the input at 0"),
        pytest.param(0.75, 0.5, id="a lag earlier: 0.75 - 0.25 = 0.5, halfway up"),
        pytest.param(1.25, 1.0, id="the input at 1.0"),
        pytest.param(2.0, 1.0, id="the input holds its last value"),
    ],
)
def test_the_output_is_the_input_a_lag_earlier(time, expected):
    assert lib.delay(RAMP, lag=0.25).sample([time]).tolist() == [expected]


def test_a_lag_of_zero_passes_the_input_through_exactly():
    times = np.linspace(0, 2, 101)
    assert lib.delay(RAMP, lag=0).sample(times).tolist() == RAMP.sample(times).tolist()


@pytest.mark.parametrize("lag", [0.05, 1.3, 40.0])
def test_it_reads_the_input_at_the_time_minus_the_lag_and_at_time_0_before_the_lag_has_passed(lag):
    wave = lib.lfo(shape="sine", rate=0.7, phase=0.3)  # not zero at 0, so a read before the lag is not 0 by accident
    times = np.concatenate([np.linspace(0, 2 * lag, 41), [0.0, lag, lag / 2, 100.0]])
    delayed = lib.delay(wave, lag=lag).sample(times)
    assert delayed.tolist() == wave.sample(np.maximum(times - lag, 0.0)).tolist()
    assert delayed[0] == wave.sample(0.0)  # the reads before the lag all show the input at 0 ...
    assert wave.sample(0.0) != 0.0  # ... which is not 0 for this input


def test_a_negative_lag_is_refused_because_a_signal_can_only_lag(reports):
    with pytest.raises(ParameterError, match="lag") as error:
        lib.delay(RAMP, lag=-0.1)
    reports(error.value, "lag")


def test_by_default_it_lags_by_a_tenth_of_a_second():
    # the ramp is 0.5 at 0.5 s, and 0.6 - 0.1 = 0.5
    assert lib.delay(RAMP).sample([0.05, 0.1, 0.6, 1.1]).tolist() == pytest.approx([0.0, 0.0, 0.5, 1.0], abs=1e-12)
