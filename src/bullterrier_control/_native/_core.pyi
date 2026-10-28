# The stub of the Rust extension `bullterrier_control._native._core` (crates/python). The functions' documentation is
# the extension's own (`help(_core.render)`).

from collections.abc import Mapping

import numpy as np
from numpy.typing import NDArray

__all__ = ["render", "shapers"]

def render(name: str, inputs: NDArray[np.float64], params: Mapping[str, object]) -> NDArray[np.float64]: ...
def shapers() -> list[str]: ...
