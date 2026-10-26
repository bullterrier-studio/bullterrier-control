import numpy as np
import pytest
from hypothesis import given
from hypothesis import strategies as st

from bullterrier_control import Grid, ParameterError, SignalError, lib

# a steady 120 BPM: a beat every half second
STEADY = Grid(beats=tuple(index * 0.5 for index in range(40)), meter=4, first_downbeat=0)
# beats of 0.5, 0.6 and 0.7 s
SLOWING = Grid(beats=(0.0, 0.5, 1.1, 1.8), meter=4, first_downbeat=0)
# beats of 0.5 s from 1.0 s on: time 0 is beat -2
LATE = Grid(beats=(1.0, 1.5, 2.0), meter=1, first_downbeat=0)


def test_each_step_holds_its_value_for_one_step_and_the_list_repeats():
    signal = lib.steps(values=[1, 2, 3], rate=1)
    assert signal.sample([0.5, 1.5, 2.5, 3.5, 4.5]).tolist() == [1.0, 2.0, 3.0, 1.0, 2.0]


def test_a_step_starts_exactly_at_its_time_and_the_instant_before_still_reads_the_last():
    signal = lib.steps(values=[1, 2, 3], rate=1)
    assert signal.sample([0.0, 1.0, 3.0, np.nextafter(1.0, 0)]).tolist() == [1.0, 2.0, 1.0, 1.0]


def test_rate_is_steps_per_second():
    # 4 steps a second: a step is 0.25 s; at 0.1 s (first), 0.3 s (second), 0.6 s (third), 0.8 s (fourth)
    signal = lib.steps(values=[0.5, -1.0, 2.0, 7.0], rate=4, per="second")
    assert signal.sample([0.1, 0.3, 0.6, 0.8, 1.1]).tolist() == [0.5, -1.0, 2.0, 7.0, 0.5]


def test_a_slow_rate_holds_a_step_for_a_long_time():
    signal = lib.steps(values=[1, 2, 3], rate=0.001, per="second")
    assert signal.sample([999.0, 1500.0, 2500.0]).tolist() == [1.0, 2.0, 3.0]


def test_the_defaults_are_four_steps_a_second():
    # a step is 0.25 s: at 0.1 s the first, at 0.3 s the second, at 0.6 s the first again
    assert lib.steps(values=[1, 2]).sample([0.1, 0.3, 0.6]).tolist() == [1.0, 2.0, 1.0]


def test_one_value_is_constant():
    signal = lib.steps(values=[0.75], rate=3, per="second")
    assert signal.sample([0.0, 0.2, 1.7, 1e6]).tolist() == [0.75] * 4


def test_values_keep_the_order_they_are_given_in():
    forward = lib.steps(values=[1, 2, 3], rate=1).sample([0.5, 1.5, 2.5]).tolist()
    backward = lib.steps(values=[3, 2, 1], rate=1, per="second").sample([0.5, 1.5, 2.5]).tolist()
    assert (forward, backward) == ([1.0, 2.0, 3.0], [3.0, 2.0, 1.0])


# --- following the beat ---


def test_per_beat_on_a_steady_grid_is_the_same_as_twice_the_rate_per_second():
    times = np.arange(0.0, 10.0, 0.125)
    per_beat = lib.steps(values=[1, 2, 3], rate=2, per="beat", grid=STEADY)
    per_second = lib.steps(values=[1, 2, 3], rate=4, per="second")
    assert per_beat.sample(times).tolist() == per_second.sample(times).tolist()


def test_per_beat_follows_a_tempo_change_a_step_per_beat_whatever_its_length():
    # a step a beat: beat 1 starts at 0.5 s, beat 2 at 1.1 s, beat 3 at 1.8 s
    signal = lib.steps(values=[1, 2, 3], rate=1, per="beat", grid=SLOWING)
    assert signal.sample([0.25, 0.5, 0.8, 1.1, 1.45, 1.79, 1.8]).tolist() == [1.0, 2.0, 2.0, 3.0, 3.0, 3.0, 1.0]


def test_per_beat_goes_on_past_the_last_beat_with_the_last_beat_length():
    # beat 3 is at 1.8 s and beats last 0.7 s from there: beat 1003.25 is at 1.8 + 0.7 * 1000.25
    signal = lib.steps(values=[1, 2, 3], rate=1, per="beat", grid=SLOWING)
    assert signal.sample([1.8 + 0.7 * 1000 + 0.175]).tolist() == [2.0]  # 1003 mod 3 = 1


def test_before_a_first_beat_that_is_not_at_zero_the_list_runs_backwards_from_it():
    # time 0 is beat -2, 0.75 s is beat -0.5, 1.0 s is beat 0: steps -2 and -1 are the second and third value
    signal = lib.steps(values=[10, 20, 30], rate=1, per="beat", grid=LATE)
    assert signal.sample([0.0, 0.75, 1.0, 1.5]).tolist() == [20.0, 30.0, 10.0, 20.0]


def test_per_beat_needs_a_grid(reports):
    with pytest.raises(ParameterError) as refused:
        lib.steps(values=[1, 2], per="beat")
    reports(refused.value, "per")


def test_a_grid_is_not_needed_and_ignored_per_second():
    with_grid = lib.steps(values=[1, 2, 3], rate=1.5, per="second", grid=SLOWING).sample([0.1, 0.9, 4.2])
    assert with_grid.tolist() == lib.steps(values=[1, 2, 3], rate=1.5, per="second").sample([0.1, 0.9, 4.2]).tolist()


# --- far times ---


def test_a_trillion_seconds_in_still_picks_the_right_step():
    # position 1e12 exactly; 10^12 leaves 1 when divided by 3 (its digits add up to 1)
    signal = lib.steps(values=[1, 2, 3], rate=1)
    assert signal.sample([1e12, 1e12 + 0.5]).tolist() == [2.0, 2.0]


def test_a_time_too_far_for_the_position_to_be_a_number_is_a_signal_error():
    with pytest.raises(SignalError):
        lib.steps(values=[1, 2, 3], rate=1000, per="second").sample([1.797e308])


@given(
    values=st.lists(st.floats(-1e6, 1e6), min_size=1, max_size=8),
    rate=st.floats(0.001, 1000),
    time=st.floats(0, 1e15),
)
def test_every_read_is_one_of_the_values(values, rate, time):
    read = lib.steps(values=values, rate=rate, per="second").sample(time).item()
    assert read in values
