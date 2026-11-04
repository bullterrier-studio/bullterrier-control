import numpy as np
import pytest

from bullterrier_control import ParameterError, from_values, lib

RATE = 100  # a sample every 0.01 s
# 0 for the first sample, then 1: a step of the input at the second sample
STEP = from_values([0.0] + [1.0] * 399, rate=RATE, interpolation="hold")
TIMES = np.arange(0, 400) / RATE


def step_response(frequency, damping, time):
    """The textbook response of a spring at rest to a unit step (not the library's stepping)."""
    omega = 2 * np.pi * frequency
    sigma = damping * omega
    if damping < 1:
        swing = omega * np.sqrt(1 - damping**2)
        return 1 - np.exp(-sigma * time) * (np.cos(swing * time) + sigma / swing * np.sin(swing * time))
    if damping > 1:
        glide = omega * np.sqrt(damping**2 - 1)
        return 1 - np.exp(-sigma * time) * (np.cosh(glide * time) + sigma / glide * np.sinh(glide * time))
    return 1 - np.exp(-omega * time) * (1 + omega * time)


@pytest.mark.parametrize("damping", [0.0, 0.25, 0.7, 1.0, 2.0, 8.0])
def test_a_step_gets_the_textbook_response_of_a_spring(damping):
    got = lib.spring(STEP, frequency=1.5, damping=damping, rate=RATE).sample(TIMES)
    # sample n has had n steps of 0.01 s since the step at sample 1
    expected = np.concatenate([[0.0], step_response(1.5, damping, TIMES[1:])])
    assert got == pytest.approx(expected, abs=1e-9)


def test_below_damping_1_it_overshoots_and_from_1_up_it_does_not():
    rings = lib.spring(STEP, frequency=2, damping=0.25, rate=RATE).sample(TIMES)
    arrives = lib.spring(STEP, frequency=2, damping=1.0, rate=RATE).sample(TIMES)
    glides = lib.spring(STEP, frequency=2, damping=3.0, rate=RATE).sample(TIMES)
    assert rings.max() > 1.4  # 1 + e^(-pi d / sqrt(1 - d^2)) = 1.44: the classic overshoot
    assert arrives.max() <= 1.0 + 1e-12
    assert glides.max() <= 1.0 + 1e-12
    assert arrives[-1] > glides[300]  # the critical one is there first, the glide still on its way


def test_a_constant_input_leaves_the_spring_at_rest():
    got = lib.spring(lib.constant(value=0.4), frequency=7, damping=0.1).sample([0.0, 0.3, 5.0])
    assert got.tolist() == [0.4, 0.4, 0.4]


@pytest.mark.parametrize(
    ("field", "value"),
    [("frequency", 0.0), ("frequency", 101.0), ("damping", -0.1), ("damping", 10.5), ("rate", 9.0)],
)
def test_a_number_outside_its_range_is_refused_naming_the_field(field, value, reports):
    with pytest.raises(ParameterError) as refused:
        lib.spring(STEP, **{field: value})
    reports(refused.value, field)
