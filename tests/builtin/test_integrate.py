import pytest

from bullterrier_control import ParameterError, lib

# 0 at 0 s rising straight to 1 at 4 s, then holding 1: the speed 0.25 a second at 1 s
SPEED = lib.keyframes(points=[{"t": 0, "value": 0}, {"t": 4, "value": 1}])


@pytest.mark.parametrize(
    ("time", "expected"),
    [
        pytest.param(0.0, 0.0, id="at 0: nothing added up yet"),
        pytest.param(0.5, 0.03125, id="under a line t / 4 up to 0.5 s: t^2 / 8"),
        pytest.param(2.0, 0.5, id="2 s: 4 / 8"),
        pytest.param(4.0, 2.0, id="4 s: 16 / 8, the area of a triangle 4 wide and 1 high"),
        pytest.param(6.0, 4.0, id="then the speed is 1: 2 more a 2 s"),
    ],
)
def test_the_output_is_the_area_under_the_input_so_far(time, expected):
    # rate 10: every time is a sample time, and the trapezoid rule is exact on straight lines
    got = lib.integrate(SPEED, rate=10).sample([time]).tolist()
    assert got == pytest.approx([expected], abs=1e-9)


def test_a_constant_starts_at_0_and_grows_by_its_value_a_second():
    # 3 a second: nothing at 0 (the first sample adds nothing), then 3 for each second
    got = lib.integrate(lib.constant(value=3.0), rate=10).sample([0.0, 0.1, 1.0, 2.5]).tolist()
    assert got == pytest.approx([0.0, 0.3, 3.0, 7.5], abs=1e-9)


def test_a_falling_input_takes_the_output_back_down():
    # +1 for 1 s, then -1 for 1 s
    wave = lib.keyframes(points=[{"t": 0, "value": 1, "ease": "hold"}, {"t": 1, "value": -1}])
    got = lib.integrate(wave, rate=100).sample([0.5, 1.0, 2.0]).tolist()
    assert got == pytest.approx([0.5, 1.0, -0.0], abs=0.02)  # a step edge falls between two samples


@pytest.mark.parametrize("rate", [9.0, 48001.0])
def test_a_rate_outside_its_range_is_refused_naming_it(rate, reports):
    with pytest.raises(ParameterError) as refused:
        lib.integrate(SPEED, rate=rate)
    reports(refused.value, "rate")
