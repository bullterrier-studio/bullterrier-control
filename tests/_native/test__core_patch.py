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
from bullterrier_control.plugin import Constant, Finite, Params, Signal, param

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
    made.register(fold, kind="shaper", params=FoldParams, inputs={}, returns="signal")
    made.register(last_point, kind="source", params=Nothing, inputs={"points": True}, returns="signal")
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

    registry.register(broken, kind="source", params=Nothing, inputs={}, returns="signal")
    with pytest.raises(RuntimeError) as refused:
        _core.Patch.parse({**HEADER, "low": {"use": "broken"}}, registry=registry).build()
    assert refused.value is raised


def test_an_invalid_error_a_body_raises_while_built_keeps_its_kind_and_names_the_node(registry):
    def broken(params: Nothing) -> Signal:
        raise ParameterError("it cannot be made", (("depth", "is too deep"),))

    registry.register(broken, kind="source", params=Nothing, inputs={}, returns="signal")
    with pytest.raises(ParameterError, match="it cannot be made") as refused:
        _core.Patch.parse({**HEADER, "low": {"use": "broken"}}, registry=registry).build()
    assert str(refused.value).startswith("low: ")
    assert refused.value.problems == (("low.depth", "is too deep"),)


def test_a_python_operation_gives_its_events_to_the_node_below(registry):
    def louder(params: Nothing, *, events: tuple[Event, ...]) -> tuple[Event, ...]:
        return tuple(event for event in events if event.strength > 0.5)

    registry.register(louder, kind="operation", params=Nothing, inputs={"events": True}, returns="events")
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
        f"[n{index}]\nuse = 'constant'\nvalue = 0.5\n" for index in range(30_000)
    )
    patch = _core.Patch.from_toml(text, registry=_core.Registry(builtin=True))
    entered, seen = threading.Event(), {}

    def long_build() -> None:
        entered.set()
        seen["built"] = patch.build(only=[f"n{index}" for index in range(0, 30_000, 100)])

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
    assert len(seen["built"]) == 300


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

    registry.register(nested, kind="source", params=Nothing, inputs={}, returns="signal")
    outer = _core.Patch.parse({**HEADER, "outer": {"use": "nested"}}, registry=registry)
    assert outer.build()["outer"].sample(0.0).tolist() == 0.75


# ---- reading a node: what it is built from ----


def written(reference: Any) -> tuple[str, str, str] | None:
    """A reference as (text, root, name), `None` for no reference."""
    return None if reference is None else (reference.text, reference.root, reference.name)


def test_a_node_says_what_it_is_built_from_and_with_which_values(registry):
    data = {
        **HEADER,
        "strong": {"use": "select", "events": "events.kick", "threshold": 0.5},
        "glow": {"use": "slew", "signal": "series.energy", "rise": 1},
        "held": {"use": "last_point", "points": [{"t": 0.0, "value": 2.0}, {"t": 1.0, "value": -1.0}]},
        "next": {"use": "fold", "signal": "glow", "depth": 0.25},
    }
    nodes = _core.Patch.parse(data, registry=registry).nodes
    strong, glow, held, after = (nodes[name] for name in ("strong", "glow", "held", "next"))
    # a reference is where the data is: a bundle entry, or a node above
    assert [written(r) for r in (glow.signal, strong.inputs["events"], after.signal)] == [
        ("series.energy", "series", "energy"),
        ("events.kick", "events", "kick"),
        ("glow", "node", "glow"),
    ]
    assert (strong.signal, held.signal) == (None, None)
    # what is written inline is the data, checked as its kind
    assert [(point.t, point.value) for point in held.inputs["points"]] == [(0.0, 2.0), (1.0, -1.0)]
    # the values are those written: a whole number stays whole, and nothing is added for what was left out
    assert (dict(strong.parameters), dict(glow.parameters), dict(after.parameters), dict(held.parameters)) == (
        {"threshold": 0.5},
        {"rise": 1},
        {"depth": 0.25},
        {},
    )
    assert type(dict(glow.parameters)["rise"]) is int


def test_a_grid_is_a_reference_or_inline_data_and_a_reference_is_equal_by_what_it_names(registry):
    grid = {"beats": [0.0, 0.5, 1.0], "meter": 2, "first_downbeat": 0}

    def beat(params: Nothing, *, grid: object = None) -> Signal:
        return last_point(params, points=())

    registry.register(beat, kind="source", params=Nothing, inputs={"grid": False}, returns="signal")
    nodes = _core.Patch.parse(
        {**HEADER, "a": {"use": "beat", "grid": "grid"}, "b": {"use": "beat", "grid": grid}}, registry=registry
    ).nodes
    named, inline = nodes["a"].inputs["grid"], nodes["b"].inputs["grid"]
    assert (named.text, named.root, named.name) == ("grid", "grid", "")
    assert inline.beats == (0.0, 0.5, 1.0)
    other = (
        _core.Patch.parse({**HEADER, "x": {"use": "beat", "grid": "grid"}}, registry=registry).nodes["x"].inputs["grid"]
    )
    assert named == other
    assert named != nodes["a"].signal  # nothing: None is not a reference
    assert len({named, other}) == 1


def test_the_inputs_and_values_of_a_node_cannot_be_changed(registry):
    node = _core.Patch.parse({**HEADER, "glow": {"use": "slew", "signal": "series.energy"}}, registry=registry).nodes[
        "glow"
    ]
    with pytest.raises(TypeError):
        node.parameters["rise"] = 1  # type: ignore[index]
    with pytest.raises(TypeError):
        node.inputs["points"] = ()  # type: ignore[index]


# ---- saving: a graph made by the registry's things, as a patch ----


def things(registry: Any) -> tuple[Any, Any, Any]:
    return registry.get("constant"), registry.get("slew"), registry.get("follow")


CHAIN = """\
format = "bullterrier-control/patch"
version = 1

[glow_1]
use = "constant"
value = 0.5

[glow_2]
use = "slew"
signal = "glow_1"
rise = 0.25
fall = 0.1
rate = 1000.0

[glow]
use = "follow"
signal = "glow_2"
attack = 0.0
release = 0.5
rate = 2000.0
"""


def test_a_chain_made_by_the_registry_is_saved_as_the_hand_written_patch(registry):
    constant, slew, follow = things(registry)
    glow = follow(slew(constant(value=0.5), rise=0.25), attack=0.0, release=0.5, rate=2000.0)
    saved = _core.Patch.from_signals({"glow": glow}.items(), registry=registry)
    assert saved.to_toml() == CHAIN
    assert list(saved.nodes) == ["glow_1", "glow_2", "glow"]
    assert written(saved.nodes["glow_2"].signal) == ("glow_1", "node", "glow_1")
    assert dict(saved.nodes["glow_2"].parameters) == {"rise": 0.25, "fall": 0.1, "rate": 1000.0}


def test_a_saved_patch_builds_what_the_graph_samples_and_reads_back_equal(registry):
    constant, slew, follow = things(registry)
    glow = follow(slew(constant(value=0.5), rise=0.25), attack=0.0, release=0.5, rate=2000.0)
    saved = _core.Patch.from_signals({"glow": glow}.items(), registry=registry)
    times = [-1.0, 0.0, 0.001, 0.25, 0.3, 1.0, 7.5]
    assert saved.build()["glow"].sample(times).tolist() == glow.sample(times).tolist()
    again = _core.Patch.from_toml(saved.to_toml(), registry=registry)
    assert again == saved
    assert again.build()["glow"].sample(times).tolist() == glow.sample(times).tolist()


def test_the_signals_are_saved_in_the_order_the_items_give_them_and_shared_upstream_is_one_node(registry):
    constant, slew, _ = things(registry)
    base = slew(constant(value=0.25))
    fast, slow = slew(base, rise=0.01), slew(base, rise=1.0)
    saved = _core.Patch.from_signals([("slow", slow), ("fast", fast)], registry=registry)
    assert list(saved.nodes) == ["slow_1", "slow_2", "slow", "fast"]
    assert written(saved.nodes["fast"].signal) == ("slow_2", "node", "slow_2")


def test_a_save_names_the_registry_it_is_for_by_keyword(registry):
    constant, slew, _ = things(registry)
    smooth = slew(constant(value=0.75), rise=0.5)
    with pytest.raises(TypeError, match="registry"):
        _core.Patch.from_signals({"smooth": smooth}.items())  # type: ignore[call-arg]  # left out: the case
    with pytest.raises(TypeError):
        _core.Patch.from_signals({"smooth": smooth}.items(), registry)  # type: ignore[misc]  # positional: the case


def test_a_python_thing_is_saved_and_read_back_in_the_registry_that_holds_it(registry):
    constant, slew, _ = things(registry)
    folded = registry.get("fold")(slew(constant(value=0.9), rise=0.5), depth=0.25)
    text = _core.Patch.from_signals({"folded": folded}.items(), registry=registry).to_toml()
    assert text.endswith('[folded]\nuse = "fold"\nsignal = "folded_2"\ndepth = 0.25\n')
    rebuilt = _core.Patch.from_toml(text, registry=registry).build()["folded"]
    times = [0.0, 0.25, 0.5, 1.0, 3.0]
    assert rebuilt.sample(times).tolist() == folded.sample(times).tolist()
    # 0.9 is above the fold at 0.75: mirrored to 0.6
    assert rebuilt.sample([3.0]).tolist() == pytest.approx([0.6])
    # a registry that does not hold that very thing cannot take the graph
    unheld = r"^cannot save as a patch: folded: is made by 'fold', which the registry it is saved for does not hold"
    with pytest.raises(FormatError, match=unheld):
        _core.Patch.from_signals({"folded": folded}.items(), registry=_core.Registry(builtin=True))


class Computed(Signal):
    """A signal a Python `compute` makes."""

    def compute(self, times):
        return np.asarray(times) * 2.0


def test_what_a_patch_cannot_hold_is_refused_at_its_path_saying_why(registry):
    constant, slew, follow = things(registry)
    smooth = slew(constant(value=0.5))
    rows: dict[str, tuple[dict[str, Any], list[tuple[str, str]]]] = {
        "a constant of Python": ({"level": Constant(0.5)}, [("level", "holds values given as they are")]),
        "values given": ({"level": _core.from_values([0.0, 1.0], 2.0)}, [("level", "holds values given as they are")]),
        "a compute": ({"level": Computed()}, [("level", "is made by a callback")]),
        "a sum": ({"mix": smooth + smooth}, [("mix", "is made by an operator")]),
        "a shaper of a sum": (
            {"glow": follow(smooth * 2.0)},
            [("glow.signal", "is made by an operator")],
        ),
        "a shaper of values": (
            {"glow": follow(slew(Constant(1.0)))},
            [("glow.signal.signal", "holds values given as they are")],
        ),
        "two at once": (
            {"a": Constant(1.0), "b": Computed()},
            [("a", "holds values"), ("b", "is made by a callback")],
        ),
        "a name no node may have": ({"Glow": smooth}, [("Glow", "is not a node name")]),
        "a reserved name": ({"grid": smooth}, [("grid", "is not a node name")]),
    }
    for row, (graph, wanted) in rows.items():
        with pytest.raises(FormatError, match=r"^cannot save as a patch: ") as refused:
            _core.Patch.from_signals(graph.items(), registry=registry)
        got = [(path, rule) for path, rule in refused.value.problems]
        assert len(got) == len(wanted), row
        for (path, rule), (want_path, want_rule) in zip(got, wanted, strict=True):
            assert path == want_path, row
            assert want_rule in rule, row


def test_a_refusal_reads_as_path_and_reason_pairs_in_the_message(registry):
    with pytest.raises(FormatError) as refused:
        _core.Patch.from_signals({"a": Constant(1.0)}.items(), registry=registry)
    path, rule = refused.value.problems[0]
    assert str(refused.value) == f"cannot save as a patch: {path}: {rule}"


@pytest.mark.parametrize(
    ("items", "error", "match"),
    [
        pytest.param([("glow", 0.5)], TypeError, "signal", id="a number is no signal"),
        pytest.param([("glow",)], TypeError, "pair", id="an item that is no pair"),
        pytest.param([(1, Constant(1.0))], TypeError, "name", id="a name that is not text"),
        pytest.param(5, TypeError, "items", id="items that are not iterable"),
    ],
)
def test_items_that_are_no_pairs_of_a_name_and_a_signal_are_a_type_error(registry, items, error, match):
    with pytest.raises(error, match=match):
        _core.Patch.from_signals(items, registry=registry)


def test_saving_nothing_is_a_patch_with_no_node(registry):
    saved = _core.Patch.from_signals([], registry=registry)
    assert saved.to_toml() == 'format = "bullterrier-control/patch"\nversion = 1\n'
    assert saved.nodes == {}


def test_a_patch_read_from_toml_is_written_as_it_reads(registry):
    patch = _core.Patch.from_toml(GLOW, registry=registry)
    assert tomllib.loads(patch.to_toml()) == tomllib.loads(GLOW)
    assert _core.Patch.from_toml(patch.to_toml(), registry=registry) == patch
