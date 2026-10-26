import pytest

from bullterrier_control import Series, Unit, from_series, lib, registry
from bullterrier_control.inputs import INPUTS
from bullterrier_control.params import Params, param
from bullterrier_control.plugin import Constant, Keyframe, Signal, shaper, source
from bullterrier_control.validated import Finite


class Level(Params):
    """A level."""

    value: Finite = param(0.5, doc="The level, as a share of the range.", unit="fraction", min=0, max=1)


def test_every_registered_source_and_shaper_is_an_attribute_of_lib():
    assert lib.slew is registry.get("slew")
    assert set(registry.all_registered()) <= set(dir(lib))


def test_a_shaper_from_lib_is_called_like_a_function():
    ramp = from_series(Series(rate=1.0, values=(0.0, 2.0), unit=Unit.RATIO))
    assert lib.clip(ramp, low=0.0, high=1.0).sample([0.0, 0.25, 1.0]).tolist() == [0.0, 0.5, 1.0]


def test_a_name_that_is_not_registered_is_an_attribute_error_listing_what_is():
    with pytest.raises(AttributeError, match="slew"):
        lib.slwe  # noqa: B018  (the attribute access is the test)


def test_a_private_name_is_not_looked_up_as_a_source():
    with pytest.raises(AttributeError):
        getattr(lib, "_" + "anything")  # built at run time: the name is the test, not a private attribute of ours


def test_help_shows_the_summary_inputs_and_every_parameter_with_unit_range_and_default():
    text = lib.slew.__doc__
    assert text is not None
    assert "climbs and falls no faster" in text
    for expected in ("rise", "seconds", "0 to 3600", "default 0.1", "rate", "hertz", "advanced"):
        assert expected in text, expected
    assert "signal" in text  # a shaper says what it takes


def test_help_lists_the_inputs_between_the_description_and_the_parameters(throwaway_registry):
    @source(params=Level)
    def track(params: Level, *, points: tuple[Keyframe, ...]) -> Signal:
        """A track through keyframes."""
        return Constant(params.value)

    @shaper(params=Level)
    def follow(signal: Signal, params: Level, *, points: tuple[Keyframe, ...] | None = None) -> Signal:
        """Follows keyframes if given."""
        return signal

    text = track.__doc__
    assert text is not None
    assert "source track(*, points, **values) -> Signal" in text
    assert "points (required)" in text
    assert INPUTS["points"].doc in text
    assert text.index("A track through keyframes.") < text.index("Inputs:") < text.index("Parameters:")
    optional = follow.__doc__
    assert optional is not None
    assert "shaper follow(signal, *, points=None, **values) -> Signal" in optional
    assert "points (optional)" in optional


def test_help_of_a_thing_without_inputs_has_no_inputs_block():
    text = lib.slew.__doc__
    assert text is not None
    assert "Inputs:" not in text
    assert "shaper slew(signal, **values) -> Signal" in text
