import numpy as np
import pytest

from bullterrier_control import Event, ParameterError, lib

# before 0.2 and after 0.7: distinct, so a swapped pair or a wrong level fails
DROP = [Event(t=12.0)]


@pytest.mark.parametrize(
    ("times", "expected"),
    [
        pytest.param([0.0, 5.0, 11.0], [0.2, 0.2, 0.2], id="before the event: before"),
        pytest.param([np.nextafter(12.0, 0)], [0.2], id="the instant before the event still reads before"),
        pytest.param([12.0], [0.7], id="at the event: after"),
        pytest.param([np.nextafter(12.0, 13)], [0.7], id="the instant after the event reads after"),
        pytest.param([13.0, 30.0, 1e6], [0.7, 0.7, 0.7], id="and it stays after"),
    ],
)
def test_the_value_is_before_until_the_event_and_after_from_it_on(times, expected):
    assert lib.latch(events=DROP, before=0.2, after=0.7).sample(times).tolist() == expected


def test_the_defaults_switch_from_0_to_1():
    assert lib.latch(events=DROP).sample([0.0, 11.999, 12.0, 30.0]).tolist() == [0.0, 0.0, 1.0, 1.0]


# unlabelled at 3 s, "kick" at 5 s, "drop" at 12 s
LABELLED = [Event(t=12.0, label="drop"), Event(t=3.0), Event(t=5.0, label="kick")]


@pytest.mark.parametrize(
    ("label", "expected"),
    [
        pytest.param("drop", [0.2, 0.2, 0.2, 0.7], id="only the matching events: the drop at 12"),
        pytest.param("kick", [0.2, 0.2, 0.7, 0.7], id="another label, another moment: 5"),
        pytest.param("", [0.2, 0.7, 0.7, 0.7], id="no label: every event, the unlabelled one too: 3"),
        pytest.param("hat", [0.2, 0.2, 0.2, 0.2], id="a label nothing has: never switches"),
    ],
)
def test_label_chooses_the_events_that_count(label, expected):
    signal = lib.latch(events=LABELLED, label=label, before=0.2, after=0.7)
    assert signal.sample([2.0, 4.0, 8.0, 15.0]).tolist() == expected


def test_it_switches_at_the_earliest_event_whatever_their_order():
    events = [Event(t=20.0), Event(t=5.0), Event(t=12.0)]
    assert lib.latch(events=events, before=0.2, after=0.7).sample([4.0, 5.0, 6.0]).tolist() == [0.2, 0.7, 0.7]


def test_no_events_give_before_always():
    assert lib.latch(events=[], before=0.2, after=0.7).sample([0.0, 12.0, 1e6]).tolist() == [0.2, 0.2, 0.2]


def test_before_above_after_is_an_inverted_latch():
    assert lib.latch(events=DROP, before=1.0, after=0.0).sample([11.0, 12.0, 30.0]).tolist() == [1.0, 0.0, 0.0]


def test_an_event_at_zero_reads_after_from_the_start():
    assert lib.latch(events=[Event(t=0.0)], before=0.2, after=0.7).sample([0.0, 1.0]).tolist() == [0.7, 0.7]


def test_the_strength_and_duration_of_an_event_do_not_matter():
    events = [Event(t=12.0, strength=0.0, duration=3.0)]
    assert lib.latch(events=events, before=0.2, after=0.7).sample([11.0, 12.0, 20.0]).tolist() == [0.2, 0.7, 0.7]


def test_equal_before_and_after_read_that_value_everywhere():
    assert lib.latch(events=DROP, before=0.4, after=0.4).sample([0.0, 12.0, 30.0]).tolist() == [0.4, 0.4, 0.4]


def test_the_shape_of_the_input_is_kept():
    times = np.array([[1.0, 12.0, 13.0], [30.0, 0.0, 11.0]])
    values = lib.latch(events=DROP, before=0.2, after=0.7).sample(times)
    assert values.tolist() == [[0.2, 0.7, 0.7], [0.7, 0.2, 0.2]]


def test_a_label_that_is_not_text_is_refused_naming_the_field(reports):
    with pytest.raises(ParameterError) as refused:
        lib.latch(events=DROP, label=3)
    reports(refused.value, "label")
