"""What an author of sources and shapers needs, in one place: bullterrier-control's own use these names too.

    from bullterrier_control.plugin import Constant, Finite, Params, Signal, param, source

    class LevelParams(Params):
        \"\"\"A constant level.\"\"\"

        value: Finite = param(0.5, doc="The level, as a share of the range.", unit="fraction", min=0, max=1)

    @source(params=LevelParams)
    def level(params: LevelParams) -> Signal:
        \"\"\"A constant level.\"\"\"
        return Constant(params.value)

A package registers its sources and shapers when its module is imported, and names that module as an entry point of
the group `bullterrier_control.sources` in its `pyproject.toml`:

    [project.entry-points."bullterrier_control.sources"]
    my_package = "my_package.sources"
"""

from .easing import EASES, curve
from .errors import InvalidError, ParameterError
from .inputs import Keyframe
from .params import Params, param
from .registry import shaper, source
from .rendered import Rendered
from .signal import Constant, Signal, maximum, minimum
from .validated import Finite, Switch, Text, Whole

__all__ = [
    "EASES",
    "Constant",
    "Finite",
    "InvalidError",
    "Keyframe",
    "ParameterError",
    "Params",
    "Rendered",
    "Signal",
    "Switch",
    "Text",
    "Whole",
    "curve",
    "maximum",
    "minimum",
    "param",
    "shaper",
    "source",
]
