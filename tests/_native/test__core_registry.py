"""The extension's registry (`Registry`, `Thing`): the core's table of sources, shapers and operations, a Python
function registered in it as a thing whose body is Python, and a call of a thing as the package calls it.

An architecture test of the extension's own interface. The calling rules and their messages are the package's
(`registry.Registered.__call__` before the core held them); every expected number and text is worked out by hand.
"""

import inspect
import re
import subprocess
import sys
import threading
import types
from collections.abc import Iterator
from concurrent.futures import ThreadPoolExecutor
from typing import Any

import numpy as np
import pytest

from bullterrier_control import Event, Series
from bullterrier_control._native import _core  # noqa: PLC2701  architecture: the boundary the registry is reached through
from bullterrier_control.errors import ParameterError, SignalError, UnknownNameError
from bullterrier_control.plugin import Constant, Finite, Params, Signal, param

TIMEOUT = 60.0  # seconds a thread test waits before it fails as a deadlock


class Level(Params):
    """A level."""

    value: Finite = param(0.5, doc="The level.", unit="fraction", min=0, max=1)


class Nothing(Params):
    """No parameters."""


class Evented(Params):
    """A parameter named like an input."""

    events: Finite = param(0.5, doc="Clashes with the input.", unit="fraction", min=0, max=1)


POINTS = [{"t": 0.0, "value": 1.0}, {"t": 2.0, "value": 3.0, "ease": "in"}]


def register(registry, function, kind="source", params=Level, inputs=None, returns="signal"):
    """`function` registered in `registry` as a thing of `kind`."""
    return registry.register(function, kind=kind, params=params, inputs=inputs or {}, returns=returns)


@pytest.fixture
def mine():
    """A registry of its own, empty."""
    return _core.Registry(builtin=False)


def level(params: Level) -> Signal:
    """A level that holds.

    More words."""
    return Constant(params.value)


def scale(signal: Signal, params: Level) -> Signal:
    """The signal times the value."""
    return signal * params.value


# ---- the registry: what it holds, finding, removing, swapping ----


def test_the_registry_of_the_core_holds_what_the_core_runs_and_an_empty_one_nothing():
    assert list(_core.Registry(builtin=True).things()) == [
        "beats",
        "constant",
        "delay",
        "every",
        "follow",
        "hold",
        "latch",
        "select",
        "shift",
        "slew",
        "thin",
    ]
    assert _core.Registry(builtin=False).things() == {}


def test_an_unknown_name_is_refused_listing_what_is_registered(mine):
    register(mine, level)
    with pytest.raises(UnknownNameError, match="unknown source, shaper or operation 'lvl'; available: level"):
        mine.get("lvl")
    with pytest.raises(UnknownNameError, match="'gone'"):
        mine.unregister("gone")


def test_a_thing_is_found_as_registered_and_gone_once_unregistered(mine):
    registered = register(mine, level)
    register(mine, scale, kind="shaper")
    assert mine.get("level") is registered
    assert list(mine.things()) == ["level", "scale"]
    mine.unregister("level")
    assert list(mine.things()) == ["scale"]
    with pytest.raises(UnknownNameError):
        mine.get("level")


def test_replace_takes_the_things_of_another_and_gives_back_its_own(mine):
    register(mine, level)
    saved = mine.replace(_core.Registry(builtin=False))
    assert mine.things() == {}
    register(mine, scale, kind="shaper")
    mine.replace(saved)
    assert list(mine.things()) == ["level"]
    assert list(saved.things()) == ["level"]  # the copy given back is a registry of its own
    saved.unregister("level")
    assert list(mine.things()) == ["level"]


# ---- registering: the rules (the core's), identity and reloads ----


def make(value):
    """A source made by a factory: every one has the qualname `make.<locals>.made`."""

    def made(params: Nothing) -> Signal:
        return Constant(value)

    return made


def test_two_functions_made_by_one_factory_do_not_replace_each_other(mine):
    first = make(1.0)
    register(mine, first, params=Nothing)
    with pytest.raises(ValueError, match=r"source 'made': the name is taken by the source defined as .*made"):
        register(mine, make(2.0), params=Nothing)
    register(mine, first, params=Nothing)  # the very same function again is a reload
    assert mine.get("made")().sample(0.0).tolist() == 1.0


def test_the_taken_name_error_shows_the_function_without_its_address(mine):
    register(mine, make(1.0), params=Nothing)
    shown = re.escape(f"{__name__}.make.<locals>.made")
    with pytest.raises(ValueError, match=rf"the name is taken by the source defined as {shown}$"):
        register(mine, make(2.0), params=Nothing)


def test_a_module_defined_again_replaces_what_it_registered_as_reloading_does(mine):
    source = "def level(params):\n    return CONSTANT(0.25)\n"
    functions = []
    for value in (0.25, 0.75):
        module = types.ModuleType("plugins_of_mine")
        vars(module)["CONSTANT"] = Constant
        exec(source.replace("0.25", str(value)), module.__dict__)  # noqa: S102  a module's text, written here
        functions.append(module.level)
    register(mine, functions[0])
    register(mine, functions[1])
    assert functions[0] is not functions[1]
    assert mine.get("level")().sample(0.0).tolist() == 0.75


def test_a_thing_of_another_kind_cannot_take_a_name(mine):
    register(mine, scale, kind="shaper")
    with pytest.raises(ValueError, match="source 'scale': the name is taken by the shaper defined as"):
        register(mine, scale, kind="source")


def fine(params: Nothing) -> Signal:
    return Constant(0.0)


def Loud(params: Nothing) -> Signal:  # a name the registry refuses
    return Constant(0.0)


def events_named(params: Level) -> tuple[Event, ...]:
    return ()


@pytest.mark.parametrize(
    ("function", "declared", "named"),
    [
        pytest.param(Loud, {}, "source 'Loud': a name is lower case", id="a name in capitals"),
        pytest.param(
            fine,
            {"params": Evented},
            "the parameter 'events' has the name of an input",
            id="a parameter named like an input",
        ),
        pytest.param(fine, {"returns": "events"}, r"a source returns a signal \(not events\)", id="a source of events"),
        pytest.param(events_named, {"kind": "operation"}, "an operation returns events or a series", id="no data"),
        pytest.param(fine, {"kind": "sink"}, "kind must be source, shaper or operation", id="an unknown kind"),
        pytest.param(fine, {"returns": "numbers"}, "returns must be signal, events or series", id="unknown returns"),
        pytest.param(fine, {"inputs": {"notes": True}}, "unknown input 'notes'", id="an unknown input"),
    ],
)
def test_a_registration_that_breaks_a_rule_is_refused_naming_it_and_registers_nothing(mine, function, declared, named):
    with pytest.raises(ValueError, match=named):
        register(mine, function, **{"params": Nothing, **declared})
    assert mine.things() == {}


@pytest.mark.parametrize("params", [dict, Level(), None], ids=["another class", "an instance", "none"])
def test_parameters_that_are_not_a_params_class_are_a_type_error(mine, params):
    with pytest.raises(TypeError, match="params must be a Params subclass"):
        register(mine, fine, params=params)
    assert mine.things() == {}


# ---- what a thing says about itself ----


def test_a_python_thing_says_what_it_is_takes_and_gives():
    def track(params: Level, *, points: object, grid: object = None) -> Signal:
        """Follows the points.

        Slowly."""
        return Constant(0.0)

    thing = register(_core.Registry(builtin=False), track, inputs={"points": True, "grid": False})
    assert (thing.name, thing.kind, thing.returns) == ("track", "source", "signal")
    assert (thing.summary, thing.function, thing.params_model) == ("Follows the points.", track, Level)
    assert thing.inputs == {"points": True, "grid": False}
    assert list(thing.inputs) == ["points", "grid"]
    with pytest.raises(TypeError):
        thing.inputs["points"] = False  # read-only
    assert thing.__doc__.startswith("source track(*, points, grid=None, **values) -> signal\n\nFollows the points.\n")
    assert "\n  value (fraction, 0 to 1), default 0.5\n      The level." in thing.__doc__
    described = thing.description()
    assert (described["kind"], described["returns"], described["summary"]) == (
        "source",
        "signal",
        "Follows the points.",
    )
    assert described["inputs"]["grid"]["required"] is False
    assert described["parameters"]["title"] == "Level"
    assert described["parameters"]["description"] == "A level."


def test_a_signature_names_the_signal_the_inputs_and_every_parameter_with_its_default():
    def chase(signal: Signal, params: Level, *, points: object) -> Signal:
        return signal

    shaper = register(_core.Registry(builtin=False), chase, kind="shaper", inputs={"points": True})
    assert str(inspect.signature(shaper)) == "(signal, /, *, points, value=0.5)"
    slew = _core.Registry(builtin=True).get("slew")
    assert str(inspect.signature(slew)) == "(signal, /, *, rise=0.1, fall=0.1, rate=1000.0)"


def test_the_description_lists_parameters_as_a_reader_sees_them_basic_first():
    slew = _core.Registry(builtin=True).get("slew")
    assert list(slew.description()["parameters"]["properties"]) == ["rise", "fall", "rate"]
    assert (slew.function, slew.params_model) == (None, None)


def test_the_description_lists_the_inputs_in_the_order_the_function_declares_them(mine):
    def late(params: Nothing, *, values: object, grid: object = None) -> Signal:
        return Constant(0.0)

    register(mine, late, params=Nothing, inputs={"values": True, "grid": False})
    assert list(mine.get("late").description()["inputs"]) == ["values", "grid"]


# ---- calling: the rules, good and bad rows ----


def track(params: Level, *, points: tuple[_core.Keyframe, ...]) -> Signal:
    """The last point's value times the level."""
    return Constant(points[-1].value * params.value)


def maybe(params: Nothing, *, points: tuple[_core.Keyframe, ...] | None = None) -> Signal:
    """1 without points, 2 with."""
    return Constant(1.0 if points is None else 2.0)


def pair(params: Nothing, *, points: tuple[_core.Keyframe, ...], values: tuple[float, ...]) -> Signal:
    """The sum of the values."""
    return Constant(sum(values))


@pytest.fixture
def callable_things():
    """One registry holding a source of each shape of call and a shaper."""
    registry = _core.Registry(builtin=False)
    register(registry, level)
    register(registry, track, inputs={"points": True})
    register(registry, maybe, params=Nothing, inputs={"points": False})
    register(registry, pair, params=Nothing, inputs={"points": True, "values": True})
    register(registry, scale, kind="shaper")
    return registry


ONE = Constant(1.0)
FOUR = Constant(4.0)

GOOD_CALLS = [
    pytest.param("level", (), {}, 0.5, id="a source with every value left at its default"),
    pytest.param("level", (), {"value": 0.25}, 0.25, id="a value given"),
    pytest.param("level", (), {"value": 1}, 1.0, id="a whole number for a decimal at the top of its range"),
    pytest.param("track", (), {"points": POINTS, "value": 0.5}, 1.5, id="a required input beside a value"),
    pytest.param("maybe", (), {}, 1.0, id="an optional input left out"),
    pytest.param("maybe", (), {"points": None}, 1.0, id="None for an optional input is leaving it out"),
    pytest.param("maybe", (), {"points": POINTS}, 2.0, id="an optional input given"),
    pytest.param("pair", (), {"points": POINTS, "values": [0.5, 2]}, 2.5, id="two inputs"),
    pytest.param("scale", (FOUR,), {"value": 0.25}, 1.0, id="a shaper with its signal"),
]


@pytest.mark.parametrize(("name", "signals", "values", "expected"), GOOD_CALLS)
def test_a_call_that_keeps_the_rules_gives_the_signal_its_body_makes(callable_things, name, signals, values, expected):
    assert callable_things.get(name)(*signals, **values).sample(3.0).tolist() == expected


BAD_CALLS = [
    pytest.param("level", (ONE,), {}, TypeError, "source 'level' takes 0 signal(s), not 1", id="a signal to a source"),
    pytest.param("scale", (), {}, TypeError, "shaper 'scale' takes 1 signal(s), not 0", id="a shaper without one"),
    pytest.param("scale", (ONE, FOUR), {}, TypeError, "shaper 'scale' takes 1 signal(s), not 2", id="two signals"),
    pytest.param("scale", (1.0,), {}, TypeError, "shaper 'scale' takes 1 signal(s), not 1", id="a number not a signal"),
    pytest.param(
        "level", (), {"points": POINTS}, TypeError, "source 'level' takes no input 'points'", id="an undeclared input"
    ),
    pytest.param(
        "level", (), {"points": None}, TypeError, "source 'level' takes no input 'points'", id="an undeclared None"
    ),
    pytest.param("track", (), {}, TypeError, "source 'track' needs the input 'points'", id="a required input missing"),
    pytest.param(
        "track",
        (),
        {"points": None},
        ParameterError,
        "source 'track': points: ",
        id="None for a required input is checked as its data",
    ),
    pytest.param(
        "track",
        (),
        {"points": [{"t": 2, "value": 1}, {"t": 1, "value": 1}]},
        ParameterError,
        "source 'track': points[1].t: ",
        id="an input that breaks its rules, under its path",
    ),
    pytest.param(
        "level", (), {"value": 2}, ParameterError, "source 'level': Level: value: must be <= 1.0 (got 2)", id="range"
    ),
    pytest.param("level", (), {"speed": 1}, ParameterError, "speed: unknown field", id="an unknown parameter"),
    pytest.param("level", (), {"value": True}, ParameterError, "value: ", id="a bool for a number"),
    pytest.param(
        "level", (), {"value": float("nan")}, ParameterError, "value: must be a finite number (got NaN)", id="NaN"
    ),
    pytest.param("level", (), {"value": {1}}, ParameterError, "value: ", id="a value JSON cannot hold"),
    pytest.param(
        "track", (), {"points": POINTS, "speed": 1}, ParameterError, "speed: unknown field", id="unknown beside input"
    ),
    # the order of the checks: the signals, an undeclared input, each input in its order, then the values
    pytest.param("level", (ONE,), {"points": [], "value": 9}, TypeError, "signal", id="the signals first"),
    pytest.param(
        "track", (), {"values": [1], "points": "x"}, TypeError, "takes no input 'values'", id="undeclared before data"
    ),
    pytest.param(
        "pair", (), {"values": "x"}, TypeError, "needs the input 'points'", id="a missing input before a later one"
    ),
    pytest.param("pair", (), {"points": "x"}, ParameterError, "points: ", id="a broken input before a missing one"),
    pytest.param("track", (), {"points": "x", "value": 9}, ParameterError, "points: ", id="inputs before the values"),
]


@pytest.mark.parametrize(("name", "signals", "values", "error", "message"), BAD_CALLS)
def test_a_call_that_breaks_a_rule_is_refused_by_kind_naming_the_thing(
    callable_things, name, signals, values, error, message
):
    with pytest.raises(error) as refused:
        callable_things.get(name)(*signals, **values)
    assert type(refused.value) is error
    assert message in str(refused.value)


def test_a_refused_parameter_lists_each_problem_by_its_field(callable_things):
    with pytest.raises(ParameterError) as refused:
        callable_things.get("level")(value=float("inf"), speed=1)
    assert [path for path, _ in refused.value.problems] == ["value", "speed"]


def test_an_input_arrives_checked_as_the_package_objects_and_the_values_as_its_params():
    received: dict[str, Any] = {}

    def listen(params: Level, *, points: object, events: object, grid: object, values: object, series: object):
        received.update(params=params, points=points, events=events, grid=grid, values=values, series=series)
        return Constant(0.0)

    inputs = dict.fromkeys(("points", "events", "grid", "values", "series"), True)
    thing = register(_core.Registry(builtin=False), listen, inputs=inputs)
    grid = {"beats": [0.0, 0.5], "meter": 4, "first_downbeat": 0}
    series = {"rate": 10, "values": [0.0, 1.0], "unit": "ratio"}
    events = [{"t": 2.0, "strength": 0.5}, {"t": 1.0}]
    thing(points=POINTS, events=events, grid=grid, values=[1, 2.5], series=series, value=0.75)
    assert type(received["params"]) is Level
    assert received["params"].value == 0.75
    assert received["points"] == (_core.Keyframe(t=0, value=1), _core.Keyframe(t=2, value=3, ease="in"))
    assert [event.t for event in received["events"]] == [1.0, 2.0]  # in time order
    assert all(type(event) is Event for event in received["events"])
    assert received["grid"].beats == (0.0, 0.5)
    assert received["values"] == (1.0, 2.5)
    assert type(received["series"]) is Series


def test_a_call_of_a_thing_of_the_core_is_checked_and_named_as_a_python_one_is():
    slew = _core.Registry(builtin=True).get("slew")
    step = _core.from_values([0.0, 1.0], 10.0, interpolation="hold")
    # 0.5 s to climb by 1 at 10 samples a second: 0.2 a sample
    assert slew(step, rise=0.5, rate=10.0).sample([0.0, 0.1, 0.2]).tolist() == pytest.approx([0.0, 0.2, 0.4])
    with pytest.raises(ParameterError, match=r"^shaper 'slew': slew: rise: must be >= 0.0 \(got -1\)$"):
        slew(step, rise=-1)
    with pytest.raises(TypeError, match="shaper 'slew' takes 1 signal"):
        slew()


def test_an_operation_gives_its_events_or_its_series():
    def louder(params: Level, *, events: tuple[Event, ...]) -> tuple[Event, ...]:
        return tuple(event for event in events if event.strength >= params.value)

    def ramp(params: Nothing) -> Series:
        return Series(rate=2, values=[0.0, 0.5, 1.0], unit="ratio")

    registry = _core.Registry(builtin=False)
    register(registry, louder, kind="operation", inputs={"events": True}, returns="events")
    register(registry, ramp, kind="operation", params=Nothing, returns="series")
    kept = registry.get("louder")(events=[{"t": 3.0, "strength": 0.9}, {"t": 1.0, "strength": 0.2}], value=0.5)
    assert type(kept) is tuple
    assert [(event.t, event.strength) for event in kept] == [(3.0, 0.9)]
    assert registry.get("ramp")().values.tolist() == [0.0, 0.5, 1.0]


# ---- hostile bodies ----


class Broken(Exception):
    """What a plugin raised."""


def nothing_yielded() -> Iterator[Event]:
    """A generator of no events."""
    yield from ()


def hostile(raised: Exception | None = None, gives: object = None) -> object:
    """A source whose body raises `raised`, or gives `gives`."""

    def body(params: Nothing) -> object:
        if raised is not None:
            raise raised
        return gives() if callable(gives) else gives

    body.__name__ = body.__qualname__ = "body"
    return body


def test_an_exception_a_body_raises_reaches_the_caller_unchanged(mine):
    raised = Broken("the plugin broke")
    register(mine, hostile(raised=raised), params=Nothing)
    with pytest.raises(Broken) as refused:
        mine.get("body")()
    assert refused.value is raised


def test_a_parameter_error_a_body_raises_is_its_own_not_the_things(mine):
    raised = ParameterError("depth: too deep", (("depth", "too deep"),))
    register(mine, hostile(raised=raised), params=Nothing)
    with pytest.raises(ParameterError) as refused:
        mine.get("body")()
    assert refused.value is raised


@pytest.mark.parametrize(
    ("kind", "returns", "gives", "named"),
    [
        pytest.param("source", "signal", 1.0, "source 'body' returned float, not a signal", id="a number"),
        pytest.param("source", "signal", None, "source 'body' returned NoneType, not a signal", id="nothing"),
        pytest.param("source", "signal", lambda: (Constant(x) for x in [1.0]), "returned generator", id="a generator"),
        pytest.param("operation", "events", [1.0], "operation 'body' returned list", id="a list of numbers"),
        pytest.param(
            "operation", "events", nothing_yielded, "returned generator, not a tuple of events", id="generator"
        ),
        pytest.param("operation", "series", (), "operation 'body' returned tuple, not a series", id="no series"),
    ],
)
def test_a_body_that_gives_the_wrong_kind_of_result_is_a_type_error_naming_the_thing(mine, kind, returns, gives, named):
    register(mine, hostile(gives=gives), kind=kind, params=Nothing, returns=returns)
    with pytest.raises(TypeError, match=named):
        mine.get("body")()


@pytest.mark.parametrize(
    ("times", "given"),
    [
        pytest.param(
            [1.0, 2.0, 2.0, 3.0], [(1.0, "0"), (2.0, "1"), (2.0, "2"), (3.0, "3")], id="in time order, two at once"
        ),
        pytest.param([], [], id="none"),
        pytest.param([3.0, 1.0], None, id="later first"),
        pytest.param([1.0, 2.0, 1.5], None, id="one out of order at the end"),
    ],
)
def test_events_a_body_gives_are_in_time_order_or_refused_naming_the_thing(mine, times, given):
    def body(params: Nothing) -> tuple[Event, ...]:
        return tuple(Event(t=t, label=str(index)) for index, t in enumerate(times))

    register(mine, body, kind="operation", params=Nothing, returns="events")
    if given is None:
        with pytest.raises(ValueError, match="operation 'body' returned events out of time order"):
            mine.get("body")()
    else:
        assert [(event.t, event.label) for event in mine.get("body")()] == given


def test_a_body_may_call_register_and_remove_things_of_its_own_registry(mine):
    register(mine, level)

    def host(params: Nothing) -> Signal:
        register(mine, scale, kind="shaper")
        made = mine.get("scale")(mine.get("level")(value=0.5), value=0.5)
        mine.unregister("scale")
        return made

    register(mine, host, params=Nothing)
    assert mine.get("host")().sample(1.0).tolist() == 0.25
    assert list(mine.things()) == ["host", "level"]


def test_a_body_that_calls_itself_without_end_is_refused_without_crashing_even_on_a_small_thread_stack():
    code = """
import threading
from bullterrier_control._native import _core
from bullterrier_control.errors import SignalError
from bullterrier_control.plugin import Params
class Nothing(Params):
    '''No parameters.'''
mine = _core.Registry(builtin=False)
def again(params):
    return mine.get("again")()
mine.register(again, kind="source", params=Nothing, inputs={}, returns="signal")
def call():
    try:
        mine.get("again")()
    except SignalError as error:
        print(error)
threading.stack_size(512 * 1024)
thread = threading.Thread(target=call)
thread.start()
thread.join()
"""
    done = subprocess.run(
        [sys.executable, "-c", code], capture_output=True, text=True, encoding="utf-8", check=False, timeout=TIMEOUT
    )
    assert done.returncode == 0, done.stderr
    assert "deep" in done.stdout


def test_a_shaper_that_gives_its_input_back_without_end_is_refused_as_too_deep(mine):
    def same(signal: Signal, params: Nothing) -> Signal:
        return signal

    register(mine, same, kind="shaper", params=Nothing)
    signal: Signal = ONE
    for _ in range(999):  # each call is a node: the first 999 fit the core's 1000 nodes
        signal = mine.get("same")(signal)
    with pytest.raises(SignalError, match="deep"):
        mine.get("same")(signal)


# ---- threads ----


def test_one_thread_registers_while_sixteen_others_query_and_call_without_deadlock(mine):
    register(mine, level)
    barrier = threading.Barrier(17)
    names = [f"level_{index}" for index in range(200)]

    def registering() -> None:
        barrier.wait(TIMEOUT)
        for index, name in enumerate(names):
            function = make(float(index))
            function.__name__ = function.__qualname__ = name
            register(mine, function, params=Nothing)

    def querying(index: int) -> tuple[list[float], list[tuple[str, float]]]:
        barrier.wait(TIMEOUT)
        levels, latest = [], []
        for _ in range(100):
            levels.append(mine.get("level")(value=index / 16).sample(0.0).tolist())
            present = [name for name in mine.things() if name.startswith("level_")]
            if present:
                latest.append((present[-1], mine.get(present[-1])().sample(0.0).tolist()))
        return levels, latest

    with ThreadPoolExecutor(max_workers=17) as pool:
        registered = pool.submit(registering)
        queried = [pool.submit(querying, index) for index in range(16)]
        registered.result(timeout=TIMEOUT)
        results = [future.result(timeout=TIMEOUT) for future in queried]
    assert sorted(mine.things()) == sorted(["level", *names])
    for index, (levels, latest) in enumerate(results):
        assert levels == [index / 16] * 100
        assert all(value == float(name.removeprefix("level_")) for name, value in latest)
    assert [mine.get(name)().sample(0.0).tolist() for name in names] == list(map(float, range(200)))


def test_sixteen_threads_calling_a_python_thing_at_once_get_their_own_values(mine):
    def wave(params: Level) -> Signal:
        class Wave(Signal):
            def compute(self, times):
                return np.sin(times) * params.value

        return Wave()

    register(mine, wave)
    barrier = threading.Barrier(16)

    def call(index: int) -> float:
        barrier.wait(TIMEOUT)
        return mine.get("wave")(value=index / 16).sample(1.0).tolist()

    with ThreadPoolExecutor(max_workers=16) as pool:
        results = [future.result(timeout=TIMEOUT) for future in [pool.submit(call, index) for index in range(16)]]
    assert results == pytest.approx([np.sin(1.0) * index / 16 for index in range(16)])


# ---- a built-in's table ----


def test_the_table_of_a_built_in_reads_its_values_and_names_it_in_a_refusal():
    table = _core.ParamTable.builtin("slew")
    assert table.parse({"rise": 0.5}) == {"rise": 0.5, "fall": 0.1, "rate": 1000.0}
    assert list(table.describe()) == ["rise", "fall", "rate"]
    with pytest.raises(ParameterError, match=r"^slew: rise: "):
        table.parse({"rise": -1})
    with pytest.raises(ValueError, match="wobble"):
        _core.ParamTable.builtin("wobble")
