import contextlib
import copy
import pickle
import warnings

import numpy as np
import pytest

from bullterrier_control import FormatError, Series, Unit, from_series

VALID = {"start": 1.5, "rate": 100, "values": [0.0, 0.25, 1], "unit": "fraction", "scaling": "peak"}


def test_a_valid_series_keeps_its_values():
    series = Series.parse(VALID)
    assert series.start == 1.5
    assert series.rate == 100.0
    assert series.values.tolist() == [0.0, 0.25, 1.0]
    assert series.unit is Unit.FRACTION
    assert series.scaling == "peak"


def test_the_start_defaults_to_zero():
    assert Series.parse({"rate": 10, "values": [3], "unit": "hertz"}).start == 0.0


def test_a_list_and_a_tuple_give_the_same_series():
    assert Series.parse({**VALID, "values": (0.0, 0.25, 1)}) == Series.parse(VALID)


def test_decibels_need_no_scaling_and_may_be_negative():
    series = Series.parse({"rate": 50, "values": [-60, -3.5], "unit": "decibels"})
    assert series.values.tolist() == [-60.0, -3.5]


INVALID = {
    "rate zero": ({**VALID, "rate": 0}, "rate"),
    "rate negative": ({**VALID, "rate": -1}, "rate"),
    "rate a bool": ({**VALID, "rate": True}, "rate"),
    "rate a string": ({**VALID, "rate": "100"}, "rate"),
    "rate missing": ({k: v for k, v in VALID.items() if k != "rate"}, "rate"),
    "no values": ({**VALID, "values": []}, "values"),
    "a nan value": ({**VALID, "values": [0.1, float("nan")]}, "values[1]"),
    "an infinite value": ({**VALID, "values": [float("inf")]}, "values[0]"),
    "a bool value": ({**VALID, "values": [True]}, "values[0]"),
    "negative start": ({**VALID, "start": -0.5}, "start"),
    "unknown unit": ({**VALID, "unit": "percent"}, "unit"),
    "fraction without scaling": ({k: v for k, v in VALID.items() if k != "scaling"}, "scaling"),
    "fraction with empty scaling": ({**VALID, "scaling": ""}, "scaling"),
    "fraction above one": ({**VALID, "values": [0.5, 1.5]}, "values[1]"),
    "fraction below zero": ({**VALID, "values": [-0.1]}, "values[0]"),
    "unknown key": ({**VALID, "fps": 30}, "fps"),
}


@pytest.mark.parametrize(("data", "named"), INVALID.values(), ids=INVALID.keys())
def test_an_invalid_series_is_refused_naming_the_problem(data, named, reports):
    with pytest.raises(FormatError) as refused:
        Series.parse(data)
    reports(refused.value, named)


def test_a_series_cannot_be_changed():
    series = Series.parse(VALID)
    with pytest.raises(AttributeError):
        series.rate = 1  # type: ignore[misc]


def test_a_series_built_directly_is_checked_like_a_parsed_one():
    with pytest.raises(FormatError, match="rate"):
        Series(rate=0, values=(1.0,), unit=Unit.RATIO)


def test_a_rule_across_fields_names_its_path_and_does_not_echo_the_whole_input():
    with pytest.raises(FormatError) as refused:
        Series.parse({**VALID, "values": [0.5, 1.5]})
    assert str(refused.value) == 'Series: values[1]: must be in [0, 1] when unit is "fraction" (got 1.5)'


def test_a_few_broken_values_are_all_listed():
    with pytest.raises(FormatError) as refused:
        Series(rate=10, values=[1.0, np.nan, 2.0, np.inf, -np.inf], unit="hertz")
    assert [path for path, _ in refused.value.problems] == ["values[1]", "values[3]", "values[4]"]
    assert str(refused.value) == (
        "Series: values[1]: must be finite (got NaN); values[3]: must be finite (got Infinity); "
        "values[4]: must be finite (got -Infinity)"
    )


def test_a_million_broken_values_give_a_short_list_that_still_counts_them_all():
    with pytest.raises(FormatError) as refused:
        Series(rate=10, values=np.full(1_000_000, np.nan), unit="hertz")
    assert len(refused.value.problems) <= 100
    assert refused.value.problems[0] == ("values[0]", "must be finite (got NaN)")
    # 20 are shown, so 999_980 of the 1_000_000 are left out
    assert str(refused.value).endswith("; ... and 999980 more")


def a_series(values):
    return Series(rate=1, values=values, unit=Unit.RATIO)


def misaligned(values):
    """`values` as a float64 view that starts one byte into its buffer: not aligned."""
    raw = np.zeros(len(values) * 8 + 1, dtype=np.uint8)
    view = raw[1:].view(np.float64)
    view[:] = values
    assert not view.flags.aligned
    return view


VALUES_GIVEN = {
    "a list of floats": ([0.5, 1.5], [0.5, 1.5]),
    "a tuple": ((0.5, 1.5), [0.5, 1.5]),
    "a mixed list of whole numbers and decimals": ([1, 0.5], [1.0, 0.5]),
    "a range": (range(3), [0.0, 1.0, 2.0]),
    "a float64 array": (np.array([0.5, 1.5]), [0.5, 1.5]),
    "an integer array": (np.array([1, -2], dtype=np.int32), [1.0, -2.0]),
    "a float32 array": (np.array([0.5, 0.25], dtype=np.float32), [0.5, 0.25]),
    "a strided view": (np.arange(6.0)[::2], [0.0, 2.0, 4.0]),
    "a reversed view": (np.arange(3.0)[::-1], [2.0, 1.0, 0.0]),
    "a misaligned view": (misaligned([0.5, 1.5, 2.5]), [0.5, 1.5, 2.5]),
}


@pytest.mark.parametrize(("given", "expected"), VALUES_GIVEN.values(), ids=VALUES_GIVEN.keys())
def test_values_of_any_array_like_become_a_float64_array(given, expected):
    values = a_series(given).values
    assert values.dtype == np.float64
    assert values.shape == (len(expected),)
    assert values.tolist() == expected


VALUES_REFUSED = {
    "a numpy bool array": (np.array([True, False]), "values[0]"),
    "a list of bools": ([False, True], "values[0]"),
    "a bool among numbers": ([0.5, True], "values[1]"),
    "text": (["0.5"], "values[0]"),
    "an integer array beyond 2^53": (np.array([1, 2**60]), "values[1]"),
    "a whole number beyond 2^53": ([2**53 + 1], "values[0]"),
    "a 2-D array": (np.zeros((2, 2)), "values"),
    "a 0-d array": (np.array(3.0), "values"),
    "a single number": (3.0, "values"),
    "nothing": (None, "values"),
    "a nested list": ([[0.5], [1.5]], "values"),
    "a complex array": (np.array([1j]), "values[0]"),
}


@pytest.mark.parametrize(("given", "named"), VALUES_REFUSED.values(), ids=VALUES_REFUSED.keys())
def test_values_that_are_not_numbers_are_refused_naming_the_field(given, named, reports):
    with pytest.raises(FormatError) as refused:
        a_series(given)
    reports(refused.value, named)


def test_values_are_a_read_only_array_of_the_series_own():
    source = np.array([0.5, 1.5])
    series = a_series(source)
    source[0] = 9.0
    assert series.values.tolist() == [0.5, 1.5]
    assert not series.values.flags.writeable
    with pytest.raises(ValueError, match="read-only"):
        series.values[0] = 9.0
    with pytest.raises(ValueError, match="WRITEABLE"):
        series.values.setflags(write=True)


def test_no_array_under_the_values_can_be_made_writeable():
    series = a_series([0.5, 1.5])
    array = series.values
    while isinstance(array, np.ndarray):  # the values, and every array they are a view of
        with pytest.raises(ValueError, match="WRITEABLE"):
            array.setflags(write=True)
        array = array.base
    with pytest.raises(ValueError, match="read-only"):
        series.values[0] = 99.0
    assert series.values.tolist() == [0.5, 1.5] == series.to_dict()["values"]


UNIT_NAMES = ["fraction", "decibels", "hertz", "midi", "ratio", "seconds", "beats"]


@pytest.mark.parametrize("name", UNIT_NAMES)
def test_a_unit_is_given_by_its_member_or_its_name_and_read_as_the_member(name):
    member = getattr(Unit, name.upper())
    scaling = "peak" if name == "fraction" else None
    by_name = Series(rate=1, values=[0.5], unit=name, scaling=scaling)
    assert by_name.unit is member
    assert Series(rate=1, values=[0.5], unit=member, scaling=scaling) == by_name
    assert (member.value, str(member), member.name) == (name, name, name.upper())


def test_a_unit_is_not_its_name():
    assert Unit.RATIO != "ratio"
    assert Unit.RATIO == Unit.RATIO
    assert Unit.RATIO != Unit.HERTZ


COPIED_UNIT = {
    "copy": copy.copy,
    "deepcopy": copy.deepcopy,
    "pickle": lambda unit: pickle.loads(pickle.dumps(unit)),  # noqa: S301  (our own bytes, just made)
}


@pytest.mark.parametrize("make", COPIED_UNIT.values(), ids=COPIED_UNIT.keys())
def test_a_copied_or_unpickled_unit_is_the_member_itself(make):
    assert make(Unit.MIDI) is Unit.MIDI


def test_units_hash_as_they_compare():
    found = {Unit.RATIO: "a", Unit.HERTZ: "b"}
    assert found[Series(rate=1, values=[1], unit="ratio").unit] == "a"
    assert len({Unit.RATIO, Unit.RATIO, Unit.HERTZ}) == 2


def test_a_unit_that_is_neither_a_member_nor_a_name_is_refused(reports):
    with pytest.raises(FormatError) as refused:
        Series(rate=1, values=[1], unit=3)  # type: ignore[arg-type]  # wrong on purpose: refused at run time
    reports(refused.value, "unit")


def _try_to_reinterpret(array):
    """Reassign the shape and the dtype of `array`; newer numpy may refuse, older only warns."""
    for name, new in (("shape", (2, 2)), ("dtype", np.int64)):
        with warnings.catch_warnings(), contextlib.suppress(AttributeError, ValueError, TypeError):
            warnings.simplefilter("ignore")
            setattr(array, name, new)


def test_reshaping_or_retyping_a_returned_array_changes_nothing_for_anyone_else():
    series = a_series([0.5, 1.5, 2.5, 3.5])
    again = a_series([0.5, 1.5, 2.5, 3.5])
    signal = from_series(series, interpolation="hold")
    before = signal.sample([0.0, 1.0, 2.0, 3.0]).tolist()

    _try_to_reinterpret(series.values)
    held = series.values
    _try_to_reinterpret(held)

    assert series.values.shape == (4,)
    assert series.values.dtype == np.float64
    assert series.values.tolist() == [0.5, 1.5, 2.5, 3.5]
    assert series == again
    assert series.to_dict() == again.to_dict()
    assert signal.sample([0.0, 1.0, 2.0, 3.0]).tolist() == before == [0.5, 1.5, 2.5, 3.5]
    assert from_series(series, interpolation="hold").sample([0.0, 1.0, 2.0, 3.0]).tolist() == before
