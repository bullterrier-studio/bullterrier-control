"""A plugin written as `docs/api.md` (section 9) and the `poe new-source` templates write one, through the author API
alone: it registers, validates its values, documents itself in `help()` and is listed by `bt-control list --json`."""

import json

import numpy as np
import pytest

from bullterrier_control import ParameterError, lib
from bullterrier_control.cli import main
from bullterrier_control.plugin import Finite, Params, Signal, param, shaper


class FoldParams(Params):
    """Where the signal folds back."""

    depth: Finite = param(0.5, doc="How far down from 1 the fold is.", unit="fraction", min=0, max=1)


class Folded(Signal):
    """A signal with what lies above the fold mirrored below it."""

    def __init__(self, signal: Signal, depth: float) -> None:
        self._signal, self._line = signal, 1.0 - depth

    def compute(self, times):
        values = self._signal.compute(times)
        return np.where(values > self._line, 2.0 * self._line - values, values)


@pytest.fixture
def fold(throwaway_registry):
    @shaper(params=FoldParams)
    def fold(signal: Signal, params: FoldParams) -> Signal:
        """A signal that mirrors what is above 1 - `depth` back below it."""
        return Folded(signal, params.depth)

    return fold


def test_a_plugin_shaper_registers_and_shapes_with_its_values_or_defaults(fold):
    ramp = lib.constant(value=0.75)
    assert lib.fold(ramp, depth=0.5).sample(1.0).tolist() == 0.25  # 2 * 0.5 - 0.75
    assert lib.fold(ramp).sample(1.0).tolist() == 0.25  # the default depth is 0.5
    assert lib.fold(ramp, depth=0.0).sample(1.0).tolist() == 0.75  # below the fold at 1: unchanged


@pytest.mark.parametrize(
    ("values", "named"),
    [
        pytest.param({"depth": 1.5}, "depth", id="above its range"),
        pytest.param({"depth": True}, "depth", id="a bool for a number"),
        pytest.param({"width": 0.5}, "width", id="an unknown name"),
    ],
)
def test_a_plugin_shaper_refuses_a_bad_value_naming_itself_and_the_field(fold, values, named, reports):
    with pytest.raises(ParameterError, match="shaper 'fold'") as refused:
        lib.fold(lib.constant(), **values)
    reports(refused.value, named)


def test_a_plugin_shaper_documents_itself_in_help(fold):
    assert lib.fold.__doc__ == (
        "shaper fold(signal, **values) -> signal\n"
        "\n"
        "A signal that mirrors what is above 1 - `depth` back below it.\n"
        "\n"
        "Parameters:\n"
        "  depth (fraction, 0 to 1), default 0.5\n"
        "      How far down from 1 the fold is."
    )


def test_a_plugin_shaper_is_listed_as_json_for_another_tool(fold, capsys):
    assert main(["list", "--json", "fold"]) == 0
    assert json.loads(capsys.readouterr().out) == {
        "fold": {
            "kind": "shaper",
            "summary": "A signal that mirrors what is above 1 - `depth` back below it.",
            "returns": "signal",
            "inputs": {},
            "parameters": {
                "additionalProperties": False,
                "description": "Where the signal folds back.",
                "properties": {
                    "depth": {
                        "advanced": False,
                        "cyclic": False,
                        "default": 0.5,
                        "description": "How far down from 1 the fold is.",
                        "maximum": 1,
                        "minimum": 0,
                        "title": "Depth",
                        "type": "number",
                        "unit": "fraction",
                    }
                },
                "title": "FoldParams",
                "type": "object",
            },
        }
    }
