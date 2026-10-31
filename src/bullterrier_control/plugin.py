"""What an author of sources, shapers and operations needs, in one place: bullterrier-control's own use these names too.

    from bullterrier_control.plugin import Constant, Finite, Params, Signal, param, source

    class LevelParams(Params):
        \"\"\"A constant level.\"\"\"

        value: Finite = param(0.5, doc="The level, as a share of the range.", unit="fraction", min=0, max=1)

    @source(params=LevelParams)
    def level(params: LevelParams) -> Signal:
        \"\"\"A constant level.\"\"\"
        return Constant(params.value)

A package registers its sources, shapers and operations when its module is imported, and names that module as an entry
point of the group `bullterrier_control.sources` in its `pyproject.toml`:

    [project.entry-points."bullterrier_control.sources"]
    my_package = "my_package.sources"
"""

from .cadence import Positions, counter
from .easing import EASES, curve
from .errors import FormatError, InvalidError, ParameterError
from .formats import Event, Grid, Series, Unit
from .hashing import uniform
from .inputs import Keyframe, matching
from .params import Finite, Params, Switch, Text, Whole, param
from .registry import operation, shaper, source
from .rendered import Rendered
from .rounding import at_least, more_than
from .scaling import between
from .signal import Constant, Signal, maximum, minimum

__all__ = [
    "EASES",
    "Constant",
    "Event",
    "Finite",
    "FormatError",
    "Grid",
    "InvalidError",
    "Keyframe",
    "ParameterError",
    "Params",
    "Positions",
    "Rendered",
    "Series",
    "Signal",
    "Switch",
    "Text",
    "Unit",
    "Whole",
    "at_least",
    "between",
    "counter",
    "curve",
    "matching",
    "maximum",
    "minimum",
    "more_than",
    "operation",
    "param",
    "shaper",
    "source",
    "uniform",
]
