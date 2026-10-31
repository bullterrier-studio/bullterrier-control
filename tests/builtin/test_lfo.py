import numpy as np
import pytest
from hypothesis import given
from hypothesis import strategies as st

from bullterrier_control import Grid, ParameterError, SignalError, lib, registry

# a steady 120 BPM: a beat every half second
STEADY = Grid(beats=tuple(index * 0.5 for index in range(40)), meter=4, first_downbeat=0)
# the grid of example 4: it slows down, with beats of 0.5, 0.6 and 0.7 s
SLOWING = Grid(beats=(0.0, 0.5, 1.1, 1.8), meter=4, first_downbeat=0)

# low -1 and high 3 (a range of 4, not 1, and not starting at 0): a swapped or ignored bound gives other numbers.
# At rate 1 per second the time is the position in the cycle: x = 0, 0.25, 0.5, 0.75.
QUARTERS = [0.0, 0.25, 0.5, 0.75]


@pytest.mark.parametrize(
    ("shape", "expected"),
    [
        # 0.5 + 0.5 sin(2 pi x) = 0.5, 1, 0.5, 0 -> -1 + 4 s
        pytest.param("sine", [1.0, 3.0, 1.0, -1.0], id="sine"),
        # 1 - |2x - 1| = 0, 0.5, 1, 0.5
        pytest.param("triangle", [-1.0, 1.0, 3.0, 1.0], id="triangle"),
        # 1 while x < 0.5: the halfway point itself is already low
        pytest.param("square", [3.0, 3.0, -1.0, -1.0], id="square"),
        # x
        pytest.param("saw", [-1.0, 0.0, 1.0, 2.0], id="saw"),
        # 1 - x
        pytest.param("saw-down", [3.0, 2.0, 1.0, 0.0], id="saw-down"),
    ],
)
def test_each_shape_reads_its_hand_computed_values_a_quarter_cycle_apart(shape, expected):
    signal = lib.lfo(shape=shape, rate=1, low=-1, high=3)
    assert signal.sample(QUARTERS).tolist() == pytest.approx(expected, abs=1e-9)


def test_the_defaults_are_a_sine_once_a_second_between_0_and_1():
    assert lib.lfo().sample(QUARTERS).tolist() == pytest.approx([0.5, 1.0, 0.5, 0.0], abs=1e-9)


def test_it_repeats_every_cycle():
    signal = lib.lfo(shape="saw", rate=1, low=-1, high=3)
    assert signal.sample([2.25, 7.5]).tolist() == pytest.approx([0.0, 1.0], abs=1e-9)


@pytest.mark.parametrize(
    ("rate", "times", "expected"),
    [
        pytest.param(2, [0.125, 0.375], [3.0, -1.0], id="twice a second: a quarter cycle in 0.125 s"),
        pytest.param(0.5, [0.5, 1.5], [3.0, -1.0], id="every other second: a quarter cycle in 0.5 s"),
        pytest.param(0, [0.0, 3.3, 1e6], [3.0, 3.0, 3.0], id="rate 0 stays where the phase put it (a quarter cycle)"),
    ],
)
def test_rate_is_cycles_per_second(rate, times, expected):
    # a quarter cycle in: the high of a sine (rate 0: held at phase 0.25)
    phase = 0.25 if rate == 0 else 0.0
    signal = lib.lfo(shape="sine", rate=rate, phase=phase, low=-1, high=3)
    assert signal.sample(times).tolist() == pytest.approx(expected, abs=1e-9)


def test_phase_starts_the_cycle_that_far_in():
    signal = lib.lfo(shape="saw", phase=0.75, low=-1, high=3)
    assert signal.sample([0.0, 0.25, 0.5]).tolist() == pytest.approx([2.0, -1.0, 0.0], abs=1e-9)


def test_low_above_high_inverts_the_signal():
    signal = lib.lfo(shape="saw", rate=1, low=3, high=-1)
    assert signal.sample(QUARTERS).tolist() == pytest.approx([3.0, 2.0, 1.0, 0.0], abs=1e-9)


@pytest.mark.parametrize(
    ("duty", "expected"),
    [
        pytest.param(0.25, [3.0, -1.0, -1.0, -1.0], id="a quarter: high only up to 0.25, not at it"),
        pytest.param(0.75, [3.0, 3.0, 3.0, -1.0], id="three quarters"),
        pytest.param(0.0, [-1.0, -1.0, -1.0, -1.0], id="0: never high"),
        pytest.param(1.0, [3.0, 3.0, 3.0, 3.0], id="1: always high"),
    ],
)
def test_duty_is_the_share_of_the_cycle_a_square_is_high(duty, expected):
    signal = lib.lfo(shape="square", rate=1, duty=duty, low=-1, high=3)
    assert signal.sample(QUARTERS).tolist() == expected


def test_duty_does_nothing_to_the_other_shapes():
    assert (
        lib.lfo(shape="saw", duty=0.1).sample(QUARTERS).tolist()
        == lib.lfo(shape="saw", duty=0.9).sample(QUARTERS).tolist()
    )


# --- following the beat ---


def test_per_beat_on_a_steady_grid_is_the_same_as_twice_the_rate_per_second():
    times = np.linspace(0.0, 19.0, 191)
    per_beat = lib.lfo(shape="triangle", rate=1, per="beat", phase=0.125, grid=STEADY, low=-1, high=3)
    per_second = lib.lfo(shape="triangle", rate=2, phase=0.125, low=-1, high=3)
    assert per_beat.sample(times).tolist() == pytest.approx(per_second.sample(times).tolist(), abs=1e-9)


def test_per_beat_rate_counts_cycles_per_beat():
    # rate 2 per beat on 0.5 s beats: 4 cycles a second, so 0.0625 s is a quarter cycle
    signal = lib.lfo(shape="sine", rate=2, per="beat", grid=STEADY, low=-1, high=3)
    assert signal.sample([0.0625, 0.1875]).tolist() == pytest.approx([3.0, -1.0], abs=1e-9)


def test_per_beat_follows_a_tempo_change_one_cycle_per_beat_whatever_its_length():
    signal = lib.lfo(shape="saw", rate=1, per="beat", grid=SLOWING)
    beats = np.asarray(SLOWING.beats)
    middles = (beats[:-1] + beats[1:]) / 2  # 0.25, 0.8, 1.45
    assert signal.sample(beats).tolist() == pytest.approx([0.0] * 4, abs=1e-9)
    assert signal.sample(middles).tolist() == pytest.approx([0.5] * 3, abs=1e-9)


def test_per_beat_continues_the_last_beat_length_far_past_the_last_beat():
    # 1000 more beats of 0.7 s (the last one), then a quarter of one: beat 3 + 1000 + 0.25
    signal = lib.lfo(shape="sine", per="beat", grid=SLOWING)
    assert signal.sample([1.8 + 0.7 * 1000 + 0.175]).tolist() == pytest.approx([1.0], abs=1e-9)


def test_a_grid_of_two_beats_is_enough_and_goes_on_for_ever():
    grid = Grid(beats=(0.0, 0.5), meter=1, first_downbeat=0)
    signal = lib.lfo(shape="sine", per="beat", grid=grid)
    assert signal.sample([0.125, 1000.125]).tolist() == pytest.approx([1.0, 1.0], abs=1e-9)


def test_before_a_first_beat_that_is_not_at_zero_the_cycle_runs_backwards_from_it():
    # beats of 0.5 s from 1.0: time 0 is beat -2, 0.75 s is beat -0.5, which is half a cycle in
    grid = Grid(beats=(1.0, 1.5, 2.0), meter=1, first_downbeat=0)
    signal = lib.lfo(shape="saw", per="beat", grid=grid)
    assert signal.sample([0.0, 0.75, 1.0]).tolist() == pytest.approx([0.0, 0.5, 0.0], abs=1e-9)


def test_a_square_with_duty_1_is_high_just_before_the_first_beat_too():
    # beat_at(0) is -2e-20 here: a float modulo of that gives exactly 1.0, the end of the cycle, which is not high
    grid = Grid(beats=(1e-20, 0.5), meter=1, first_downbeat=0)
    signal = lib.lfo(shape="square", per="beat", duty=1.0, grid=grid)
    assert signal.sample([0.0]).tolist() == [1.0]


@pytest.mark.parametrize(
    ("shape", "expected"),
    [
        pytest.param("saw", 1.0, id="a saw is at the top of its ramp"),
        pytest.param("saw-down", 0.0, id="a saw-down is at the bottom of its ramp"),
        pytest.param("triangle", 0.0, id="a triangle is at the bottom"),
    ],
)
def test_just_before_a_first_beat_near_zero_the_wave_is_at_the_end_of_its_cycle_not_the_start(shape, expected):
    # beat_at(0) is -2e-17, a hair before the cycle starts again: the end of the previous cycle
    grid = Grid(beats=(1e-17, 0.5), meter=1, first_downbeat=0)
    signal = lib.lfo(shape=shape, per="beat", grid=grid)
    assert signal.sample([0.0]).tolist() == pytest.approx([expected], abs=1e-9)


def test_per_beat_needs_a_grid(reports):
    with pytest.raises(ParameterError) as refused:
        lib.lfo(per="beat")
    reports(refused.value, "per")


def test_a_grid_is_not_needed_and_ignored_per_second():
    with_grid = lib.lfo(rate=1.5, grid=SLOWING).sample([0.1, 0.9, 4.2])
    assert with_grid.tolist() == lib.lfo(rate=1.5).sample([0.1, 0.9, 4.2]).tolist()


# --- far times and the bounds ---


def test_a_billion_seconds_in_keeps_the_phase_and_stays_in_range():
    # 1000 cycles a second for 1e9 s is exactly 1e12 cycles; a phase of 0.25 is left over (exact in a float there)
    signal = lib.lfo(shape="saw", rate=1000, phase=0.25, low=-1, high=3)
    assert signal.sample([1e9]).tolist() == pytest.approx([0.0], abs=1e-9)


@pytest.mark.parametrize("per", ["second", "beat"])
def test_a_time_too_far_for_the_position_to_be_a_number_is_a_signal_error(per):
    signal = lib.lfo(rate=1000, per=per, grid=STEADY)
    with pytest.raises(SignalError):
        signal.sample([1.797e308])


def test_the_top_of_the_wave_is_exactly_high_even_where_float_rounding_would_overshoot_it():
    # 0.3 + (0.9 - 0.3) * 1 is 0.9000000000000001 in floats: a square with duty 1 sits at the top all the time
    signal = lib.lfo(shape="square", duty=1.0, low=0.3, high=0.9)
    assert signal.sample([0.0, 0.4, 0.99]).tolist() == [0.9, 0.9, 0.9]


@given(
    shape=st.sampled_from(["sine", "triangle", "square", "saw", "saw-down"]),
    rate=st.floats(0, 1000),
    phase=st.floats(0, 1),
    duty=st.floats(0, 1),
    low=st.floats(-1e6, 1e6),
    high=st.floats(-1e6, 1e6),
    time=st.floats(0, 1e300),
)
def test_the_output_never_leaves_low_and_high_whichever_way_round(shape, rate, phase, duty, low, high, time):
    signal = lib.lfo(shape=shape, rate=rate, phase=phase, duty=duty, low=low, high=high)
    value = signal.sample(time).item()
    assert min(low, high) <= value <= max(low, high)


def test_the_phase_wraps_so_a_rotating_phase_needs_none_of_its_own():
    wrapped = lib.lfo(shape="saw", phase=2.75, low=-1, high=3)
    inside = lib.lfo(shape="saw", phase=0.75, low=-1, high=3)
    assert wrapped.sample(QUARTERS).tolist() == pytest.approx(inside.sample(QUARTERS).tolist(), abs=1e-9)


def test_the_phase_is_described_as_cyclic_for_a_consumer_mapping_onto_it():
    phase = registry.get("lfo").description()["parameters"]["properties"]["phase"]
    assert (phase["unit"], phase["minimum"], phase["maximum"], phase["cyclic"]) == ("cycles", 0, 1, True)


@pytest.mark.parametrize("shape", ["sine", "triangle", "square", "saw", "saw-down"])
def test_a_phase_at_the_top_of_its_range_is_the_phase_at_the_bottom_exactly(shape):
    # a decimal time such as 3.1 sits a hair either side of a cycle edge, and adding a whole cycle first would move it
    times = np.linspace(0, 10, 1001)
    bottom = lib.lfo(shape=shape, rate=1, duty=0.1, phase=0.0)
    top = lib.lfo(shape=shape, rate=1, duty=0.1, phase=1.0)
    assert top.sample(times).tolist() == bottom.sample(times).tolist()
