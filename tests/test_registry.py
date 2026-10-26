import threading
from concurrent.futures import ThreadPoolExecutor

import pytest

from bullterrier_control import BullterrierControlError, FormatError, UnknownNameError, registry
from bullterrier_control.formats.model import Finite
from bullterrier_control.params import Params, param
from bullterrier_control.plugin import Constant, Signal, shaper, source


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


def test_a_value_that_breaks_a_rule_is_refused_naming_the_registered_thing_and_the_field(reports):
    @source(params=Level)
    def level(params: Level) -> Signal:
        return Constant(params.value)

    with pytest.raises(FormatError, match="source 'level'") as refused:
        level(value=2)
    reports(refused.value, "value")
    with pytest.raises(FormatError) as unknown:
        level(speed=1)
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
    "a shaper taking none": lambda: shaper(params=Nothing)(one_argument),  # type: ignore[arg-type]
    "a name that is not lower case": lambda: source(params=Nothing)(_named("Loud")),
    "a name with a space": lambda: source(params=Nothing)(_named("a b")),
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


def test_registering_the_same_function_again_replaces_it_as_reloading_a_module_does():
    def define(value):
        @source(params=Nothing)
        def twice(params: Nothing) -> Signal:
            return Constant(value)

        return twice

    define(1.0)
    define(2.0)
    assert registry.get("twice")().sample(0.0).tolist() == 2.0


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
    """What `importlib.metadata` gives for an entry point: a name, the module it names, and `load`."""

    def __init__(self, name, action):
        self.name, self.value, self._action, self.loads = name, f"{name}_module", action, 0

    def load(self):
        self.loads += 1
        return self._action()


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
