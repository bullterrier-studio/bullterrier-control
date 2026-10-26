import math

import numpy as np
import pytest
from hypothesis import given
from hypothesis import strategies as st

from bullterrier_control import Event, ParameterError, lib

E = math.e

# A single event of strength s at time e, with duration g or none, read at u = t - e (`help(lib.envelope)`): 0 before
# it; s * u / attack while it rises; then from s toward s * sustain over `decay`; from g on, from the level L it had at
# g toward 0 over `release`. A moment (no duration) has g = infinity and sustain 0. Each expected value below is that,
# worked by hand.


def by_hand(t, events, *, attack=0.0, decay=0.25, sustain=0.0, release=0.0, curve="exponential", mix="max"):
    """The oracle: every event's value at time t from the formulas above, uncut, combined by `mix`."""

    def before_the_end(u, s, sustain):
        if u < attack:
            return s * u / attack
        if decay == 0:
            return s * sustain
        if curve == "exponential":
            return s * (sustain + (1 - sustain) * math.exp(-(u - attack) / decay))
        return s * (1 - (1 - sustain) * min(1.0, (u - attack) / decay))

    def one(event):
        u = t - event.t
        g = math.inf if event.duration is None else event.duration
        held = 0.0 if event.duration is None else sustain
        if u < 0:
            return 0.0
        if u < g:
            return before_the_end(u, event.strength, held)
        level = before_the_end(g, event.strength, held)
        if release == 0:
            return 0.0
        if curve == "exponential":
            return level * math.exp(-(u - g) / release)
        return level * max(0.0, 1 - (u - g) / release)

    values = [one(event) for event in events]
    return sum(values) if mix == "sum" else max(values, default=0.0)


KICKS = [Event(t=0.5, strength=1.0), Event(t=1.0, strength=0.6), Event(t=1.5, strength=1.0)]


@pytest.mark.parametrize(
    ("mix", "expected"),
    [
        pytest.param("max", [0.6, 0.6 / E], id="max: the second kick beats the first one's e^-2 and e^-3"),
        pytest.param("sum", [0.6 + E**-2, 0.6 / E + E**-3], id="sum: both kicks add"),
    ],
)
def test_overlapping_kicks_combine_by_mix(mix, expected):
    signal = lib.envelope(events=KICKS, attack=0, decay=0.25, mix=mix)
    assert signal.sample([1.0, 1.25]).tolist() == pytest.approx(expected, abs=1e-12)


# a moment of strength 0.8 at 1.0 s: it rises over 0.1 s, then falls with a time constant of 0.25 s from 1.1 s on
MOMENT = [Event(t=1.0, strength=0.8)]


@pytest.mark.parametrize(
    ("curve", "time", "expected"),
    [
        pytest.param("exponential", 0.99, 0.0, id="before the event"),
        pytest.param("exponential", 1.0, 0.0, id="at the event the rise starts from 0"),
        pytest.param("exponential", 1.05, 0.4, id="half way up: 0.8 * 0.05 / 0.1"),
        pytest.param("exponential", 1.1, 0.8, id="the top, at the end of the attack"),
        pytest.param("exponential", 1.35, 0.8 / E, id="one decay later: 0.8 e^-1"),
        pytest.param("exponential", 1.6, 0.8 / E**2, id="two decays later: 0.8 e^-2"),
        pytest.param("linear", 1.05, 0.4, id="linear: the same rise"),
        pytest.param("linear", 1.225, 0.4, id="linear: half the decay after the top, 0.8 * (1 - 0.5)"),
        pytest.param("linear", 1.35, 0.0, id="linear: down at the end of the decay"),
        pytest.param("linear", 2.0, 0.0, id="linear: and stays down"),
    ],
)
def test_a_moment_rises_over_the_attack_then_falls_over_the_decay(curve, time, expected):
    signal = lib.envelope(events=MOMENT, attack=0.1, decay=0.25, curve=curve)
    assert signal.sample(time).tolist() == pytest.approx(expected, abs=1e-12)


# a note at 2.0 s held for 1.0 s: no attack, it falls toward the sustain 0.5 with a time constant of 0.25 s; at its end
# (u = 1.0) it has reached L = 0.5 + 0.5 e^-4 and releases from there
LEVEL_AT_THE_END = 0.5 + 0.5 * E**-4
NOTE = [Event(t=2.0, strength=1.0, duration=1.0)]


@pytest.mark.parametrize(
    ("release", "time", "expected"),
    [
        pytest.param(0.5, 2.0, 1.0, id="the start, at full strength"),
        pytest.param(0.5, 2.25, 0.5 + 0.5 / E, id="one decay in: 0.5 + 0.5 e^-1"),
        pytest.param(0.5, 2.999, 0.5 + 0.5 * E**-3.996, id="just before the end it still holds"),
        pytest.param(0.5, 3.0, LEVEL_AT_THE_END, id="the end: the release starts from L"),
        pytest.param(0.5, 3.5, LEVEL_AT_THE_END / E, id="one release later: L e^-1"),
        pytest.param(0.0, 2.999, 0.5 + 0.5 * E**-3.996, id="release 0: it holds until the end"),
        pytest.param(0.0, 3.0, 0.0, id="release 0: and is 0 at once from the end"),
    ],
)
def test_an_event_with_a_duration_falls_to_its_sustain_and_releases_from_where_it_is_at_its_end(
    release, time, expected
):
    signal = lib.envelope(events=NOTE, attack=0, decay=0.25, sustain=0.5, release=release)
    assert signal.sample(time).tolist() == pytest.approx(expected, abs=1e-12)


def test_the_linear_curve_reaches_the_sustain_and_zero_exactly():
    # decay 0.4 toward sustain 0.5, held 2 s, released over 1 s: u = 0.2 is half way down (1 - 0.5 * 0.5), the
    # sustain from u = 0.4, L = 0.5 at the end, half of it half way through the release, 0 from u = 3
    signal = lib.envelope(
        events=[Event(t=0, duration=2)], attack=0, decay=0.4, sustain=0.5, release=1.0, curve="linear"
    )
    assert signal.sample([0.0, 0.2, 0.4, 1.0, 2.5, 3.0, 5.0]).tolist() == [1.0, 0.75, 0.5, 0.5, 0.25, 0.0, 0.0]


def test_a_duration_shorter_than_the_attack_stops_the_rise_and_releases_from_where_it_got():
    # the rise reaches 0.5 s / 1 s = 0.5 at the end (u = 0.5) and releases linearly over 0.5 s from there
    signal = lib.envelope(events=[Event(t=0, duration=0.5)], attack=1.0, release=0.5, curve="linear")
    assert signal.sample([0.25, 0.5, 0.75, 1.0, 1.2]).tolist() == [0.25, 0.5, 0.25, 0.0, 0.0]


@pytest.mark.parametrize(
    ("event", "attack", "times", "expected"),
    [
        pytest.param(Event(t=1, strength=0.8), 0.0, [1.0, 1.1, 3.0], [0.0, 0.0, 0.0], id="a moment without attack"),
        pytest.param(
            Event(t=1, strength=0.8),
            0.5,
            [1.25, 1.5, 1.6],
            [0.4, 0.0, 0.0],
            id="a moment with attack: up to the attack's end, then 0 from it",
        ),
        pytest.param(
            Event(t=1, strength=0.8, duration=1.0),
            0.0,
            [1.0, 1.5, 1.999, 2.0],
            [0.24, 0.24, 0.24, 0.0],
            id="a duration: straight to the sustain 0.8 * 0.3, until the end",
        ),
    ],
)
def test_decay_zero_ends_the_fall_at_once(event, attack, times, expected):
    signal = lib.envelope(events=[event], attack=attack, decay=0, sustain=0.3)
    assert signal.sample(times).tolist() == pytest.approx(expected, abs=1e-12)


# strengths 1.0 and 0.7, 0.5 s apart, decay 0.5: at 1.25 s only the first (e^-0.5); at 1.5 s the second has just come
# (e^-1 against 0.7); at 2.0 s e^-2 against 0.7 e^-1
TWO = [Event(t=1.0, strength=1.0), Event(t=1.5, strength=0.7)]
TWO_TIMES = [0.5, 1.25, 1.5, 2.0]


@pytest.mark.parametrize(
    ("mix", "expected"),
    [
        pytest.param("max", [0.0, E**-0.5, 0.7, 0.7 / E], id="max takes the larger"),
        pytest.param("sum", [0.0, E**-0.5, E**-1 + 0.7, E**-2 + 0.7 / E], id="sum adds"),
    ],
)
def test_mix_takes_the_larger_or_adds(mix, expected):
    assert lib.envelope(events=TWO, decay=0.5, mix=mix).sample(TWO_TIMES).tolist() == pytest.approx(expected, abs=1e-12)


def test_sum_is_the_sum_of_the_single_event_signals_and_max_the_larger():
    first = lib.envelope(events=TWO[:1], decay=0.5).sample(TWO_TIMES)
    second = lib.envelope(events=TWO[1:], decay=0.5).sample(TWO_TIMES)
    assert lib.envelope(events=TWO, decay=0.5, mix="sum").sample(TWO_TIMES).tolist() == (first + second).tolist()
    assert (
        lib.envelope(events=TWO, decay=0.5, mix="max").sample(TWO_TIMES).tolist() == np.maximum(first, second).tolist()
    )


def test_events_in_any_order_give_the_same_signal():
    times = np.linspace(0, 3, 61)
    in_order = lib.envelope(events=KICKS, decay=0.3).sample(times)
    assert lib.envelope(events=KICKS[::-1], decay=0.3).sample(times).tolist() == in_order.tolist()


@pytest.mark.parametrize(
    ("mix", "expected"), [pytest.param("max", 0.9, id="max"), pytest.param("sum", 1.3, id="sum: more than 1")]
)
def test_events_at_the_same_time_both_count(mix, expected):
    events = [Event(t=1.0, strength=0.4), Event(t=1.0, strength=0.9)]
    assert lib.envelope(events=events, mix=mix).sample(1.0).tolist() == pytest.approx(expected, abs=1e-12)


def test_an_event_at_zero_reads_its_strength_at_zero():
    assert lib.envelope(events=[Event(t=0, strength=0.35)]).sample(0.0).tolist() == 0.35


@pytest.mark.parametrize("mix", ["max", "sum"])
def test_an_event_of_strength_zero_adds_nothing(mix):
    events = [Event(t=1.0, strength=0.0), Event(t=1.2, strength=0.5)]
    # at 1.45 s the second is one decay old: 0.5 e^-1
    assert lib.envelope(events=events, mix=mix).sample([1.0, 1.1, 1.45]).tolist() == pytest.approx(
        [0.0, 0.0, 0.5 / E], abs=1e-12
    )


# a kick at 1 s (0.9), a snare at 2 s (0.5), an unlabelled event at 3 s (0.7); each read at its own time, by when a
# linear decay of 0.5 s has taken the one before to exactly 0
LABELLED = [Event(t=1, strength=0.9, label="kick"), Event(t=2, strength=0.5, label="snare"), Event(t=3, strength=0.7)]


@pytest.mark.parametrize(
    ("label", "expected"),
    [
        pytest.param("kick", [0.9, 0.0, 0.0], id="only the matching events"),
        pytest.param("snare", [0.0, 0.5, 0.0], id="another label, other events"),
        pytest.param("", [0.9, 0.5, 0.7], id="no label: every event, the unlabelled one too"),
        pytest.param("hat", [0.0, 0.0, 0.0], id="a label nothing has: zeros"),
    ],
)
def test_label_chooses_the_events_that_count(label, expected):
    assert (
        lib.envelope(events=LABELLED, label=label, decay=0.5, curve="linear").sample([1.0, 2.0, 3.0]).tolist()
        == expected
    )


@pytest.mark.parametrize(
    ("events", "times", "expected"),
    [
        pytest.param([], [0.0, 1.0, 1e6], [0.0, 0.0, 0.0], id="no events: zeros"),
        pytest.param(
            [Event(t=1e308, duration=1e308)],
            [0.0, 1e308, 1.7e308],
            [0.0, 1.0, 0.5],
            # its end and reach overflow to infinity; 0.7e308 s in, it is still held and long fallen to the sustain
            id="an event whose end is past the largest float: 0 before, 1 at it, then the sustain 0.5",
        ),
    ],
)
def test_building_and_reading_give_no_warning(events, times, expected):
    # every warning is an error in this suite, the building included (sample alone silences numpy's)
    envelope = lib.envelope(events=events, sustain=0.5, release=1.0)
    assert envelope.sample(times).tolist() == expected


def test_reading_before_the_first_event_gives_zero():
    envelope = lib.envelope(events=[Event(t=5, strength=0.6)], attack=2)
    assert envelope.sample([0.0, 2.5, 4.999]).tolist() == [0.0, 0.0, 0.0]


def test_a_long_decay_read_far_away_gives_zero():
    assert lib.envelope(events=[Event(t=1.0)], decay=3600).sample(1e12).tolist() == 0.0


@pytest.mark.parametrize(
    ("event", "start_of_the_fall", "time_constant"),
    [
        pytest.param(Event(t=1.0, strength=0.9), 1.0, 0.5, id="the decay of a moment"),
        pytest.param(Event(t=1.0, strength=0.9, duration=2.0), 3.0, 0.3, id="the release of an event with a duration"),
    ],
)
def test_the_exponential_tail_is_within_a_billionth_of_the_strength_of_the_uncut_formula(
    event, start_of_the_fall, time_constant
):
    # read from 15 to 30 time constants into the fall: across the point (ln 1e9 = 20.7) where a contribution is dropped
    times = start_of_the_fall + time_constant * np.linspace(15, 30, 3001)
    signal = lib.envelope(events=[event], decay=0.5, release=0.3, sustain=1.0)
    uncut = [by_hand(t, [event], decay=0.5, release=0.3, sustain=1.0) for t in times]
    assert np.abs(signal.sample(times) - uncut).max() < 1e-9 * 0.9


events_strategy = st.lists(
    st.builds(
        Event,
        t=st.floats(0, 20),
        strength=st.floats(0, 1),
        duration=st.none() | st.floats(0.01, 5),
    ),
    max_size=8,
)
shape_strategy = st.fixed_dictionaries(
    {
        "attack": st.just(0.0) | st.floats(0, 2),
        "decay": st.just(0.0) | st.floats(0, 3),
        "sustain": st.floats(0, 1),
        "release": st.just(0.0) | st.floats(0, 3),
        "curve": st.sampled_from(["exponential", "linear"]),
    }
)
times_strategy = st.lists(st.floats(0, 40), min_size=1, max_size=20)


@given(events=events_strategy, shape=shape_strategy, times=times_strategy, mix=st.sampled_from(["max", "sum"]))
def test_the_signal_is_every_events_value_combined_by_mix(events, shape, times, mix):
    # within a billionth of each strength: what the cut-off of an exponential tail may drop
    signal = lib.envelope(events=events, mix=mix, **shape)
    expected = [by_hand(t, events, mix=mix, **shape) for t in times]
    assert signal.sample(times).tolist() == pytest.approx(expected, abs=1e-9 * max(len(events), 1))


@given(events=events_strategy, shape=shape_strategy, times=times_strategy)
def test_under_max_the_signal_never_exceeds_the_largest_strength(events, shape, times):
    largest = max((event.strength for event in events), default=0.0)
    assert (lib.envelope(events=events, **shape).sample(times) <= largest).all()


def test_many_events_read_at_many_times_give_the_brute_force_values():
    # 10^5 kicks, one every 10 ms over 1000 s, read at 10^4 times: completing at all shows that memory does not grow
    # with events x times. The oracle here is the exponential decay of every kick (no attack), the largest one taken.
    rng = np.random.default_rng(11)
    starts = np.arange(100_000) * 0.01
    strengths = rng.uniform(0.1, 1.0, starts.size)
    times = rng.uniform(0, 1001, 10_000)
    events = [Event(t=t, strength=s) for t, s in zip(starts.tolist(), strengths.tolist(), strict=True)]

    values = lib.envelope(events=events, decay=0.05).sample(times)

    checked = times[:50]
    expected = []
    for t in checked:
        started = starts <= t
        expected.append((strengths[started] * np.exp(-(t - starts[started]) / 0.05)).max(initial=0.0))
    assert values[:50].tolist() == pytest.approx(expected, abs=1e-9)


@pytest.mark.parametrize(
    ("field", "value"),
    [
        pytest.param("curve", "cubic", id="curve is exponential or linear"),
        pytest.param("mix", "min", id="mix is max or sum"),
        pytest.param("label", 3, id="label is text"),
    ],
)
def test_a_choice_that_is_not_offered_is_refused_naming_the_field(field, value, reports):
    with pytest.raises(ParameterError) as refused:
        lib.envelope(events=KICKS, **{field: value})
    reports(refused.value, field)
