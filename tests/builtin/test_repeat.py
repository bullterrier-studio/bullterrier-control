import numpy as np
import pytest
from hypothesis import assume, given
from hypothesis import strategies as st

from bullterrier_control import Grid, ParameterError, lib

# the input reads as its time up to 10 s: what comes out is the time it was read at
CLOCK = lib.keyframes(points=[{"t": 0, "value": 0}, {"t": 10, "value": 10}])
# beats of 0.5, 0.6 and 0.7 s; the last beat length goes on after the last beat
SLOWING = Grid(beats=(0.0, 0.5, 1.1, 1.8), meter=4, first_downbeat=0)
# the first beat is at 1 s: time 0 is two beats before it
LATE = Grid(beats=(1.0, 1.5, 2.0), meter=1, first_downbeat=0)


@pytest.mark.parametrize(
    ("mode", "times", "expected"),
    [
        pytest.param("wrap", [0.0, 1.0, 1.5, 1.99], [0.0, 1.0, 1.5, 1.99], id="wrap: the first stretch is the input"),
        pytest.param("wrap", [2.0, 3.0, 3.5, 5.0], [0.0, 1.0, 1.5, 1.0], id="wrap: from 2 s the input again from 0"),
        pytest.param("ping-pong", [0.5, 2.0], [0.5, 2.0], id="ping-pong: forward to the end of the stretch"),
        pytest.param("ping-pong", [3.0, 3.5, 4.0], [1.0, 0.5, 0.0], id="ping-pong: then backward to 0"),
        pytest.param("ping-pong", [5.0, 6.0], [1.0, 2.0], id="ping-pong: then forward again"),
    ],
)
def test_the_input_is_read_at_each_time_folded_into_the_first_two_seconds(mode, times, expected):
    # the input's own interpolation rounds in the last digit (1.99 reads 1.9900000000000002)
    assert lib.repeat(CLOCK, length=2, mode=mode).sample(times).tolist() == pytest.approx(expected, abs=1e-12)


@pytest.mark.parametrize(
    ("grid", "length", "mode", "times", "expected"),
    [
        # beats at 0, 0.5, 1.1, 1.8 s; 1.45 s is 2.5 beats in, 2.15 s is 3.5, 2.5 s is 4 (the last beat length goes on)
        pytest.param(
            SLOWING, 2, "wrap", [0.25, 1.1, 1.45, 1.8], [0.25, 0.0, 0.25, 0.5], id="two beats, a tempo change"
        ),
        pytest.param(SLOWING, 2, "wrap", [2.15, 2.5], [0.8, 0.0], id="past the last beat"),
        pytest.param(SLOWING, 2, "ping-pong", [1.45, 1.8, 2.15], [0.8, 0.5, 0.25], id="ping-pong in beats"),
        # time 0 is beat -2: the stretch starts at time 0, not at the first beat (1 s), and so does the repeat
        pytest.param(LATE, 1.5, "wrap", [0.5, 0.75, 1.0], [0.5, 0.0, 0.25], id="counting starts where time 0 falls"),
    ],
)
def test_beats_of_the_grid_set_the_length(grid, length, mode, times, expected):
    got = lib.repeat(CLOCK, length=length, mode=mode, per="beat", grid=grid).sample(times).tolist()
    assert got == pytest.approx(expected, abs=1e-12)


@given(
    length=st.floats(min_value=0.5, max_value=20.0),
    time=st.floats(min_value=0.0, max_value=1000.0),
)
def test_a_wrap_repeats_every_length_and_a_ping_pong_every_two(length, time):
    # a wrap jumps back to 0 at each multiple of its length: keep away from the jump, where a rounding of
    # time + length across it is a different value, not a defect
    assume(1e-6 < time % length < length - 1e-6)
    wrap = lib.repeat(CLOCK, length=length)
    swing = lib.repeat(CLOCK, length=length, mode="ping-pong")
    assert wrap.sample([time + length]).tolist() == pytest.approx(wrap.sample([time]).tolist(), abs=1e-9 * (1 + time))
    assert swing.sample([time + 2 * length]).tolist() == pytest.approx(
        swing.sample([time]).tolist(), abs=1e-9 * (1 + time)
    )


@given(length=st.floats(min_value=0.5, max_value=20.0), offset=st.floats(min_value=0.0, max_value=19.0))
def test_a_ping_pong_runs_backward_as_it_ran_forward(length, offset):
    swing = lib.repeat(CLOCK, length=length, mode="ping-pong")
    offset = offset % length
    assert swing.sample([length + offset]).tolist() == pytest.approx(
        swing.sample([length - offset]).tolist(), abs=1e-9 * (1 + length)
    )


def test_the_repeat_never_reads_later_than_the_time_it_is_read_at():
    times = np.linspace(0, 40, 801)
    for mode in ("wrap", "ping-pong"):
        read = lib.repeat(CLOCK, length=3, mode=mode).sample(times)
        assert (read <= times + 1e-12).all()


def test_per_beat_needs_a_grid(reports):
    with pytest.raises(ParameterError) as refused:
        lib.repeat(CLOCK, per="beat")
    reports(refused.value, "per")


@pytest.mark.parametrize("length", [0.0, 0.0001, 3601.0])
def test_a_length_outside_its_range_is_refused_naming_it(length, reports):
    with pytest.raises(ParameterError) as refused:
        lib.repeat(CLOCK, length=length)
    reports(refused.value, "length")
