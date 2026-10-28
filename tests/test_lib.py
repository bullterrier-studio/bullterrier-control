import numpy as np
import pytest

from bullterrier_control import Event, Series, Unit, from_series, lib, registry
from bullterrier_control.params import Params, param
from bullterrier_control.plugin import Constant, Keyframe, Signal, operation, shaper, source
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
    def chase(signal: Signal, params: Level, *, points: tuple[Keyframe, ...] | None = None) -> Signal:
        """Follows keyframes if given."""
        return signal

    text = track.__doc__
    assert text is not None
    assert "source track(*, points, **values) -> signal" in text
    assert "points (required)" in text
    assert "strictly increasing" in text
    assert text.index("A track through keyframes.") < text.index("Inputs:") < text.index("Parameters:")
    optional = chase.__doc__
    assert optional is not None
    assert "shaper chase(signal, *, points=None, **values) -> signal" in optional
    assert "points (optional)" in optional


def test_help_of_a_thing_without_inputs_has_no_inputs_block():
    text = lib.slew.__doc__
    assert text is not None
    assert "Inputs:" not in text
    assert "shaper slew(signal, **values) -> signal" in text


def test_help_of_a_thing_without_parameters_has_no_parameters_heading(throwaway_registry):
    class Nothing(Params):
        """No parameters."""

    @source(params=Nothing)
    def silence(params: Nothing) -> Signal:
        """A signal that is always 0."""
        return Constant(0.0)

    text = silence.__doc__
    assert text is not None
    assert "A signal that is always 0." in text
    assert "Parameters:" not in text
    assert "Parameters:" in lib.slew.__doc__  # type: ignore[operator]  # a thing with parameters still lists them


def test_help_of_an_operation_shows_its_inputs_and_what_it_returns(throwaway_registry):
    @operation(params=Level)
    def keep_loud(params: Level, *, events: tuple[Event, ...]) -> tuple[Event, ...]:
        """The events at least as strong as `value`."""
        return events

    @operation(params=Level)
    def flatten(params: Level, *, series: Series) -> Series:
        """The same series."""
        return series

    text, other = keep_loud.__doc__, flatten.__doc__
    assert text is not None
    assert other is not None
    assert "operation keep_loud(*, events, **values) -> events" in text
    assert "events (required)" in text
    assert "The events at least as strong as `value`." in text
    assert "operation flatten(*, series, **values) -> series" in other
    assert "series (required)" in other
    assert "Signal" not in text + other


def test_help_names_what_a_thing_gives_in_lower_case_whatever_its_kind():
    assert "-> signal" in (lib.lfo.__doc__ or "")
    assert "-> events" in (lib.select.__doc__ or "")
    assert "-> series" in (lib.normalise.__doc__ or "")


def test_help_marks_a_parameter_that_wraps_as_cyclic():
    text = lib.lfo.__doc__
    assert text is not None
    assert "phase (cycles, 0 to 1), default 0.0, cyclic" in text
    assert "rate (cycles, 0 to 1000), default 1.0\n" in text  # a rate does not wrap: no `, cyclic`


class Ramp(Signal):
    """The time up to 1, then 1; `aligned=False` returns it as a float64 view at an odd byte offset, which is legal."""

    def __init__(self, *, aligned: bool) -> None:
        self.aligned = aligned

    def compute(self, times):
        values = np.minimum(times, 1.0)
        if self.aligned:
            return values
        view = np.frombuffer(bytearray(times.size * 8 + 1), dtype=np.float64, offset=1, count=times.size)
        view[:] = values
        return view


def test_a_shaper_over_a_signal_whose_compute_returns_a_misaligned_array_samples_as_over_any_other():
    times = [0.0, 0.5, 1.0, 5.0]
    expected = lib.slew(Ramp(aligned=True), rise=2.0, fall=2.0).sample(times)
    assert lib.slew(Ramp(aligned=False), rise=2.0, fall=2.0).sample(times).tolist() == expected.tolist()
