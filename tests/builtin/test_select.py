import pytest

from bullterrier_control import Event, ParameterError, lib

# (time, strength, label): 0.5 is exactly the threshold used below, so a `>` where `>=` belongs shows
EVENTS = [
    Event(t=0.5, strength=0.9, label="kick"),
    Event(t=0.7, strength=0.45, label="kick"),
    Event(t=1.0, strength=0.8),
    Event(t=1.2, strength=0.5, label="snare"),
]


def times(events):
    return [event.t for event in events]


@pytest.mark.parametrize(
    ("values", "expected"),
    [
        pytest.param({}, [0.5, 0.7, 1.0, 1.2], id="the defaults keep every event, the unlabelled one too"),
        pytest.param({"threshold": 0.5}, [0.5, 1.0, 1.2], id="a strength equal to the threshold is kept; 0.45 is not"),
        pytest.param({"threshold": 0.9}, [0.5], id="a strength equal to a high threshold is kept"),
        pytest.param({"threshold": 0.95}, [], id="above every strength: none"),
        pytest.param({"label": "kick"}, [0.5, 0.7], id="a label keeps the events carrying it"),
        pytest.param({"label": "snare"}, [1.2], id="another label"),
        pytest.param({"label": "kick", "threshold": 0.5}, [0.5], id="both: a kick of at least 0.5"),
        pytest.param({"label": "hat"}, [], id="a label nothing has: none"),
        pytest.param({"label": "", "threshold": 0.8}, [0.5, 1.0], id="an empty label keeps the unlabelled one"),
    ],
)
def test_select_keeps_the_events_that_match_the_label_and_reach_the_threshold(values, expected):
    assert times(lib.select(events=EVENTS, **values)) == expected


def test_select_keeps_every_field_of_the_events_it_keeps():
    note = Event(t=0.5, strength=0.7, duration=0.25, label="lead", pitch=64.5)
    assert lib.select(events=[note, Event(t=0.6, strength=0.1)], threshold=0.5) == (note,)


def test_select_gives_events_in_time_order_whatever_the_order_given():
    assert times(lib.select(events=EVENTS[::-1], threshold=0.5)) == [0.5, 1.0, 1.2]


def test_select_keeps_events_at_the_same_time_in_the_order_given():
    first, second = Event(t=1.0, strength=0.6, label="a"), Event(t=1.0, strength=0.9, label="b")
    assert lib.select(events=[second, first]) == (second, first)
    assert lib.select(events=[first, second]) == (first, second)


def test_select_of_no_events_is_no_events():
    assert lib.select(events=[], threshold=0.5) == ()


@pytest.mark.parametrize(
    ("values", "named"),
    [
        pytest.param({"threshold": 1.5}, "threshold", id="a threshold above 1"),
        pytest.param({"threshold": -0.1}, "threshold", id="a threshold below 0"),
        pytest.param({"label": 3}, "label", id="a label that is not text"),
    ],
)
def test_select_refuses_a_bad_parameter_naming_it(values, named, reports):
    with pytest.raises(ParameterError) as refused:
        lib.select(events=EVENTS, **values)
    reports(refused.value, named)
