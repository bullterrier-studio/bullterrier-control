"""beats: events from a beat grid: every beat, the downbeats marked, and optionally subdivisions between them."""

import numpy as np

from ..plugin import Event, Grid, Params, operation, param


class BeatsParams(Params):
    """How finely to divide each beat."""

    division: int = param(
        1,
        doc=(
            "Events per beat, 1 to 64: 1 gives the beats alone, 2 adds one `sub` event halfway between neighbouring"
            " beats, 4 adds three, and so on. Raising it gives a finer pulse."
        ),
        unit="count",
        min=1,
        max=64,
    )


@operation(params=BeatsParams)
def beats(params: BeatsParams, *, grid: Grid) -> tuple[Event, ...]:
    """Events at the grid's beats and, with a `division`, at equal steps of beats between them, each of strength 1.

    The label is `downbeat` on the first beat of each bar (every `meter`th beat from `first_downbeat`), `beat` on
    any other beat, and `sub` between beats. There are events from the first stored beat to the last, none beyond,
    and a step is an equal share of a beat, so it follows tempo changes. Select some with `select`: the downbeats
    are `select(events=beats(grid=g), label="downbeat")`.
    """
    division = params.division
    steps = np.arange((len(grid.beats) - 1) * division + 1)
    times = grid.seconds_at(steps / division)
    return tuple(
        Event(t=float(time), strength=1.0, label=_label(int(step), division, grid))
        for step, time in zip(steps, times, strict=True)
    )


def _label(step: int, division: int, grid: Grid) -> str:
    """The label of the event at `step` divisions from the first beat."""
    if step % division:
        return "sub"
    # a bar starts every `meter` beats from `first_downbeat`, and `first_downbeat` is below `meter`, so no beat before
    # it can start one
    return "downbeat" if (step // division - grid.first_downbeat) % grid.meter == 0 else "beat"
