import importlib.util
import threading
from concurrent.futures import ThreadPoolExecutor

import pytest

from bullterrier_control import BullterrierControlError, Event, ParameterError, Series, UnknownNameError, lib, registry
from bullterrier_control.params import Params, param
from bullterrier_control.plugin import Constant, Keyframe, Signal, operation, shaper, source
from bullterrier_control.validated import Finite


class Level(Params):
    """A level."""

    value: Finite = param(0.5, doc="The level, as a share of the range.", unit="fraction", min=0, max=1)


class Nothing(Params):
    """No parameters."""


@pytest.fixture(autouse=True)
def clean(throwaway_registry):
    """Every test here registers throwaway things."""


def test_a_registered_source_is_called_with_keyword_values_or_none_and_makes_a_signal():
    @source(params=Level)
    def level(params: Level) -> Signal:
        return Constant(params.value)

    assert level(value=0.25).sample([0.0, 3.0]).tolist() == [0.25, 0.25]
    assert level().sample(1.0).tolist() == 0.5


def test_a_registered_shaper_is_called_with_a_signal_and_keyword_values():
    @shaper(params=Level)
    def scale(signal: Signal, params: Level) -> Signal:
        return signal * params.value

    assert scale(Constant(4.0), value=0.25).sample(1.0).tolist() == 1.0
    assert scale(Constant(4.0)).sample(1.0).tolist() == 2.0


def test_the_registered_thing_knows_its_name_kind_and_parameters():
    @source(params=Level)
    def level(params: Level) -> Signal:
        """A constant level."""
        return Constant(params.value)

    assert (level.name, level.kind, level.params_model) == ("level", "source", Level)
    assert level.summary == "A constant level."
    assert [field.name for field in level.params_model.fields_doc()] == ["value"]


def test_a_call_with_the_wrong_number_of_signals_is_a_type_error():
    @source(params=Nothing)
    def silence(params: Nothing) -> Signal:
        return Constant(0.0)

    @shaper(params=Nothing)
    def same(signal: Signal, params: Nothing) -> Signal:
        return signal

    with pytest.raises(TypeError, match="silence"):
        silence(Constant(1.0))
    with pytest.raises(TypeError, match="same"):
        same()
    with pytest.raises(TypeError, match="same"):
        same(Constant(1.0), Constant(2.0))
    with pytest.raises(TypeError, match="same"):
        same(1.0)  # type: ignore[arg-type]  # a number is not a signal here: only the arithmetic accepts numbers


def test_a_value_that_breaks_a_rule_is_refused_naming_the_registered_thing_and_the_field(reports):
    @source(params=Level)
    def level(params: Level) -> Signal:
        return Constant(params.value)

    with pytest.raises(ParameterError, match="source 'level'") as refused:
        level(value=2)
    reports(refused.value, "value")
    with pytest.raises(ParameterError) as unknown:
        level(speed=1)
    reports(unknown.value, "speed")


POINTS = [{"t": 0.5, "value": 2.0, "ease": "in"}, {"t": 1.5, "value": -1.0}, {"t": 4, "value": 0.25, "ease": "out"}]
POINTS_AS_KEYFRAMES = (
    Keyframe(t=0.5, value=2.0, ease="in"),
    Keyframe(t=1.5, value=-1.0, ease="linear"),
    Keyframe(t=4.0, value=0.25, ease="out"),
)


def test_a_required_input_arrives_validated_as_a_tuple_of_keyframes():
    received = []

    @source(params=Level)
    def track(params: Level, *, points: tuple[Keyframe, ...]) -> Signal:
        received.append(points)
        return Constant(params.value)

    assert track(points=POINTS, value=0.75).sample(0.0).tolist() == 0.75  # the parameters still work beside it
    assert received == [POINTS_AS_KEYFRAMES]
    assert type(received[0]) is tuple
    assert track.inputs == {"points": True}


def test_a_shaper_takes_its_input_after_its_signal_and_parameters():
    received = []

    @shaper(params=Level)
    def chase(signal: Signal, params: Level, *, points: tuple[Keyframe, ...]) -> Signal:
        received.append(points)
        return signal * params.value

    assert chase(Constant(4.0), points=POINTS, value=0.25).sample(0.0).tolist() == 1.0
    assert received == [POINTS_AS_KEYFRAMES]


def test_an_optional_input_arrives_as_none_when_left_out_and_validated_when_given():
    received = []

    @source(params=Nothing)
    def maybe(params: Nothing, *, points: tuple[Keyframe, ...] | None = None) -> Signal:
        received.append(points)
        return Constant(0.0)

    maybe()
    maybe(points=POINTS)
    assert received == [None, POINTS_AS_KEYFRAMES]
    assert maybe.inputs == {"points": False}


def test_a_thing_without_inputs_declares_none():
    @source(params=Nothing)
    def plain(params: Nothing) -> Signal:
        return Constant(0.0)

    assert plain.inputs == {}


def test_a_missing_required_input_is_a_type_error_naming_it():
    @source(params=Nothing)
    def track(params: Nothing, *, points: tuple[Keyframe, ...]) -> Signal:
        return Constant(0.0)

    with pytest.raises(TypeError, match=r"track.*points"):
        track()


def test_an_input_the_function_does_not_declare_is_a_type_error_naming_it():
    @source(params=Level)
    def level(params: Level) -> Signal:
        return Constant(params.value)

    with pytest.raises(TypeError, match="source 'level' takes no input 'points'"):
        level(points=POINTS)


def test_an_input_that_breaks_a_rule_is_a_parameter_error_naming_the_thing_and_the_path(reports):
    @source(params=Nothing)
    def track(params: Nothing, *, points: tuple[Keyframe, ...]) -> Signal:
        return Constant(0.0)

    with pytest.raises(ParameterError, match="source 'track'") as bad_ease:
        track(points=[{"t": 0, "value": 1}, {"t": 1, "value": 1, "ease": "wobble"}])
    reports(bad_ease.value, "points[1].ease")
    assert str(bad_ease.value).startswith("source 'track': points[1].ease: ")  # no private model name between
    with pytest.raises(ParameterError, match="source 'track'") as unsorted:
        track(points=[{"t": 2, "value": 1}, {"t": 1, "value": 1}])
    reports(unsorted.value, "points[1].t")


def test_none_for_an_optional_input_counts_as_left_out_but_a_required_one_is_refused(reports):
    received = []

    @source(params=Nothing)
    def maybe(params: Nothing, *, points: tuple[Keyframe, ...] | None = None) -> Signal:
        received.append(points)
        return Constant(0.0)

    @source(params=Nothing)
    def must(params: Nothing, *, points: tuple[Keyframe, ...]) -> Signal:
        return Constant(0.0)

    maybe(points=None)
    assert received == [None]
    with pytest.raises(ParameterError, match="source 'must'") as refused:
        must(points=None)
    reports(refused.value, "points")


def test_the_declared_inputs_cannot_be_changed_by_a_caller():
    @source(params=Nothing)
    def track(params: Nothing, *, points: tuple[Keyframe, ...]) -> Signal:
        return Constant(0.0)

    with pytest.raises(TypeError):
        track.inputs["points"] = False  # type: ignore[index]  # a read-only mapping
    assert track.inputs == {"points": True}


def test_an_unknown_name_beside_an_input_is_still_a_parameter_error(reports):
    @source(params=Level)
    def track(params: Level, *, points: tuple[Keyframe, ...]) -> Signal:
        return Constant(params.value)

    with pytest.raises(ParameterError) as unknown:
        track(points=POINTS, speed=1)
    reports(unknown.value, "speed")


def test_listings_are_sorted_and_split_by_kind():
    @source(params=Nothing)
    def zero(params: Nothing) -> Signal:
        return Constant(0.0)

    @source(params=Nothing)
    def one(params: Nothing) -> Signal:
        return Constant(1.0)

    @shaper(params=Nothing)
    def keep(signal: Signal, params: Nothing) -> Signal:
        return signal

    assert {"one", "zero"} <= set(registry.sources())
    assert "keep" in registry.shapers()
    assert "keep" not in registry.sources()
    assert "one" not in registry.shapers()
    for listing in (registry.sources(), registry.shapers(), registry.all_registered()):
        assert list(listing) == sorted(listing)
    assert registry.get("keep") is keep


def test_a_registered_operation_takes_its_inputs_and_values_and_returns_its_data(throwaway_registry):
    @operation(params=Level)
    def keep_loud(params: Level, *, events: tuple[Event, ...]) -> tuple[Event, ...]:
        return tuple(event for event in events if event.strength >= params.value)

    # the events arrive checked into time order (0.5, 1.0, 2.0); a value of 0.7 keeps only the last, the default 0.5 two
    given = [{"t": 2.0, "strength": 0.9}, {"t": 1.0, "strength": 0.2}, {"t": 0.5, "strength": 0.6}]
    assert [event.t for event in keep_loud(events=given, value=0.7)] == [2.0]
    assert [event.t for event in keep_loud(events=given)] == [0.5, 2.0]
    assert (keep_loud.name, keep_loud.kind, keep_loud.params_model) == ("keep_loud", "operation", Level)
    assert keep_loud.inputs == {"events": True}


def test_an_operation_may_return_a_series(throwaway_registry):
    @operation(params=Nothing)
    def halve(params: Nothing, *, series: Series) -> Series:
        return Series(rate=series.rate, values=tuple(value / 2 for value in series.values), unit=series.unit)

    halved = halve(series={"rate": 4, "values": [1.0, 0.5], "unit": "ratio"})
    assert (halved.rate, halved.values) == (4.0, (0.5, 0.25))


def test_an_operation_is_listed_apart_from_sources_and_shapers_and_found_through_get_and_lib():
    @operation(params=Nothing)
    def nothing_at_all(params: Nothing, *, events: tuple[Event, ...]) -> tuple[Event, ...]:
        return events

    @source(params=Nothing)
    def zero(params: Nothing) -> Signal:
        return Constant(0.0)

    assert "nothing_at_all" in registry.operations()
    assert "zero" not in registry.operations()
    assert "nothing_at_all" not in registry.sources()
    assert "nothing_at_all" not in registry.shapers()
    assert "nothing_at_all" in registry.all_registered()
    assert list(registry.operations()) == sorted(registry.operations())
    assert registry.get("nothing_at_all") is nothing_at_all
    assert lib.nothing_at_all is nothing_at_all
    assert "nothing_at_all" in dir(lib)


def test_calling_an_operation_with_a_signal_is_a_type_error():
    @operation(params=Nothing)
    def pass_on(params: Nothing, *, events: tuple[Event, ...]) -> tuple[Event, ...]:
        return events

    with pytest.raises(TypeError, match="pass_on"):
        pass_on(Constant(1.0), events=[])


def test_an_operation_cannot_share_a_name_with_a_source():
    @source(params=Nothing)
    def twin(params: Nothing) -> Signal:
        return Constant(0.0)

    def operation_called_twin(params: Nothing, *, events: tuple[Event, ...]) -> tuple[Event, ...]:
        return events

    operation_called_twin.__name__ = "twin"
    with pytest.raises(ValueError, match="twin"):
        operation(params=Nothing)(operation_called_twin)
    assert registry.get("twin") is twin


def test_an_unknown_name_lists_what_exists():
    @source(params=Nothing)
    def known(params: Nothing) -> Signal:
        return Constant(0.0)

    with pytest.raises(UnknownNameError, match="known") as refused:
        registry.get("unknwon")
    assert isinstance(refused.value, KeyError)
    assert "unknwon" in str(refused.value)
    assert not str(refused.value).startswith("'")  # a KeyError would quote the whole message


def not_params(signal: Signal, params: Nothing) -> Signal:
    return signal


def one_argument(params: Nothing) -> Signal:
    return Constant(0.0)


BROKEN = {
    "parameters that are not a Params model": lambda: source(params=dict)(_named("fine")),  # type: ignore[type-var]
    "a source taking a signal": lambda: source(params=Nothing)(not_params),  # type: ignore[arg-type]
    "a shaper taking none": lambda: shaper(params=Nothing)(one_argument),  # type: ignore[arg-type]  # the wrong shape is the case
    "a name that is not lower case": lambda: source(params=Nothing)(_named("Loud")),
    "a name with a space": lambda: source(params=Nothing)(_named("a b")),
    "a name with a trailing newline": lambda: source(params=Nothing)(_named("level\n")),
}


def _named(name):
    def function(params: Nothing) -> Signal:
        return Constant(0.0)

    function.__name__ = name
    return function


@pytest.mark.parametrize("register", BROKEN.values(), ids=BROKEN.keys())
def test_a_broken_registration_is_refused_and_registers_nothing(register):
    before = dict(registry.all_registered())
    with pytest.raises(ValueError, match=r"\w"):
        register()
    assert dict(registry.all_registered()) == before


class Pointed(Params):
    """Parameters with a field named like an input."""

    points: Finite = param(0.5, doc="Clashes with the input of the same name.", unit="count", min=0, max=1)


def unknown_input(params: Nothing, *, colour: object) -> Signal:
    return Constant(0.0)


def odd_default(params: Nothing, *, points: object = ()) -> Signal:
    return Constant(0.0)


def takes_kwargs(params: Nothing, **extra: object) -> Signal:
    return Constant(0.0)


def takes_args(params: Nothing, *extra: object) -> Signal:
    return Constant(0.0)


def positional_input(params: Nothing, points: object) -> Signal:
    return Constant(0.0)


def kwargs_named_like_an_input(params: Nothing, **points: object) -> Signal:
    return Constant(0.0)


def returns_a_signal(params: Nothing, *, events: tuple[Event, ...]) -> Signal:
    return Constant(0.0)


def returns_nothing_declared(params: Nothing, *, events: tuple[Event, ...]):
    return events


def returns_a_list(params: Nothing, *, events: tuple[Event, ...]) -> list[Event]:
    return list(events)


def returns_other_things(params: Nothing, *, events: tuple[Event, ...]) -> tuple[float, ...]:
    return ()


def returns_a_series_or_none(params: Nothing, *, events: tuple[Event, ...]) -> Series | None:
    return None


BROKEN_OPERATIONS = {
    "a signal": returns_a_signal,
    "no declared result": returns_nothing_declared,
    "a list of events": returns_a_list,
    "a tuple of something else": returns_other_things,
    "a series or nothing": returns_a_series_or_none,
}


@pytest.mark.parametrize("function", BROKEN_OPERATIONS.values(), ids=BROKEN_OPERATIONS.keys())
def test_an_operation_that_does_not_declare_events_or_a_series_as_its_result_is_refused_naming_it(function):
    before = dict(registry.all_registered())
    with pytest.raises(ValueError, match=function.__name__):
        operation(params=Nothing)(function)
    assert dict(registry.all_registered()) == before


def operation_taking_a_signal(signal: Signal, params: Nothing, *, events: tuple[Event, ...]) -> tuple[Event, ...]:
    return events


BROKEN_INPUTS = {
    "a keyword-only argument that is no input": (lambda: source(params=Nothing)(unknown_input), "'colour'"),
    "an input with a default other than None": (lambda: source(params=Nothing)(odd_default), "'points'"),
    "arbitrary keyword arguments": (lambda: source(params=Nothing)(takes_kwargs), "'extra'"),
    "arbitrary positional arguments": (lambda: source(params=Nothing)(takes_args), "'extra'"),
    "an input that is positional": (lambda: source(params=Nothing)(positional_input), "'points'"),
    "arbitrary keyword arguments named like an input": (
        lambda: source(params=Nothing)(kwargs_named_like_an_input),
        "'points'",
    ),
    "a parameter named like an input": (lambda: source(params=Pointed)(_named("pointy")), "'points'"),
    "a shaper written without a signal": (
        lambda: shaper(params=Nothing)(one_argument),  # type: ignore[arg-type]  # the wrong shape is the case
        r"one_argument\(signal, params\)",
    ),
    "an operation written with a signal": (
        lambda: operation(params=Nothing)(operation_taking_a_signal),  # type: ignore[arg-type]  # the wrong shape is the case
        r"operation_taking_a_signal\(params\)",
    ),
}


@pytest.mark.parametrize(("register", "named"), BROKEN_INPUTS.values(), ids=BROKEN_INPUTS.keys())
def test_a_registration_with_a_wrong_input_is_refused_naming_it_and_registers_nothing(register, named):
    before = dict(registry.all_registered())
    with pytest.raises(ValueError, match=named):
        register()
    assert dict(registry.all_registered()) == before


LAZY_ANNOTATIONS = """
from __future__ import annotations

from bullterrier_control.plugin import Event, Params, Series, operation


class NothingParams(Params):
    \"\"\"No parameters.\"\"\"


@operation(params=NothingParams)
def lazy_events(params: NothingParams, *, events: tuple[Event, ...]) -> tuple[Event, ...]:
    return events


@operation(params=NothingParams)
def lazy_series(params: NothingParams, *, series: Series) -> Series:
    return series
"""


def test_a_module_with_postponed_annotations_registers_its_operations(tmp_path):
    path = tmp_path / "lazy_package.py"
    path.write_text(LAZY_ANNOTATIONS, encoding="utf-8")
    with registry.isolated():
        import_from_file(path, "lazy_package")
        assert registry.get("lazy_events").returns == "events"
        assert registry.get("lazy_series").returns == "series"
        assert registry.get("lazy_events")(events=[{"t": 1.0}])[0].t == 1.0


def test_an_operation_whose_postponed_annotation_names_nothing_is_refused_saying_so(tmp_path):
    path = tmp_path / "broken_lazy.py"
    path.write_text(LAZY_ANNOTATIONS.replace("-> Series:", "-> Missing:"), encoding="utf-8")
    with registry.isolated(), pytest.raises(ValueError, match=r"lazy_series.*Missing"):
        import_from_file(path, "broken_lazy")


def test_an_operation_whose_return_annotation_is_wrong_says_what_it_found():
    with pytest.raises(ValueError, match="found list"):
        operation(params=Nothing)(returns_a_list)  # type: ignore[type-var]  # the declaration is the case
    with pytest.raises(ValueError, match="found none"):
        operation(params=Nothing)(returns_nothing_declared)


def test_a_second_function_with_a_registered_name_is_refused_naming_both():
    @source(params=Nothing)
    def clash(params: Nothing) -> Signal:
        return Constant(0.0)

    def other():
        @source(params=Nothing)
        def clash(params: Nothing) -> Signal:
            return Constant(1.0)

    with pytest.raises(ValueError, match="clash"):
        other()
    assert registry.get("clash").summary == clash.summary


RELOADABLE = """
from bullterrier_control.plugin import Constant, Params, Signal, source

VALUE = {value}


class LevelParams(Params):
    \"\"\"No parameters.\"\"\"


@source(params=LevelParams)
def reloaded(params: LevelParams) -> Signal:
    return Constant(VALUE)
"""


def import_from_file(path, name="reloadable_package"):
    """Run the file as a module called `name`, as importing (or reloading) it would: a new module object each time."""
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_a_module_that_is_imported_again_replaces_what_it_registered_as_reloading_does(tmp_path):
    path = tmp_path / "reloadable_package.py"
    path.write_text(RELOADABLE.format(value=1.0), encoding="utf-8")
    import_from_file(path)
    assert registry.get("reloaded")().sample(0.0).tolist() == 1.0
    path.write_text(RELOADABLE.format(value=22.0), encoding="utf-8")  # another size: no stale bytecode
    import_from_file(path)
    assert registry.get("reloaded")().sample(0.0).tolist() == 22.0


def test_two_functions_made_by_one_factory_do_not_replace_each_other():
    def make(value):
        @source(params=Nothing)
        def made(params: Nothing) -> Signal:
            return Constant(value)

    make(1.0)
    with pytest.raises(ValueError, match="made"):
        make(2.0)
    assert registry.get("made")().sample(0.0).tolist() == 1.0


def test_a_source_and_a_shaper_cannot_share_a_name():
    @source(params=Nothing)
    def shared(params: Nothing) -> Signal:
        return Constant(0.0)

    with pytest.raises(ValueError, match="shared"):

        @shaper(params=Nothing)
        def shared(signal: Signal, params: Nothing) -> Signal:
            return signal


def test_a_throwaway_registration_leaves_no_trace(throwaway_registry):
    @source(params=Nothing)
    def passing(params: Nothing) -> Signal:
        return Constant(0.0)

    registry.unregister("passing")
    assert "passing" not in registry.all_registered()
    with pytest.raises(UnknownNameError):
        registry.unregister("passing")


class FakeEntryPoint:
    """What `importlib.metadata` gives for an entry point: a name, the module it names, and `load`. Like importing a
    module, `load` runs the package's code once; a failed import is not remembered, so it runs again."""

    def __init__(self, name, action):
        self.name, self.value, self._action, self.loads, self._imported = name, f"{name}_module", action, 0, False

    def load(self):
        self.loads += 1
        if not self._imported:
            self._action()
            self._imported = True


@pytest.fixture
def entry_points(monkeypatch, empty_registry):
    """`entry_points(*fakes)`: the installed packages of the registry's group are exactly these fakes."""

    def install(*fakes):
        monkeypatch.setattr("importlib.metadata.entry_points", lambda *, group: list(fakes))

    return install


def test_the_packages_of_the_group_are_loaded_once_on_the_first_query_not_before(entry_points):
    def define():
        @source(params=Nothing)
        def from_package(params: Nothing) -> Signal:
            return Constant(3.0)

    package = FakeEntryPoint("package", define)
    entry_points(package)
    assert package.loads == 0
    assert "from_package" in registry.sources()
    registry.get("from_package")
    registry.all_registered()
    assert package.loads == 1


def test_threads_that_query_together_wait_for_one_load_and_all_see_everything(entry_points):
    started = threading.Barrier(8)

    def define():
        @source(params=Nothing)
        def shared_load(params: Nothing) -> Signal:
            return Constant(1.0)

    package = FakeEntryPoint("package", define)
    entry_points(package)

    def query(_):
        started.wait()
        return "shared_load" in registry.sources()

    with ThreadPoolExecutor(8) as pool:
        assert all(pool.map(query, range(8)))
    assert package.loads == 1


def test_a_package_that_fails_to_load_raises_naming_its_entry_point_and_the_next_query_tries_again(entry_points):
    attempts = []

    def define():
        attempts.append(1)
        if len(attempts) == 1:
            raise RuntimeError("not today")

        @source(params=Nothing)
        def recovered(params: Nothing) -> Signal:
            return Constant(1.0)

    entry_points(FakeEntryPoint("flaky", define))
    with pytest.raises(BullterrierControlError, match="flaky") as refused:
        registry.sources()
    assert "not today" in str(refused.value)
    assert "recovered" in registry.sources()
    assert len(attempts) == 2


def test_the_packages_are_loaded_in_the_order_of_their_names_whatever_order_they_are_installed_in(entry_points):
    order = []
    entry_points(FakeEntryPoint("b", lambda: order.append("b")), FakeEntryPoint("a", lambda: order.append("a")))
    registry.sources()
    assert order == ["a", "b"]


def test_a_package_that_queries_the_registry_while_it_loads_sees_the_table_so_far_and_loading_goes_on(entry_points):
    seen = []

    def first():
        @source(params=Nothing)
        def early(params: Nothing) -> Signal:
            return Constant(0.0)

        seen.append(sorted(registry.all_registered()))  # a query during loading must not start loading again

    def second():
        @source(params=Nothing)
        def late(params: Nothing) -> Signal:
            return Constant(0.0)

    one, two = FakeEntryPoint("a", first), FakeEntryPoint("b", second)
    entry_points(one, two)
    assert {"early", "late"} <= set(registry.sources())
    assert seen == [["early"]]
    assert (one.loads, two.loads) == (1, 1)


def test_a_package_that_fails_after_registering_leaves_nothing_of_itself_behind(entry_points):
    def half_done():
        @source(params=Nothing)
        def half(params: Nothing) -> Signal:
            return Constant(0.0)

        raise RuntimeError("not today")

    entry_points(FakeEntryPoint("broken", half_done))
    for _ in range(2):
        # the second try registers `half` again: it would be refused as a name taken had the first one stayed behind
        with pytest.raises(BullterrierControlError, match="not today"):
            registry.sources()
    with pytest.raises(BullterrierControlError, match="not today"):
        registry.get("half")


def test_a_package_loaded_before_a_failing_one_is_not_loaded_again_or_blamed(entry_points):
    def good():
        @source(params=Nothing)
        def kept(params: Nothing) -> Signal:
            return Constant(0.0)

    def bad():
        raise RuntimeError("no")

    entry_points(FakeEntryPoint("a_good", good), FakeEntryPoint("b_bad", bad))
    for _ in range(2):  # every query names the package that fails, never the one that loaded fine
        with pytest.raises(BullterrierControlError, match="b_bad"):
            registry.sources()


def test_an_isolated_registry_starts_empty_and_gives_back_what_it_took(throwaway_registry):
    before = dict(registry.all_registered())
    assert before  # the installed package registered its own
    with registry.isolated():
        assert registry.all_registered() == {}  # an already imported package does not register again

        @source(params=Nothing)
        def inside(params: Nothing) -> Signal:
            return Constant(0.0)

        assert list(registry.all_registered()) == ["inside"]
    assert dict(registry.all_registered()) == before


def test_leaving_an_isolated_block_gives_back_whether_the_packages_were_loaded(entry_points):
    package = FakeEntryPoint("package", lambda: None)
    entry_points(package)
    registry.sources()
    assert package.loads == 1
    with registry.isolated():
        pass
    registry.sources()
    assert package.loads == 1  # not loaded again: the registry was loaded before the block, and is after it
