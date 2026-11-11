import pytest
from hypothesis import given, settings
from hypothesis import strategies as st

from bullterrier_control import BullterrierControlError, Event, lib, load_patch

# Times are multiples of 1/64, which floats hold exactly: the range is decided exactly.


def times(events):
    return [event.t for event in events]


@pytest.mark.parametrize(
    ("values", "given", "expected"),
    [
        pytest.param({}, [0.0, 1.0, 2.0], [0.0, 1.0, 2.0], id="the defaults keep all"),
        pytest.param({"start": 2.0}, [1.0, 2.0, 3.0], [2.0, 3.0], id="an event exactly at start is kept"),
        pytest.param({"start": 2.0}, [1.9999, 2.0], [2.0], id="an event a hair before start is not"),
        pytest.param({"end": 2.0}, [1.0, 2.0, 3.0], [1.0], id="an event exactly at end is dropped"),
        pytest.param({"end": 2.0}, [1.9999, 2.0], [1.9999], id="an event a hair before end is kept"),
        pytest.param(
            {"start": 1.0, "end": 3.0}, [0.5, 1.0, 2.0, 3.0, 4.0], [1.0, 2.0], id="start and end make a range: [1, 3)"
        ),
        pytest.param({"start": 2.0, "end": 2.0}, [1.0, 2.0, 3.0], [], id="start equal to end is empty"),
        pytest.param(
            {"start": 2.0, "first": 2}, [1.0, 2.0, 3.0, 4.0], [2.0, 3.0], id="first counts in the range, not the input"
        ),
        pytest.param({"start": 2.0, "first": 10}, [1.0, 2.0, 3.0], [2.0, 3.0], id="first beyond what remains"),
        pytest.param({"first": 2}, [1.0, 1.0, 1.0], [1.0, 1.0], id="equal times count one by one"),
        pytest.param({"start": 0.1 + 0.2}, [0.3], [0.3], id="decimals: 0.1 + 0.2 is at 0.3 within rounding"),
        pytest.param({"start": 1.0, "end": 2.0, "first": 3}, [], [], id="no events keep none"),
        pytest.param({"start": 3.0}, [4.0, 1.0, 3.0, 2.0], [3.0, 4.0], id="time order whatever the order given"),
    ],
)
def test_window_keeps_the_events_in_the_range_then_the_first_n(values, given, expected):
    assert times(lib.window(events=[Event(t=t) for t in given], **values)) == expected


@pytest.mark.parametrize(
    ("values", "field"),
    [
        pytest.param({"start": 5.0, "end": 2.0}, "end", id="end before start"),
        pytest.param({"first": -1}, "first", id="negative first"),
        pytest.param({"start": -0.5}, "start", id="negative start"),
        pytest.param({"end": -0.5}, "end", id="negative end"),
        pytest.param({"first": 10001}, "first", id="first above its range"),
    ],
)
def test_window_refuses_a_bad_value_naming_the_field(values, field):
    with pytest.raises(BullterrierControlError, match=field):
        lib.window(events=[Event(t=1.0)], **values)


def test_window_keeps_every_field_of_what_it_keeps():
    loud = Event(t=3.0, strength=0.9, label="a", pitch=40)
    assert lib.window(events=[Event(t=1.0), loud], start=2.0) == (loud,)


def test_the_first_four_kicks_after_the_reveal_thinned_are_the_times_worked_by_hand():
    kicks = [Event(t=t) for t in (0.5, 1.0, 8.0, 8.1, 8.5, 8.6, 9.2, 10.0)]
    assert times(lib.thin(events=lib.window(events=kicks, start=8.0, first=4), gap=0.3)) == [8.0, 8.5]


@settings(max_examples=200)
@given(
    steps=st.lists(st.integers(min_value=0, max_value=256), max_size=30),
    start_steps=st.integers(min_value=0, max_value=300),
    end_steps=st.integers(min_value=0, max_value=300),
    first=st.integers(min_value=0, max_value=40),
)
def test_window_agrees_with_a_brute_force_filter(steps, start_steps, end_steps, first):
    end_steps = 0 if end_steps < start_steps else end_steps  # an end before start is an error, tested apart
    start, end = start_steps / 64, end_steps / 64
    events = [Event(t=step / 64, label=str(index)) for index, step in enumerate(steps)]
    ordered = sorted(events, key=lambda event: event.t)
    in_range = [e for e in ordered if e.t >= start and (end == 0 or e.t < end)]
    expected = tuple(in_range[:first] if first else in_range)
    assert lib.window(events=events, start=start, end=end, first=first) == expected


def test_a_toml_patch_using_window_gives_the_events_the_python_call_gives(tmp_path):
    path = tmp_path / "words.toml"
    path.write_text(
        'format = "bullterrier-control/patch"\nversion = 1\n\n'
        '[words]\nuse = "window"\nstart = 8.0\nfirst = 2\n'
        "events = [{ t = 0.5 }, { t = 8.0 }, { t = 8.5 }, { t = 9.0 }]\n",
        encoding="utf-8",
    )
    from_patch = load_patch(path).build()["words"]
    assert times(from_patch) == [8.0, 8.5]
    assert from_patch == lib.window(events=[Event(t=t) for t in (0.5, 8.0, 8.5, 9.0)], start=8.0, first=2)
