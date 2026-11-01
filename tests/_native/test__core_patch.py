"""The extension's patches (`Patch`, `patch_schema`): a patch read and built by the Rust core, against a registry that
holds the core's things and Python ones.

An architecture test of the extension's own interface. Every expected number is worked out by hand: a series of 10
samples a second read linearly, slewed at 10 samples a second (0.5 s to climb by 1 is 0.2 a sample), folded at
1 - depth.
"""

import math
import sys
import threading
import tomllib
from collections.abc import Iterator, Mapping
from concurrent.futures import ThreadPoolExecutor
from typing import Any

import numpy as np
import pytest

from bullterrier_control import Bundle, Event
from bullterrier_control._native import _core  # noqa: PLC2701  architecture: the boundary a patch is read through
from bullterrier_control.errors import FormatError, ParameterError, UnknownNameError
from bullterrier_control.plugin import Finite, Params, Signal, param

TIMEOUT = 60.0  # seconds a thread test waits before it fails as a deadlock
HEADER = {"format": "bullterrier-control/patch", "version": 1}
ENERGY = Bundle.parse(
    {
        "format": "bullterrier-control/bundle",
        "version": 1,
        "series": {"energy": {"rate": 10, "values": [0.0, *[1.0] * 9], "unit": "ratio"}},
        "events": {"kick": [{"t": 0.5, "strength": 0.9}, {"t": 1.0, "strength": 0.4}]},
    }
)


class FoldParams(Params):
    """Where the signal folds back."""

    depth: Finite = param(0.5, doc="How far down from 1 the fold is.", unit="fraction", min=0, max=1)


FOLD = _core.ParamTable(
    [{"name": "depth", "kind": "float", "default": 0.5, "doc": "The fold.", "unit": "fraction", "min": 0, "max": 1}],
    name="FoldParams",
)


class Folded(Signal):
    """A signal with what lies above the fold mirrored below it (docs/api.md, section 9)."""

    def __init__(self, signal: Signal, depth: float) -> None:
        self._signal, self._line = signal, 1.0 - depth

    def compute(self, times):
        values = self._signal.compute(times)
        return np.where(values > self._line, 2.0 * self._line - values, values)


def fold(signal: Signal, params: FoldParams) -> Signal:
    """A signal that mirrors what is above 1 - `depth` back below it."""
    return Folded(signal, params.depth)


class Nothing(Params):
    """No parameters."""


NOTHING = _core.ParamTable([], name="Nothing")


def last_point(params: Nothing, *, points: tuple[_core.Keyframe, ...]) -> Signal:
    """The value of the last keyframe at or before each time."""

    class Held(Signal):
        def compute(self, times):
            at = np.searchsorted([point.t for point in points], times, side="right") - 1
            return np.array([point.value for point in points])[np.maximum(at, 0)]

    return Held()


@pytest.fixture
def registry():
    """The core's things, `fold` (a Python shaper) and `last_point` (a Python source with an input)."""
    made = _core.Registry(builtin=True)
    made.register(fold, kind="shaper", params=FoldParams, table=FOLD, inputs={}, returns="signal")
    made.register(last_point, kind="source", params=Nothing, table=NOTHING, inputs={"points": True}, returns="signal")
    return made


GLOW = """\
format = "bullterrier-control/patch"
version = 1

[glow]
use = "slew"
signal = "series.energy"
rise = 0.5
rate = 10

[folded]
use = "fold"
signal = "glow"
depth = 0.5
"""


# ---- Python inside a patch the core reads ----


def test_a_python_shaper_builds_inside_a_patch_read_from_toml(registry):
    built = _core.Patch.from_toml(GLOW, registry=registry).build(ENERGY)
    assert list(built) == ["glow", "folded"]
    times = [0.0, 0.15, 0.25, 0.35, 0.45, 0.55]
    assert built["glow"].sample(times).tolist() == pytest.approx([0.0, 0.2, 0.4, 0.6, 0.8, 1.0])
    # above 0.5 folds back: 0.6 to 0.4, 0.8 to 0.2, 1.0 to 0.0
    assert built["folded"].sample(times).tolist() == pytest.approx([0.0, 0.2, 0.4, 0.4, 0.2, 0.0])


def test_a_python_source_takes_its_inline_input_as_keyframes(registry):
    points = [{"t": 0.0, "value": 2.0}, {"t": 1.0, "value": -1.0}]
    patch = _core.Patch.parse({**HEADER, "held": {"use": "last_point", "points": points}}, registry=registry)
    assert patch.build()["held"].sample([0.5, 1.0, 7.0]).tolist() == [2.0, -1.0, -1.0]


def test_an_exception_a_python_body_raises_while_built_reaches_the_caller_unchanged(registry):
    raised = RuntimeError("the plugin broke")

    def broken(params: Nothing) -> Signal:
        raise raised

    registry.register(broken, kind="source", params=Nothing, table=NOTHING, inputs={}, returns="signal")
    with pytest.raises(RuntimeError) as refused:
        _core.Patch.parse({**HEADER, "low": {"use": "broken"}}, registry=registry).build()
    assert refused.value is raised


def test_an_invalid_error_a_body_raises_while_built_keeps_its_kind_and_names_the_node(registry):
    def broken(params: Nothing) -> Signal:
        raise ParameterError("it cannot be made", (("depth", "is too deep"),))

    registry.register(broken, kind="source", params=Nothing, table=NOTHING, inputs={}, returns="signal")
    with pytest.raises(ParameterError, match="it cannot be made") as refused:
        _core.Patch.parse({**HEADER, "low": {"use": "broken"}}, registry=registry).build()
    assert str(refused.value).startswith("low: ")
    assert refused.value.problems == (("low.depth", "is too deep"),)


def test_a_python_operation_gives_its_events_to_the_node_below(registry):
    def louder(params: Nothing, *, events: tuple[Event, ...]) -> tuple[Event, ...]:
        return tuple(event for event in events if event.strength > 0.5)

    registry.register(
        louder, kind="operation", params=Nothing, table=NOTHING, inputs={"events": True}, returns="events"
    )
    data = {**HEADER, "loud": {"use": "louder", "events": "events.kick"}, "kept": {"use": "select", "events": "loud"}}
    built = _core.Patch.parse(data, registry=registry).build(ENERGY)
    assert [(event.t, event.strength) for event in built["kept"]] == [(0.5, 0.9)]
    assert type(built["loud"]) is tuple


# ---- reading: what a Python mapping holds ----


def test_the_nodes_are_read_in_the_order_the_mapping_gives_them(registry):
    smooth = {"use": "slew", "signal": "series.energy"}
    after = {"use": "follow", "signal": "smooth"}
    patch = _core.Patch.parse({**HEADER, "smooth": smooth, "after": after}, registry=registry)
    assert list(patch.nodes) == ["smooth", "after"]
    with pytest.raises(FormatError) as refused:
        _core.Patch.parse({**HEADER, "after": after, "smooth": smooth}, registry=registry)
    assert [path for path, _ in refused.value.problems] == ["after.signal"]
    assert "a node below" in str(refused.value)


@pytest.mark.parametrize(
    ("value", "path", "rule"),
    [
        pytest.param({"use": "constant", "value": math.nan}, "a.value", "must be finite (got nan)", id="NaN"),
        pytest.param({"use": "constant", "value": -math.inf}, "a.value", "must be finite (got -inf)", id="an infinity"),
        pytest.param({"use": "constant", "value": 2**64}, "a.value", "beyond 64 bits", id="an integer beyond 64 bits"),
        pytest.param({"use": "constant", "value": {1}}, "a.value", "invalid type: set", id="a set"),
        pytest.param(
            {"use": "select", "events": [{"t": 0.5}, {"t": math.nan}]}, "a.events[1].t", "finite", id="deep inside"
        ),
        pytest.param({"use": "constant", 3: 1}, "a", "a key must be text", id="a key of a node that is not text"),
    ],
)
def test_a_value_json_cannot_hold_is_refused_at_its_path(registry, value, path, rule):
    with pytest.raises(FormatError) as refused:
        _core.Patch.parse({**HEADER, "a": value}, registry=registry)
    assert [problem for problem in refused.value.problems if problem[0] == path], refused.value.problems
    assert rule in str(refused.value)
    assert str(refused.value).startswith("Patch: ")


@pytest.mark.parametrize("key", [1, None, (1, 2)], ids=["an int", "none", "a tuple"])
def test_a_top_level_key_that_is_not_text_is_refused_naming_it(registry, key):
    with pytest.raises(FormatError) as refused:
        _core.Patch.parse({**HEADER, key: {"use": "constant"}}, registry=registry)
    assert str(key) in [path for path, _ in refused.value.problems]


def test_every_problem_is_one_format_error_listing_each_path(registry):
    data = {**HEADER, "a": {"use": "nosuch"}, "b": {"use": "select", "events": "events.Kick", "threshold": -1}}
    with pytest.raises(FormatError) as refused:
        _core.Patch.parse(data, registry=registry)
    assert [path for path, _ in refused.value.problems] == ["a.use", "b.events", "b.threshold"]


def test_a_mapping_that_raises_while_read_raises_unchanged(registry):
    class Exploding(Mapping[str, object]):
        def __iter__(self) -> Iterator[str]:
            return iter(["use"])

        def __len__(self) -> int:
            return 1

        def __getitem__(self, key: str) -> object:
            raise LookupError("no items today")

    with pytest.raises(LookupError, match="no items today"):
        _core.Patch.parse({**HEADER, "a": Exploding()}, registry=registry)


def test_a_patch_is_a_mapping(registry):
    with pytest.raises(TypeError, match="list"):
        _core.Patch.parse([("format", "x")], registry=registry)  # type: ignore[arg-type]  # the point


def test_a_toml_text_and_the_same_table_in_python_are_one_patch(registry):
    assert _core.Patch.from_toml(GLOW, registry=registry) == _core.Patch.parse(tomllib.loads(GLOW), registry=registry)
    other = GLOW.replace("depth = 0.5", "depth = 0.25")
    assert _core.Patch.from_toml(GLOW, registry=registry) != _core.Patch.from_toml(other, registry=registry)


def test_a_text_that_is_not_toml_is_a_format_error_saying_where(registry):
    with pytest.raises(FormatError, match=r"^not TOML \(line 1, column 10: "):
        _core.Patch.from_toml("format = \n", registry=registry)


def test_each_node_says_what_it_uses_and_gives(registry):
    patch = _core.Patch.from_toml(GLOW, registry=registry)
    assert {name: (node.use, node.returns) for name, node in patch.nodes.items()} == {
        "glow": ("slew", "signal"),
        "folded": ("fold", "signal"),
    }


# ---- building ----


def test_building_some_nodes_builds_only_those_and_what_they_use(registry):
    patch = _core.Patch.from_toml(GLOW, registry=registry)
    assert patch.build(ENERGY, only=["folded"]).keys() == {"folded"}
    with pytest.raises(UnknownNameError, match="unknown node 'halo'; the patch has: glow, folded"):
        patch.build(ENERGY, only={"halo"})
    with pytest.raises(TypeError):
        patch.build(ENERGY, only="folded")  # text is not a list of names


def test_a_path_into_a_bundle_needs_one_holding_it(registry):
    patch = _core.Patch.from_toml(GLOW, registry=registry)
    with pytest.raises(FormatError, match=r"^glow\.signal: 'series\.energy' is a path into a bundle") as refused:
        patch.build()
    assert refused.value.problems == (("glow.signal", "'series.energy' is a path into a bundle, and none was given"),)


def test_the_core_alone_builds_a_patch_of_its_own_things():
    data = {**HEADER, "level": {"use": "constant", "value": 0.25}, "strong": {"use": "select", "events": "events.kick"}}
    built = _core.Patch.parse(data, registry=_core.Registry(builtin=True)).build(ENERGY)
    assert built["level"].sample(3.0).tolist() == 0.25
    assert [event.t for event in built["strong"]] == [0.5, 1.0]


def test_the_schema_of_a_patch_states_its_header_and_its_node_names():
    schema = _core.patch_schema()
    assert schema["properties"] == {"format": {"const": "bullterrier-control/patch"}, "version": {"const": 1}}
    assert schema["$defs"]["node"]["required"] == ["use"]


# ---- threads and the GIL ----


def test_a_long_build_of_rust_things_lets_another_python_thread_run():
    text = "format = 'bullterrier-control/patch'\nversion = 1\n" + "".join(
        f"[n{index}]\nuse = 'constant'\nvalue = 0.5\n" for index in range(100_000)
    )
    patch = _core.Patch.from_toml(text, registry=_core.Registry(builtin=True))
    entered, seen = threading.Event(), {}

    def long_build() -> None:
        entered.set()
        seen["built"] = patch.build(only=[f"n{index}" for index in range(0, 100_000, 1000)])

    interval = sys.getswitchinterval()
    # no thread gives up the GIL on a timer: the main thread runs while the builder builds only if it lets go of it
    sys.setswitchinterval(1000.0)
    try:
        builder = threading.Thread(target=long_build, daemon=True)
        builder.start()
        assert entered.wait(TIMEOUT)
        ran_during_the_build = "built" not in seen
        builder.join(TIMEOUT)
    finally:
        sys.setswitchinterval(interval)
    assert ran_during_the_build
    assert len(seen["built"]) == 100


def test_sixteen_threads_building_one_patch_with_python_inside_get_equal_values(registry):
    patch = _core.Patch.from_toml(GLOW, registry=registry)
    barrier = threading.Barrier(16)

    def build(_: int) -> list[float]:
        barrier.wait(TIMEOUT)
        return patch.build(ENERGY)["folded"].sample([0.35, 0.45]).tolist()

    with ThreadPoolExecutor(max_workers=16) as pool:
        results: list[Any] = [future.result(timeout=TIMEOUT) for future in [pool.submit(build, i) for i in range(16)]]
    assert results == [pytest.approx([0.4, 0.2])] * 16


def test_a_body_may_read_and_build_a_patch_while_a_patch_builds_it(registry):
    def nested(params: Nothing) -> Signal:
        inner = _core.Patch.parse({**HEADER, "level": {"use": "constant", "value": 0.75}}, registry=registry)
        return inner.build()["level"]

    registry.register(nested, kind="source", params=Nothing, table=NOTHING, inputs={}, returns="signal")
    outer = _core.Patch.parse({**HEADER, "outer": {"use": "nested"}}, registry=registry)
    assert outer.build()["outer"].sample(0.0).tolist() == 0.75
