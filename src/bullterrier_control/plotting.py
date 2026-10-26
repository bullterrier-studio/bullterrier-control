"""Plotting signals as a picture: `bt-control plot`, and the gallery of docs/gallery.md.

    plot({"glow": signal}, start=0, stop=2, path=Path("glow.png"), title="glow.toml")

Promises:
- One PNG of a fixed size with time in seconds along the bottom, running from `start` to `stop` left to right, the value
  upward, one line per signal and a legend when there are several. The same signals, range and title write the same
  bytes (no date or version is written into the file).
- It needs matplotlib, an optional dependency (`pip install "bullterrier-control[plot]"`): without it every call is a
  `BackendUnavailableError` saying so, and nothing is written. matplotlib is imported when a picture is drawn, so
  importing this package never needs it.
"""

from collections.abc import Mapping
from pathlib import Path

import numpy as np

from .errors import BackendUnavailableError
from .signal import Signal

__all__ = ["plot"]

# presentation: 8 by 3 inches at 100 dots per inch is 800 by 300 pixels, and each signal is read once per pixel column
_WIDTH, _HEIGHT, _DPI = 8, 3, 100


def plot(curves: Mapping[str, Signal], *, start: float, stop: float, path: Path, title: str = "") -> None:
    """Draw `curves` (name to signal) from `start` to `stop` seconds into the PNG file at `path`, replacing it.

    `ValueError` if `stop` is not after `start`; `BackendUnavailableError` if matplotlib is not installed."""
    if not stop > start:
        raise ValueError(f"stop ({stop}) must be after start ({start})")
    try:
        from matplotlib.figure import Figure
    except ImportError:
        raise BackendUnavailableError('plotting needs matplotlib: pip install "bullterrier-control[plot]"') from None
    times = np.linspace(start, stop, _WIDTH * _DPI)
    figure = Figure(figsize=(_WIDTH, _HEIGHT), dpi=_DPI, layout="constrained")
    axes = figure.subplots()
    for name, signal in curves.items():
        axes.plot(times, signal.sample(times), label=name)
    axes.set_xlim(start, stop)
    axes.set_xlabel("seconds")
    axes.grid(visible=True, alpha=0.3)
    if title:
        axes.set_title(title)
    if len(curves) > 1:
        axes.legend(loc="upper right")
    figure.savefig(path, format="png", metadata={"Software": None})
