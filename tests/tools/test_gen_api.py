"""tools/gen_api.py writes the typed face of `lib` (`lib.pyi`) and the chain methods of `Signal` (`_chain.py`).

An architecture test of the tooling and of the extension point: the drift test's subject is every built-in source,
shaper and operation, so a new one fails here until `uv run poe api` has rewritten the two files. The other tests run
the generator on a small registry of throwaways, made to look built-in (only built-ins are written).
"""

import importlib.util
from collections.abc import Callable
from pathlib import Path
from typing import Literal

import pytest

from bullterrier_control import Event, registry
from bullterrier_control.plugin import (
    Constant,
    Finite,
    Params,
    Signal,
    Switch,
    Text,
    Whole,
    operation,
    param,
    shaper,
    source,
)

ROOT = Path(__file__).resolve().parents[2]
LIB = "src/bullterrier_control/lib.pyi"
CHAIN = "src/bullterrier_control/_chain.py"


def tool():
    spec = importlib.util.spec_from_file_location("gen_api", ROOT / "tools" / "gen_api.py")
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


gen_api = tool()


def test_the_committed_files_are_what_the_built_ins_make(capsys):
    assert gen_api.main(["--check"]) == 0, capsys.readouterr().err


class WarpParams(Params):
    """How a warp bends."""

    depth: Finite = param(1, doc="How far it bends, 0 to 1.", unit="fraction", min=0, max=1)
    taps: Whole = param(3, doc="How many taps, 1 to 8.", unit="count", min=1, max=8)
    mode: Literal["up", "down"] = param("up", doc="Which way: 'up' or 'down'.")
    only: Literal["one"] = param("one", doc="The one choice: 'one'.")
    on: Switch = param(False, doc="Whether it bends at all.")
    tag: Text = param("", doc="A name for it.")


def built_in[F: Callable[..., object]](function: F) -> F:
    """`function` as if a module of bullterrier_control.builtin defined it."""
    function.__module__ = "bullterrier_control.builtin.probe"
    return function


@pytest.fixture
def small(empty_registry) -> dict[str, str]:
    """The files written for a registry of one built-in shaper, source and operation each, and one plugin shaper."""

    def warp(signal: Signal, params: WarpParams, *, values: tuple[float, ...]) -> Signal:
        """A signal bent by `depth`.

        More than the summary."""
        return signal

    def level(params: WarpParams) -> Signal:
        """A level."""
        return Constant(0.0)

    def pick(params: WarpParams, *, events: tuple[Event, ...]) -> tuple[Event, ...]:
        """The events it picks."""
        return events

    def fold(signal: Signal, params: WarpParams) -> Signal:
        """A plugin's shaper."""
        return signal

    shaper(params=WarpParams)(built_in(warp))
    source(params=WarpParams)(built_in(level))
    operation(params=WarpParams)(built_in(pick))
    shaper(params=WarpParams)(fold)
    return {path.as_posix(): text for path, text in gen_api.render(registry.all_registered()).items()}


def test_the_two_files_compile(small):
    assert sorted(small) == [CHAIN, LIB]
    for path, text in small.items():
        compile(text, path, "exec")


def test_a_chain_method_takes_the_inputs_and_parameters_by_keyword_with_their_types_and_defaults(small):
    assert (
        "    def warp(\n"
        "        self,\n"
        "        *,\n"
        "        values: Sequence[float],\n"
        "        depth: float = 1.0,\n"
        "        taps: int = 3,\n"
        '        mode: Literal["up", "down"] = "up",\n'
        '        only: Literal["one"] = "one",\n'
        "        on: bool = False,\n"
        '        tag: str = "",\n'
        "    ) -> S:\n"  # Signal inherits Chain["Signal"]: S is Signal
    ) in small[CHAIN]


def test_a_chain_method_applies_the_shaper_of_its_name_with_every_value_under_its_own_name(small):
    assert (
        '        return self._apply("warp", values=values, depth=depth, taps=taps, mode=mode, only=only, on=on,'
        " tag=tag)\n"
    ) in small[CHAIN]


def test_a_chain_method_documents_the_summary_and_each_parameter_with_unit_range_and_default(small):
    assert '        """A signal bent by `depth`.\n' in small[CHAIN]
    assert "            depth: fraction, 0 to 1, default 1.0. How far it bends, 0 to 1.\n" in small[CHAIN]
    assert "            mode: default 'up'. Which way: 'up' or 'down'.\n" in small[CHAIN]


@pytest.mark.parametrize(
    "line",
    [
        "def warp(\n    signal: Signal,\n    /,\n    *,\n    values: Sequence[float],\n    depth: float = 1.0,\n",
        "def level(\n    *,\n    depth: float = 1.0,\n",
        "def pick(\n    *,\n    events: Sequence[Event | Mapping[str, object]],\n    depth: float = 1.0,\n",
        '    tag: str = "",\n) -> Signal:\n',
        '    tag: str = "",\n) -> tuple[Event, ...]:\n',
        "def __getattr__(name: str) -> Registered[Any, Any]: ...\n",
    ],
    ids=[
        "a shaper takes its signal first",
        "a source takes keywords only",
        "an operation takes its data as an input",
        "a source or shaper gives a signal",
        "an operation gives what it declares",
        "any other name is the registry's entry",
    ],
)
def test_lib_has_a_typed_function_for_each_built_in(small, line):
    assert line in small[LIB]


def test_a_plugin_is_left_out_of_both_files(small):
    assert "fold" not in small[LIB]
    assert "fold" not in small[CHAIN]


@pytest.mark.parametrize("name", ["sample", "pipe", "compute"])
def test_a_built_in_shaper_named_like_an_attribute_of_signal_is_refused_naming_it(empty_registry, name):
    def clash(signal: Signal, params: WarpParams) -> Signal:
        """Would hide a method of Signal."""
        return signal

    clash.__name__ = name
    shaper(params=WarpParams)(built_in(clash))
    with pytest.raises(ValueError, match=name):
        gen_api.render(registry.all_registered())


def test_check_passes_on_the_files_just_written_and_fails_naming_one_that_differs(tmp_path, capsys):
    assert gen_api.main(["--root", str(tmp_path)]) == 0
    assert gen_api.main(["--check", "--root", str(tmp_path)]) == 0
    chain = tmp_path / CHAIN
    chain.write_text(chain.read_text(encoding="utf-8") + "# edited\n", encoding="utf-8")
    capsys.readouterr()
    assert gen_api.main(["--check", "--root", str(tmp_path)]) == 1
    said = capsys.readouterr().err
    assert CHAIN in said
    assert "uv run poe api" in said
    assert LIB not in said


def test_check_fails_naming_a_file_that_is_missing(tmp_path, capsys):
    assert gen_api.main(["--root", str(tmp_path)]) == 0
    (tmp_path / LIB).unlink()
    assert gen_api.main(["--check", "--root", str(tmp_path)]) == 1
    assert LIB in capsys.readouterr().err
