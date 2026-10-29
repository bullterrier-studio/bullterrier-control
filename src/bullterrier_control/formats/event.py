"""`events`: Events made from times in one call."""

from collections.abc import Sequence

import numpy as np
from numpy.typing import NDArray

from .._native._core import Event
from ..errors import FormatError

__all__ = ["events"]


def events(
    times: Sequence[float] | NDArray[np.float64],
    *,
    strength: float | Sequence[float] | NDArray[np.float64] = 1.0,
    label: str | None = None,
    duration: float | None = None,
) -> tuple[Event, ...]:
    """One `Event` per time, in the order given. `strength` is one number for all or one per time; `label` and
    `duration` apply to all. A `FormatError` names the field that breaks an `Event` rule and the event's place in
    the list (`events[1].t`), or `strength` when its count differs from the times'."""
    moments = list(times)
    strengths = list(strength) if isinstance(strength, Sequence | np.ndarray) else [strength] * len(moments)
    if len(strengths) != len(moments):
        raise FormatError.at(
            "strength", f"needs one value per time or a single number (got {len(strengths)} for {len(moments)} times)"
        )
    made = []
    for index, (t, each) in enumerate(zip(moments, strengths, strict=True)):
        try:
            made.append(Event(t, strength=each, label=label, duration=duration))
        except FormatError as error:  # the same problems, under the event's place in the list
            problems = tuple((f"events[{index}].{path}", rule) for path, rule in error.problems)
            raise FormatError("; ".join(f"{path}: {rule}" for path, rule in problems), problems) from None
    return tuple(made)
