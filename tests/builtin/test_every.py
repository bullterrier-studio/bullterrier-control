import pytest

from bullterrier_control import Event, ParameterError, lib

# seven events a quarter of a second apart, each telling its own index by its strength (index / 10)
SEVEN = [Event(t=index * 0.25, strength=index / 10) for index in range(7)]


def indices(events):
    return [round(event.strength * 10) for event in events]


@pytest.mark.parametrize(
    ("values", "expected"),
    [
        pytest.param({}, [0, 2, 4, 6], id="the defaults keep every second event, from the first"),
        pytest.param({"n": 3, "offset": 1}, [1, 4], id="every third from the second: 1, 4 (7 is past the end)"),
        pytest.param({"n": 3}, [0, 3, 6], id="every third from the first"),
        pytest.param({"n": 1}, [0, 1, 2, 3, 4, 5, 6], id="n = 1 keeps every event"),
        pytest.param({"n": 2, "offset": 1}, [1, 3, 5], id="an offset starts at that event"),
        pytest.param({"n": 4, "offset": 3}, [3], id="the last offset below n keeps one"),
        pytest.param({"n": 10}, [0], id="n above the count keeps only the first"),
        pytest.param({"n": 10, "offset": 9}, [], id="an offset past the last event keeps none"),
    ],
)
def test_every_keeps_the_events_at_offset_plus_whole_multiples_of_n(values, expected):
    assert indices(lib.every(events=SEVEN, **values)) == expected


def test_every_counts_in_time_order_whatever_the_order_given():
    assert indices(lib.every(events=SEVEN[::-1], n=3, offset=1)) == [1, 4]


def test_every_counts_events_at_the_same_time_one_by_one_in_the_order_given():
    first, second, third, fourth = (Event(t=1.0, label=name) for name in "abcd")
    assert lib.every(events=[first, second, third, fourth]) == (first, third)
    assert lib.every(events=[fourth, third, second, first]) == (fourth, second)


def test_every_keeps_every_field_of_the_events_it_keeps():
    note = Event(t=0.5, strength=0.7, duration=0.25, label="lead", pitch=64.5)
    assert lib.every(events=[note, Event(t=0.75)], n=2) == (note,)


def test_every_of_no_events_is_no_events():
    assert lib.every(events=[]) == ()


def test_an_offset_that_is_not_below_n_is_refused_at_offset(reports):
    with pytest.raises(ParameterError) as refused:
        lib.every(events=SEVEN, n=3, offset=3)
    reports(refused.value, "offset")
    assert [path for path, _ in refused.value.problems] == ["offset"]


def test_an_offset_just_below_n_is_accepted():
    assert indices(lib.every(events=SEVEN, n=3, offset=2)) == [2, 5]
