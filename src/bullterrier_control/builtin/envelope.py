"""envelope: on each event, rise, fall toward a sustain level and release: a flare on a kick, a swell on a note."""

import math
from collections.abc import Callable

import numpy as np
from numpy.typing import NDArray

from ..plugin import Event, Params, Signal, matching, source

# The share of a fall still left a given number of seconds into it.
_Left = Callable[[NDArray[np.float64]], NDArray[np.float64]]

# An exponential fall never quite ends: a contribution is dropped once it is below a billionth of its strength, which
# is ln(1e9) ~ 20.723 time constants into the fall (e^-x < 1e-9 for x > ln(1e9)).
_TIME_CONSTANTS_TO_A_BILLIONTH = math.log(1e9)


class EnvelopeParams(Params, builtin="envelope"):
    """Which events count, and the shape each one gives the value: attack, decay, sustain, release."""


def _fade(curve: str, span: float) -> tuple[_Left, float]:
    """The share left of a fall over `span` seconds (1 at its start, toward 0), and how many seconds into the fall it
    can still be seen. A span of 0 is decided here, once, so nothing divides by it."""
    if span == 0:
        return np.zeros_like, 0.0
    if curve == "linear":
        # (span - elapsed) / span, not 1 - elapsed / span: it cannot overflow however short the span
        return (lambda elapsed: np.maximum(span - elapsed, 0.0) / span), span
    return (lambda elapsed: np.exp(-elapsed / span)), span * _TIME_CONSTANTS_TO_A_BILLIONTH


class _Envelope(Signal):
    """The events' envelopes, each read only over the times it can reach and folded into the output by `mix`."""

    def __init__(self, events: tuple[Event, ...], params: EnvelopeParams) -> None:
        self._attack = params.attack
        # the rise is a linear fall over the attack run backwards: risen = 1 - left
        self._rise_left, _ = _fade("linear", params.attack)
        self._decay_left, decay_reach = _fade(params.curve, params.decay)
        self._release_left, release_reach = _fade(params.curve, params.release)
        self._fold = np.maximum if params.mix == "max" else np.add
        # an event of strength 0 adds nothing under either mix
        counted = [event for event in matching(events, params.label) if event.strength > 0]
        # a moment is an event that never ends and has no sustain: the same formula serves both
        self._starts = np.array([event.t for event in counted])
        self._strengths = np.array([event.strength for event in counted])
        self._ends = np.array([math.inf if event.duration is None else event.duration for event in counted])
        self._sustains = np.array([0.0 if event.duration is None else params.sustain for event in counted])
        # an overflow here is right: a span far shorter than an end gives a fall long over, an end near the largest
        # float a reach that never ends
        with np.errstate(over="ignore"):
            # a duration is seen until its release is over; a moment until its decay is
            self._reach_ends = self._starts + np.where(
                np.isinf(self._ends), params.attack + decay_reach, self._ends + release_reach
            )
            self._levels_at_end = self._held(self._ends, self._strengths, self._sustains)

    def _held(
        self, since: NDArray[np.float64], strength: float | NDArray[np.float64], sustain: float | NDArray[np.float64]
    ) -> NDArray[np.float64]:
        """The value `since` seconds after an event starts, as long as it has not ended (one event, or each of many)."""
        # np.where evaluates both branches: without the clamp, the unused one computes exp(+large), giving inf
        # or 0 * inf = NaN
        return np.where(
            since < self._attack,
            strength * (1.0 - self._rise_left(since)),
            strength * (sustain + (1.0 - sustain) * self._decay_left(np.maximum(since - self._attack, 0.0))),
        )

    def compute(self, times: NDArray[np.float64]) -> NDArray[np.float64]:
        flat = times.ravel()
        order = np.argsort(flat, kind="stable")
        ordered = flat[order]
        values = np.zeros_like(ordered)
        # each event touches only the reads from its start to its reach end: memory is O(times + events)
        firsts = np.searchsorted(ordered, self._starts, side="left")
        lasts = np.searchsorted(ordered, self._reach_ends, side="right")
        # only events with a read in their window: a single read does not pay for the events nowhere near it
        seen = firsts < lasts
        for first, last, start, strength, end, sustain, level in zip(
            firsts[seen].tolist(),
            lasts[seen].tolist(),
            self._starts[seen].tolist(),
            self._strengths[seen].tolist(),
            self._ends[seen].tolist(),
            self._sustains[seen].tolist(),
            self._levels_at_end[seen].tolist(),
            strict=True,
        ):
            since = ordered[first:last] - start
            released = level * self._release_left(np.maximum(since - end, 0.0))
            window = values[first:last]
            self._fold(window, np.where(since < end, self._held(since, strength, sustain), released), out=window)
        result = np.empty_like(values)
        result[order] = values
        return result.reshape(times.shape)


@source(params=EnvelopeParams)
def envelope(params: EnvelopeParams, *, events: tuple[Event, ...]) -> Signal:
    """A signal that, on each event, rises to the event's strength over `attack`, falls toward `sustain` over `decay`
    and, once an event with a duration ends, falls to 0 over `release`.

    Read u seconds after an event of strength s: 0 before it; s * u / attack while it rises; then from s toward
    s * sustain, exponentially (s * (sustain + (1 - sustain) * e^(-(u - attack) / decay))) or linearly (reaching it
    `decay` seconds after the attack). An event without a duration never ends and has sustain 0. From the end of an
    event with a duration, the level L it had there falls to 0: L * e^(-(u - duration) / release), or linearly over
    `release`. If the duration is shorter than the attack, the rise stops there. Only events with `label` count
    (all if it is empty). Overlapping events combine by `mix`.

    The value is never below 0; under `max` never above the largest strength, under `sum` it can exceed 1 (clip it).
    An exponential tail is dropped once it is below a billionth of its event's strength, so each event's contribution
    is within 1e-9 times its strength of the uncut formula. Memory grows with the times read plus the events, never
    with their product.
    """
    return _Envelope(events, params)
