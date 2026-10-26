"""quantize: snap events to the beat grid: a flash that lands on the beat instead of just beside it."""

import numpy as np

from ..plugin import Event, FormatError, Grid, ParameterError, Params, at_least, operation, param


class QuantizeParams(Params):
    """How fine the positions are that events snap to."""

    division: int = param(
        1,
        doc=(
            "Snap positions per beat, 1 to 64: 1 snaps to the beats, 2 to the beats and the halves between them, 4 to"
            " quarters. Raising it moves events less."
        ),
        unit="count",
        min=1,
        max=64,
    )


@operation(params=QuantizeParams)
def quantize(params: QuantizeParams, *, events: tuple[Event, ...], grid: Grid) -> tuple[Event, ...]:
    """The events, each moved to the nearest beat position that is a multiple of 1/`division` of a beat.

    Nearest is measured in beats, so it follows tempo changes; exactly halfway goes to the later position, and an event
    whose nearest position would be before time 0 goes to the next position up (a position at 0 to within float
    rounding, a nanosecond, is 0). Before the first beat and after the last the grid's first or last beat length goes
    on. The result is in time order, and events that snap to one time keep the order they had; every other field is
    unchanged (a duration does not move its end). A time too far from the beats to snap is a `ParameterError` naming
    `events`.
    """
    division = params.division
    try:
        # an overflow is refused by the grid just below, not warned about
        with np.errstate(over="ignore"):
            steps = np.floor(grid.beat_at([event.t for event in events]) * division + 0.5)
            snapped = grid.seconds_at(steps / division)
            before_zero = ~at_least(snapped, 0.0)
            snapped[before_zero] = grid.seconds_at((steps[before_zero] + 1) / division)
            snapped = np.maximum(snapped, 0.0)  # a position at 0 to within rounding is 0
    except FormatError as error:
        raise ParameterError.at("events", f"a time is too far from the grid's beats to snap ({error})") from None
    # the snap never decreases, so events sorted by time stay sorted
    return tuple(event.model_copy(update={"t": float(time)}) for event, time in zip(events, snapped, strict=True))
