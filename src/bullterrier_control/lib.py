"""Every registered source, shaper and operation, by name: the way to use them.

    from bullterrier_control import lib

    glow = lib.slew(energy, rise=0.02, fall=0.4)       # a shaper: a signal, then parameter values
    glow = lib.clip(glow, low=0.15, high=0.9)
    kicks = lib.select(events=bundle.events["kick"], threshold=0.5)    # an operation: data in, data out
    dir(lib)                                            # what is installed
    help(lib.slew)                                      # what it does, and every parameter with its unit and range

Built-in and installed packages' sources, shapers and operations alike (`bullterrier_control.registry` loads them on
first use).
"""

from .errors import UnknownNameError
from .registry import Registered, all_registered, get


def __getattr__(name: str) -> Registered:
    """The source, shaper or operation called `name`; `AttributeError` listing the available ones if there is none."""
    if name.startswith("_"):
        raise AttributeError(name)
    try:
        return get(name)
    except UnknownNameError as error:
        raise AttributeError(str(error)) from None


def __dir__() -> list[str]:
    """The names of every registered source, shaper and operation."""
    return sorted(all_registered())
