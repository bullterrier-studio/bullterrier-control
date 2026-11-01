"""The Rust extension's boundary, `_native._core`: the parameter tables (`ParamTable`, a built-in's through its
`Thing.describe_params`) the package's parameters are read through.

An architecture test: the extension is internal (the package reaches it through its parameter classes, and only this
test calls it directly). A table refuses a declaration or values with a `ParameterError` listing every rule broken.
"""

import math
import subprocess
import sys

import numpy as np
import pytest

from bullterrier_control import Series, registry
from bullterrier_control._native import _core  # noqa: PLC2701  architecture: the boundary the shapers call
from bullterrier_control.errors import FormatError, ParameterError

# ---- parameter tables ----

CORE = _core.Registry(builtin=True)  # the things the Rust core runs
BUILT_INS = [
    "beats", "clip", "constant", "delay", "envelope", "every", "follow", "gate", "hold", "keyframes", "latch", "lfo",
    "noise", "normalise", "quantize", "remap", "select", "shift", "slew", "steps", "thin",
]  # fmt: skip
RISE = "Seconds the output takes to climb by 1; larger is slower. 0 follows a rise at once."
FALL = "Seconds the output takes to fall by 1; larger is slower. 0 follows a fall at once."
RATE = "Internal samples per second; the output is exact to one sample. Raising it is more exact and slower."
# slew's declaration as a plugin would write it, from its doc comments and attributes
SLEW_SPEC: list[dict[str, object]] = [
    {"name": "rise", "kind": "float", "default": 0.1, "doc": RISE, "unit": "seconds", "min": 0, "max": 3600},
    {"name": "fall", "kind": "float", "default": 0.1, "doc": FALL, "unit": "seconds", "min": 0, "max": 3600},
    {"name": "rate", "kind": "float", "default": 1000.0, "doc": RATE, "unit": "hertz", "min": 10, "max": 48000,
     "advanced": True},
]  # fmt: skip
SLEW_DESCRIBED = {
    "rise": {"type": "number", "default": 0.1, "description": RISE, "title": "Rise", "unit": "seconds", "minimum": 0.0,
             "maximum": 3600.0, "advanced": False, "cyclic": False, "range": "0 to 3600"},
    "fall": {"type": "number", "default": 0.1, "description": FALL, "title": "Fall", "unit": "seconds", "minimum": 0.0,
             "maximum": 3600.0, "advanced": False, "cyclic": False, "range": "0 to 3600"},
    "rate": {"type": "number", "default": 1000.0, "description": RATE, "title": "Rate", "unit": "hertz",
             "minimum": 10.0, "maximum": 48000.0, "advanced": True, "cyclic": False, "range": "10 to 48000"},
}  # fmt: skip
# a table with a field of every kind; `span` (advanced) reaches far beyond 64 bits, so only the integer rule refuses one
EVERY_KIND: list[dict[str, object]] = [
    {"name": "rate", "kind": "float", "default": 1.0, "doc": "Cycles a second.", "unit": "hertz", "min": 0, "max": 10},
    {"name": "span", "kind": "float", "default": 0.0, "doc": "A wide number.", "unit": "ratio", "min": -1e30,
     "max": 1e30, "advanced": True},
    {"name": "seed", "kind": "whole", "default": 3, "doc": "Which noise.", "unit": "count", "min": 0,
     "max": 4294967295},
    {"name": "on", "kind": "switch", "default": False, "doc": "Whether it runs."},
    {"name": "label", "kind": "text", "default": "a", "doc": "A label."},
    {"name": "shape", "kind": "choice", "choices": ["sine", "saw"], "default": "sine", "doc": "'sine' or 'saw'."},
    {"name": "phase", "kind": "float", "default": 0.0, "doc": "Where it starts.", "unit": "cycles", "min": 0, "max": 1,
     "cyclic": True},
]  # fmt: skip
DEFAULTS = {"rate": 1.0, "seed": 3, "on": False, "label": "a", "shape": "sine", "phase": 0.0, "span": 0.0}
DEPTH = {"name": "depth", "kind": "float", "default": 0.5, "doc": "How far.", "unit": "fraction", "min": 0, "max": 1}
SHAPE = {"name": "shape", "kind": "choice", "choices": ["sine", "saw"], "default": "sine", "doc": "'sine' or 'saw'."}
PHASE = {"name": "phase", "kind": "float", "default": 0.0, "doc": "Where.", "unit": "cycles", "min": 0, "max": 1}
LONE = "\ud800"  # a lone surrogate: a str that cannot be written as UTF-8


def problems_of(refused):
    """The (path, rule) pairs a `ParameterError` lists."""
    return [(path, rule) for path, rule in refused.value.problems]


def assert_lists(refused, expected):
    """`refused` lists exactly the paths of `expected`, in order, each rule holding its short part."""
    problems = problems_of(refused)
    assert [path for path, _ in problems] == [path for path, _ in expected], problems
    for (path, rule), (_, part) in zip(problems, expected, strict=True):
        assert part in rule, (path, rule, part)


@pytest.mark.parametrize("name", BUILT_INS)
def test_the_core_has_a_table_for_every_registered_built_in(name):
    assert name in registry.all_registered()
    thing = CORE.get(name)
    assert list(thing.describe_params()) == list(thing.description()["parameters"]["properties"])


def test_a_built_ins_table_describes_each_field_with_its_range_basic_fields_first():
    described = CORE.get("slew").describe_params()
    assert described == SLEW_DESCRIBED
    assert list(described) == ["rise", "fall", "rate"]


def test_a_table_declared_as_a_built_in_describes_it_the_same():
    assert _core.ParamTable(SLEW_SPEC, name="Slew").describe() == CORE.get("slew").describe_params()


def test_a_table_lists_its_basic_fields_before_its_advanced_ones_each_in_declaration_order():
    table = _core.ParamTable(EVERY_KIND, name="Every")
    order = ["rate", "seed", "on", "label", "shape", "phase", "span"]
    assert list(table.describe()) == order
    assert list(table.parse({})) == order


def test_a_table_describes_a_field_of_each_kind_as_a_json_schema_property():
    described = _core.ParamTable(EVERY_KIND, name="Every").describe()
    assert described["seed"] == {
        "type": "integer", "default": 3, "description": "Which noise.", "title": "Seed", "unit": "count",
        "minimum": 0, "maximum": 4294967295, "advanced": False, "cyclic": False, "range": "0 to 4294967295",
    }  # fmt: skip
    assert described["shape"] == {
        "type": "string", "enum": ["sine", "saw"], "default": "sine", "description": "'sine' or 'saw'.",
        "title": "Shape", "unit": None, "advanced": False, "cyclic": False,
    }  # fmt: skip
    assert described["on"]["type"] == "boolean"
    assert described["phase"]["cyclic"] is True
    assert "range" not in described["label"]


@pytest.mark.parametrize(
    ("values", "expected"),
    [
        pytest.param({}, {}, id="nothing given: every default"),
        pytest.param(
            {"rate": 2.5, "seed": 7, "on": True, "label": "b", "shape": "saw", "phase": 0.5, "span": -3.0},
            {"rate": 2.5, "seed": 7, "on": True, "label": "b", "shape": "saw", "phase": 0.5, "span": -3.0},
            id="every field given",
        ),
        pytest.param({"rate": 4}, {"rate": 4.0}, id="an integer for a decimal is that decimal"),
        pytest.param({"rate": 10, "seed": 4294967295}, {"rate": 10.0, "seed": 4294967295}, id="the top ends"),
        pytest.param({"rate": 0.0, "seed": 0}, {"rate": 0.0, "seed": 0}, id="the bottom ends"),
        pytest.param({"phase": 1.25}, {"phase": 0.25}, id="a phase past its range wraps into it"),
        pytest.param({"phase": -0.25}, {"phase": 0.75}, id="a phase below its range wraps into it"),
        pytest.param({"phase": 1.0}, {"phase": 1.0}, id="a phase at the top end is kept"),
        pytest.param({"span": 2**63 - 1}, {"span": float(2**63 - 1)}, id="the largest 64-bit integer for a decimal"),
        pytest.param(
            {"rate": np.float64(2.5), "seed": np.int64(9), "on": np.bool_(True)},
            {"rate": 2.5, "seed": 9, "on": True},
            id="numpy scalars are their values",
        ),
        pytest.param({"rate": True}, [("rate", "must be a number")], id="true is not a number"),
        pytest.param({"rate": "1"}, [("rate", "must be a number")], id="a numeric string is not a number"),
        pytest.param({"rate": None}, [("rate", "must be a number")], id="None is not a number"),
        pytest.param({"seed": 1.0}, [("seed", "whole number")], id="1.0 is not a whole number"),
        pytest.param({"on": 1}, [("on", "true or false")], id="1 is not a switch"),
        pytest.param({"rate": math.nan}, [("rate", "finite")], id="NaN is refused"),
        pytest.param({"phase": math.inf}, [("phase", "finite")], id="an infinity is refused, even for a phase"),
        pytest.param({"span": -math.inf}, [("span", "finite")], id="minus infinity is refused"),
        pytest.param({"span": 2**63}, [("span", "64 bits")], id="an integer beyond 64 bits for a decimal"),
        pytest.param({"seed": 2**70}, [("seed", "64 bits")], id="an integer far beyond 64 bits"),
        pytest.param({"speed": 2.0}, [("speed", "unknown field")], id="an unknown field"),
        pytest.param({"speed": math.nan}, [("speed", "unknown field")], id="an unknown field holding NaN"),
        pytest.param({"shape": "tri"}, [("shape", "one of")], id="a name that is not a choice"),
        pytest.param({"rate": 10.5}, [("rate", "<= 10")], id="above the range"),
        pytest.param({"seed": -1}, [("seed", ">= 0")], id="below the range"),
        pytest.param({"label": LONE}, [("label", "UTF-8")], id="text with a lone surrogate"),
        pytest.param({"rate": {1, 2}}, [("rate", "parameter value")], id="a set is no parameter value"),
        pytest.param({"rate": object()}, [("rate", "parameter value")], id="an object is no parameter value"),
        pytest.param(
            {"rate": math.nan, "seed": 1.0, "on": 1, "label": LONE, "shape": "tri", "phase": True, "speed": 1},
            [
                ("rate", "finite"),
                ("seed", "whole number"),
                ("on", "true or false"),
                ("label", "UTF-8"),
                ("shape", "one of"),
                ("phase", "must be a number"),
                ("speed", "unknown field"),
            ],
            id="every broken rule is listed, in the table's order, unknown fields last",
        ),
        pytest.param([("rate", 1.0)], [("", "mapping")], id="pairs are not a mapping"),
        pytest.param(None, [("", "mapping")], id="None is not a mapping"),
        pytest.param({1: 2.0}, [("", "must be text")], id="a key that is not text"),
        pytest.param({LONE: 2.0}, [("", "UTF-8")], id="a key with a lone surrogate"),
    ],
)
def test_values_are_read_strictly_and_every_broken_rule_is_listed(values, expected):
    table = _core.ParamTable(EVERY_KIND, name="Every")
    if isinstance(expected, dict):
        assert table.parse(values) == {**DEFAULTS, **expected}
    else:
        with pytest.raises(ParameterError) as refused:
            table.parse(values)
        assert_lists(refused, expected)
        assert str(refused.value).startswith("Every: ")


def test_values_come_back_as_plain_python_values_of_each_fields_kind():
    parsed = _core.ParamTable(EVERY_KIND, name="Every").parse({"rate": 4, "seed": np.int64(9), "on": np.bool_(True)})
    kinds = {name: type(value) for name, value in parsed.items()}
    assert kinds == {
        "rate": float, "seed": int, "on": bool, "label": str, "shape": str, "phase": float, "span": float,
    }  # fmt: skip


def test_a_refusal_of_many_values_lists_twenty_in_its_message_and_counts_the_rest():
    with pytest.raises(ParameterError, match=r"; \.\.\. and 5 more$") as refused:
        _core.ParamTable(EVERY_KIND, name="Every").parse({f"extra{index:02}": 1 for index in range(25)})
    assert len(refused.value.problems) == 25


@pytest.mark.parametrize(
    ("spec", "expected"),
    [
        pytest.param([DEPTH], [], id="a decimal with unit, range, default and doc"),
        pytest.param(EVERY_KIND, [], id="a field of every kind"),
        pytest.param([], [], id="no fields at all"),
        pytest.param([{**SHAPE, "choices": ["sine"]}], [], id="a choice of one"),
        pytest.param([{**DEPTH, "doc": ""}], [("depth", "needs a description")], id="an empty doc"),
        pytest.param([{k: v for k, v in DEPTH.items() if k != "doc"}], [("depth", "description")], id="no doc"),
        pytest.param([{**DEPTH, "unit": None}], [("depth", "needs a unit")], id="a number without a unit"),
        pytest.param(
            [{**DEPTH, "default": 2.0}], [("depth", "must lie in the range")], id="a range without the default"
        ),
        pytest.param([{**DEPTH, "name": "strength"}], [("strength", "do not name")], id="the name strength"),
        pytest.param([{**DEPTH, "name": "seconds"}], [("seconds", "after its unit")], id="a name that is a unit"),
        pytest.param([{**PHASE, "max": 2, "cyclic": True}], [("phase", "one cycle")], id="cyclic, two cycles wide"),
        pytest.param([DEPTH, {**DEPTH, "default": 0.25}], [("depth", "more than once")], id="a name twice"),
        pytest.param(
            [{**SHAPE, "choices": ["sine", "saw", "sine"]}],
            [("shape", "offered more than once")],
            id="a choice offered twice",
        ),
        pytest.param(
            [{**SHAPE, "choices": []}],
            [("shape", "at least one name"), ("shape", "default is not valid")],
            id="a choice of nothing",
        ),
        pytest.param(
            [{k: v for k, v in DEPTH.items() if k != "default"}], [("depth", "needs a default")], id="no default"
        ),
        pytest.param([{**DEPTH, "name": ""}], [("", "needs a name")], id="an empty name"),
        pytest.param([{**DEPTH, "unit": "percent"}], [("depth", "unknown unit")], id="an unknown unit"),
        pytest.param([{**DEPTH, "unit": 3}], [("depth", "unit must be")], id="a unit that is not text"),
        pytest.param([{**DEPTH, "kind": "list"}], [("depth", "kind must be one of")], id="an unknown kind"),
        pytest.param([{**DEPTH, "default": math.nan}], [("depth", "finite")], id="a NaN default"),
        pytest.param([{**DEPTH, "default": {0.5}}], [("depth", "parameter value")], id="a set as the default"),
        pytest.param([{**DEPTH, "min": "0"}], [("depth", "min must be a number")], id="a min that is text"),
        pytest.param([{**DEPTH, "max": True}], [("depth", "max must be a number")], id="a max that is a bool"),
        pytest.param([{**DEPTH, "advanced": 1}], [("depth", "true or false")], id="advanced that is not a bool"),
        pytest.param([{**DEPTH, "step": 0.1}], [("depth", "unknown key")], id="an unknown key"),
        pytest.param([{**DEPTH, "choices": ["a"]}], [("depth", "only for a choice")], id="choices on a number"),
        pytest.param([{**SHAPE, "choices": "sine"}], [("shape", "list of names")], id="choices that are text"),
        pytest.param([{**SHAPE, "choices": ["sine", 2]}], [("shape", "list of names")], id="a choice that is a number"),
        pytest.param([{k: v for k, v in DEPTH.items() if k != "name"}], [("[0]", "needs a name")], id="no name"),
        pytest.param([DEPTH, {**DEPTH, "name": 7}], [("[1]", "name must be text")], id="a name that is not text"),
        pytest.param([{**DEPTH, "name": LONE}], [("[0]", "UTF-8")], id="a name with a lone surrogate"),
        pytest.param([["depth", 0.5]], [("[0]", "mapping")], id="a declaration that is not a mapping"),
        pytest.param(DEPTH, [("", "list")], id="one mapping instead of a list of them"),
        pytest.param(
            [{**DEPTH, "doc": " ", "unit": None}, {**DEPTH, "name": "k"}],
            [("depth", "needs a description"), ("depth", "needs a unit"), ("k", "do not name")],
            id="every rule each field breaks is listed",
        ),
        pytest.param(
            [{**DEPTH, "unit": "percent"}, {**DEPTH, "name": "strength", "min": "0"}],
            [("depth", "unknown unit"), ("strength", "min must be a number")],
            id="a declaration that cannot be read is refused before any rule is checked",
        ),
    ],
)
def test_a_declaration_is_accepted_only_when_it_follows_every_rule(spec, expected):
    if not expected:
        assert sorted(_core.ParamTable(spec, name="Mine").describe()) == sorted(field["name"] for field in spec)
        return
    with pytest.raises(ParameterError) as refused:
        _core.ParamTable(spec, name="Mine")
    assert_lists(refused, expected)
    assert str(refused.value).startswith("Mine: ")


def test_a_built_ins_values_are_read_through_its_table():
    lfo = CORE.get("lfo")
    times = [0.0, 0.3, 0.7]
    # a phase one cycle further is the same position
    assert (
        lfo(phase=1.25, shape="saw-down").sample(times).tolist()
        == lfo(phase=0.25, shape="saw-down").sample(times).tolist()
    )
    with pytest.raises(ParameterError, match=r"lfo: shape: ") as refused:
        lfo(rate=-1.0, shape="tri")
    assert [path for path, _ in refused.value.problems] == ["shape", "rate"]


def test_a_built_ins_rule_between_fields_is_checked():
    clip = CORE.get("clip")
    assert clip(_core.Constant(1.0), low=0.5, high=0.5).sample(0.0).tolist() == 0.5
    with pytest.raises(ParameterError, match="high"):
        clip(_core.Constant(1.0), low=0.5, high=0.25)


def test_the_core_refuses_an_ease_the_package_does_not_name():
    with pytest.raises(ParameterError, match="ease"):
        CORE.get("remap")(_core.Constant(0.0), ease="wobble")


# ---- nesting ----

DEEPEST = 128  # the most lists or mappings a value read from Python may be nested in (one more is refused)
RATE_SPEC = [{"name": "rate", "kind": "float", "default": 1.0, "doc": "d", "unit": "hertz", "min": 0, "max": 10}]


def nested(depth):
    """`depth` lists, each inside the next: `nested(2)` is `[[]]`."""
    value: list[object] = []
    for _ in range(depth - 1):
        value = [value]
    return value


@pytest.mark.parametrize(
    ("read", "depth", "refused", "part"),
    [
        pytest.param(
            lambda value: _core.ParamTable(RATE_SPEC, name="X").parse({"rate": value}),
            DEEPEST, ParameterError, "must be a number", id="a value at the limit reaches the table's rule",
        ),
        pytest.param(
            lambda value: _core.ParamTable(RATE_SPEC, name="X").parse({"rate": value}),
            DEEPEST + 1, ParameterError, "nested too deeply (limit 128)", id="a value past the limit is refused",
        ),
        pytest.param(
            lambda value: _core.ParamTable([{**RATE_SPEC[0], "default": value}], name="X"),
            DEEPEST, ParameterError, "the default is not valid: must be a number", id="a default at the limit",
        ),
        pytest.param(
            lambda value: _core.ParamTable([{**RATE_SPEC[0], "default": value}], name="X"),
            DEEPEST + 1, ParameterError, "the default is not valid: nested too deeply", id="a default past the limit",
        ),
        pytest.param(
            lambda value: registry.get("slew")(_core.Constant(0.0), rise=value),
            DEEPEST, ParameterError, "must be a number", id="a shaper's value at the limit",
        ),
        pytest.param(
            lambda value: registry.get("slew")(_core.Constant(0.0), rise=value),
            DEEPEST + 1, ParameterError, "nested too deeply", id="a shaper's value past the limit",
        ),
        pytest.param(
            lambda value: Series.parse({"rate": 1.0, "unit": "ratio", "values": value}),
            DEEPEST + 1, FormatError, "values[0]: invalid type", id="a format reads no deeper than its fields",
        ),
    ],
)  # fmt: skip
def test_a_value_is_read_through_at_most_128_levels_of_nesting(read, depth, refused, part):
    with pytest.raises(refused) as error:
        read(nested(depth))
    assert part in str(error.value)


# each in a fresh interpreter: a reader that recursed without a limit would crash it, not fail a test
NESTING = """
import numpy

from bullterrier_control import Series, registry
from bullterrier_control._native import _core
from bullterrier_control.errors import BullterrierControlError

cyclic = []
cyclic.append(cyclic)
deep = []
for _ in range(1_000_000):
    deep = [deep]
held = numpy.empty((), dtype=object)  # an array whose tolist() is itself
held[()] = held
table = _core.ParamTable([{"name": "rate", "kind": "float", "default": 1.0, "doc": "d", "unit": "hertz", "min": 0,
                          "max": 10}], name="X")
try:
    READ
except (BullterrierControlError, ValueError) as error:
    print(type(error).__name__, error)
"""


@pytest.mark.parametrize(
    ("read", "said"),
    [
        pytest.param("table.parse({'rate': cyclic})", "ParameterError X: rate: nested too deeply", id="table, a cycle"),
        pytest.param("table.parse({'rate': deep})", "ParameterError X: rate: nested too deeply", id="table, 1M deep"),
        pytest.param("table.parse({'rate': held})", "ParameterError X: rate: nested too deeply",
                     id="table, an array that holds itself"),
        pytest.param("_core.ParamTable([{'name': 'rate', 'kind': 'text', 'default': cyclic, 'doc': 'd'}], name='X')",
                     "ParameterError X: rate: the default is not valid: nested too deeply", id="a default, a cycle"),
        pytest.param("registry.get('slew')(_core.Constant(0.0), rise=deep)",
                     "ParameterError shaper 'slew': slew: rise: nested too deeply", id="a shaper, 1M deep"),
        pytest.param("registry.get('slew')(_core.Constant(0.0), rise=cyclic)",
                     "ParameterError shaper 'slew': slew: rise: nested too deeply", id="a shaper, a cycle"),
        pytest.param("Series.parse({'rate': 1.0, 'unit': 'ratio', 'values': cyclic})", "FormatError Series: values[0]",
                     id="a format, a cycle"),
        pytest.param("Series.parse({'rate': 1.0, 'unit': 'ratio', 'values': deep})", "FormatError Series: values[0]",
                     id="a format, 1M deep"),
        pytest.param("_core.Event.parse(held)", "FormatError Event: nested too deeply",
                     id="a format, an array that holds itself"),
    ],
)  # fmt: skip
def test_a_cyclic_or_a_million_deep_value_is_refused_without_crashing(read, said):
    code = NESTING.replace("READ", read)
    done = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True, encoding="utf-8", check=False)
    assert done.returncode == 0, done.stderr
    assert done.stdout.startswith(said), done.stdout
