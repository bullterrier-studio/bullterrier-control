"""every: keep every Nth event: a flash on every second kick, a cut on every fourth beat."""

from pydantic import model_validator

from ..plugin import Event, InvalidError, Params, operation, param


class EveryParams(Params):
    """Which of the events in time order to keep: every `n`th, counting from `offset`."""

    n: int = param(
        2,
        doc="Keep one event in this many, 1 to 1000: 1 keeps all, 2 every second one, 3 every third.",
        unit="count",
        min=1,
        max=1000,
    )
    offset: int = param(
        0,
        doc=(
            "How many events to skip before the first one kept, 0 to 999 and below `n`: 0 starts with the first event,"
            " 1 with the second."
        ),
        unit="count",
        min=0,
        max=999,
    )

    @model_validator(mode="after")
    def _offset_is_below_n(self) -> "EveryParams":
        if self.offset >= self.n:
            raise InvalidError.at("offset", f"{self.offset} is not below n = {self.n}")
        return self


@operation(params=EveryParams)
def every(params: EveryParams, *, events: tuple[Event, ...]) -> tuple[Event, ...]:
    """The events at positions `offset`, `offset + n`, `offset + 2n`, ... of the events in time order.

    Events at the same time count one by one, in the order given. Every field of a kept event is unchanged.
    """
    return events[params.offset :: params.n]
