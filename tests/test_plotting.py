"""Plotting a signal over time: the picture shows the signal's shape, left to right, and is the same every time.

The picture is read back as pixels. matplotlib's first line colour (#1f77b4) is the oracle for where a curve was drawn.
"""

import sys

import matplotlib.image
import numpy as np
import pytest

from bullterrier_control import Series, Unit, from_series, lib
from bullterrier_control.errors import BackendUnavailableError
from bullterrier_control.plotting import plot

LINE = np.array([0x1F, 0x77, 0xB4]) / 255  # the first colour of matplotlib's default cycle


def curve_pixels(path):
    """(rows, columns) of the pixels of the first line colour in the picture at `path`, and the picture's width."""
    picture = matplotlib.image.imread(path)
    hit = np.abs(picture[..., :3] - LINE).max(axis=2) < 0.1
    rows, columns = np.nonzero(hit)
    return rows, columns, picture.shape[1]


def ramp(rising=True):
    values = (0.0, 1.0) if rising else (1.0, 0.0)
    return from_series(Series(rate=1, values=values, unit=Unit.RATIO))


def test_the_picture_is_a_png_of_a_fixed_size(tmp_path):
    path = tmp_path / "ramp.png"
    plot({"ramp": ramp()}, start=0, stop=1, path=path)
    data = path.read_bytes()
    assert data[:8] == b"\x89PNG\r\n\x1a\n"
    assert (int.from_bytes(data[16:20], "big"), int.from_bytes(data[20:24], "big")) == (800, 300)


@pytest.mark.parametrize("rising", [True, False], ids=["rising", "falling"])
def test_a_curve_is_drawn_with_time_running_left_to_right_and_the_value_upward(rising, tmp_path):
    path = tmp_path / "ramp.png"
    plot({"ramp": ramp(rising)}, start=0, stop=1, path=path)
    rows, columns, width = curve_pixels(path)
    assert len(rows) > 100  # the line is there at all
    left, right = rows[columns < width / 3].mean(), rows[columns > 2 * width / 3].mean()
    # row numbers grow downward: a rising curve is lower (a larger row) on the left than on the right
    assert (left - right > 50) if rising else (right - left > 50)


def test_the_axes_show_exactly_the_range_asked_for_with_no_margin_of_empty_time(tmp_path):
    path = tmp_path / "ramp.png"
    plot({"ramp": ramp()}, start=0, stop=1, path=path)
    _, columns, width = curve_pixels(path)
    # the axes run from about 35 px (after the labels of the value axis) to the right edge; a margin of a twentieth on
    # each side would start the curve past 70 px and end it before 750
    assert columns.min() < 55
    assert columns.max() > width - 35


def test_the_range_drawn_is_the_range_asked_for(tmp_path):
    # a ramp that reaches 1 at 1 s, drawn from 1 s to 2 s, is a flat line (it holds its last value)
    path = tmp_path / "held.png"
    plot({"ramp": ramp()}, start=1, stop=2, path=path)
    rows, columns, width = curve_pixels(path)
    left, right = rows[columns < width / 3].mean(), rows[columns > 2 * width / 3].mean()
    assert abs(left - right) < 5


def test_two_curves_are_both_drawn_and_named_in_a_legend(tmp_path):
    one, both = tmp_path / "one.png", tmp_path / "both.png"
    plot({"ramp": ramp()}, start=0, stop=1, path=one)
    plot({"ramp": ramp(), "level": lib.constant(value=0.5)}, start=0, stop=1, path=both)
    assert one.read_bytes() != both.read_bytes()
    picture = matplotlib.image.imread(both)
    orange = np.abs(picture[..., :3] - np.array([0xFF, 0x7F, 0x0E]) / 255).max(axis=2) < 0.1
    assert orange.sum() > 100  # the second curve has the second colour of the cycle


def test_several_curves_are_named_in_a_legend_and_a_single_one_is_not(tmp_path):
    def picture(names):
        path = tmp_path / f"{'-'.join(names)}.png"
        plot({name: ramp() for name in names}, start=0, stop=1, path=path)
        return path.read_bytes()

    # the same curves under other names differ only in the legend's text
    assert picture(["a", "b"]) != picture(["c", "d"])
    # a lone curve has no legend, so its name is written nowhere
    assert picture(["a"]) == picture(["c"])


def test_nothing_about_the_machine_or_the_version_is_written_into_the_file(tmp_path):
    path = tmp_path / "ramp.png"
    plot({"ramp": ramp()}, start=0, stop=1, path=path)
    assert b"matplotlib" not in path.read_bytes().lower()  # its default "Software" text names the version


def test_drawing_the_same_thing_twice_writes_the_same_bytes(tmp_path):
    first, second = tmp_path / "a.png", tmp_path / "b.png"
    for path in (first, second):
        plot({"ramp": ramp()}, start=0, stop=1, path=path, title="a ramp")
    assert first.read_bytes() == second.read_bytes()


def test_a_different_signal_gives_a_different_picture(tmp_path):
    rising, falling = tmp_path / "rising.png", tmp_path / "falling.png"
    plot({"ramp": ramp()}, start=0, stop=1, path=rising)
    plot({"ramp": ramp(False)}, start=0, stop=1, path=falling)
    assert rising.read_bytes() != falling.read_bytes()


def test_without_matplotlib_it_says_what_to_install(tmp_path, monkeypatch):
    for name in [name for name in sys.modules if name == "matplotlib" or name.startswith("matplotlib.")]:
        monkeypatch.setitem(sys.modules, name, None)  # an import of it fails, as if it were not installed
    with pytest.raises(BackendUnavailableError, match=r"bullterrier-control\[plot\]"):
        plot({"ramp": ramp()}, start=0, stop=1, path=tmp_path / "ramp.png")
    assert not (tmp_path / "ramp.png").exists()


def test_a_range_with_no_width_is_refused(tmp_path):
    with pytest.raises(ValueError, match="stop"):
        plot({"ramp": ramp()}, start=1, stop=1, path=tmp_path / "ramp.png")
