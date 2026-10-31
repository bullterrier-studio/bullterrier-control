"""The Rust extension's boundary, `_native._core`: what the core implements, what `render` reads and what it refuses.

An architecture test: the extension is internal (the package reaches it through `Rendered`, and only this test calls it
directly), and only it can say which shapers the Rust core implements. Every refusal is a `ValueError`, a programming
error: `Rendered` hands each shaper a 1-D, C-contiguous float64 array, and a shaper's parameter model validates a
user's values before they get here. The rows run against `slew`, the first shaper the core implements.
"""

import numpy as np
import pytest

from bullterrier_control import registry
from bullterrier_control._native import _core  # noqa: PLC2701  architecture: the boundary the shapers call

SLEW = {"rise": 0.5, "fall": 0.5, "rate": 10.0}  # 0.1 s a sample, 0.2 a sample up or down
STEP = np.array([0.0, 1.0, 1.0])


def test_every_shaper_of_the_rust_core_is_a_registered_shaper():
    assert set(_core.shapers()) <= set(registry.shapers())


def test_a_shaper_renders_into_a_new_float64_array_leaving_its_input_as_it_was():
    inputs = STEP.copy()
    out = _core.render("slew", inputs, SLEW)
    assert out.dtype == np.float64
    assert out.tolist() == pytest.approx([0.0, 0.2, 0.4], abs=1e-12)
    assert inputs.tolist() == STEP.tolist()


def test_a_parameter_left_out_takes_its_default():
    assert _core.render("slew", STEP, {"rise": 0.5, "rate": 10.0}).tolist() == pytest.approx([0.0, 0.2, 0.4], abs=1e-12)


def test_no_samples_render_to_an_empty_float64_array():
    out = _core.render("slew", np.zeros(0), SLEW)
    assert out.dtype == np.float64
    assert out.shape == (0,)


@pytest.mark.parametrize(
    ("name", "inputs", "params", "named"),
    [
        pytest.param("no_such_shaper", STEP, SLEW, "no_such_shaper", id="an unknown shaper is refused by its name"),
        pytest.param("slew", [0.0, 1.0], SLEW, "numpy array", id="a list is refused: a numpy array only"),
        pytest.param("slew", STEP.astype(np.float32), SLEW, "float64", id="float32 is refused: float64 only"),
        pytest.param("slew", np.zeros((2, 3)), SLEW, "1-D", id="a 2-D array is refused: 1-D only"),
        pytest.param("slew", np.zeros(6)[::2], SLEW, "C-contiguous", id="a strided view is refused: contiguous only"),
        pytest.param("slew", STEP, {**SLEW, "depth": 1.0}, "depth", id="an extra parameter is refused by its name"),
        pytest.param("slew", STEP, {**SLEW, "rise": 4000.0}, "rise", id="a value out of range is refused by name"),
        pytest.param("slew", STEP, {**SLEW, "rate": 2**70}, "signed 64-bit", id="an integer beyond 64 bits is refused"),
        pytest.param("slew", STEP, {**SLEW, "rate": None}, "NoneType", id="None has no JSON form"),
        pytest.param("slew", STEP, {**SLEW, "rate": (10.0,)}, "tuple", id="a tuple has no JSON form"),
        pytest.param("slew", STEP, {**SLEW, "rate": True}, "rate", id="a bool is a boolean, not the number 1"),
        pytest.param("slew", STEP, {**SLEW, "rise": float("nan")}, "not finite", id="NaN has no JSON form"),
    ],
)
def test_what_the_core_cannot_read_is_a_value_error_naming_the_rule(name, inputs, params, named):
    with pytest.raises(ValueError, match=named):
        _core.render(name, inputs, params)
