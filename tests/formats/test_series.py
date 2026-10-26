import pytest

from bullterrier_control import FormatError, Series, Unit

VALID = {"start": 1.5, "rate": 100, "values": [0.0, 0.25, 1], "unit": "fraction", "scaling": "peak"}


def test_a_valid_series_keeps_its_values():
    series = Series.parse(VALID)
    assert series.start == 1.5
    assert series.rate == 100.0
    assert series.values == (0.0, 0.25, 1.0)
    assert series.unit is Unit.FRACTION
    assert series.scaling == "peak"


def test_the_start_defaults_to_zero():
    assert Series.parse({"rate": 10, "values": [3], "unit": "hertz"}).start == 0.0


def test_a_list_and_a_tuple_give_the_same_series():
    assert Series.parse({**VALID, "values": (0.0, 0.25, 1)}) == Series.parse(VALID)


def test_decibels_need_no_scaling_and_may_be_negative():
    series = Series.parse({"rate": 50, "values": [-60, -3.5], "unit": "decibels"})
    assert series.values == (-60.0, -3.5)


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
    with pytest.raises(ValueError, match="frozen"):
        series.rate = 1


def test_a_series_built_directly_is_checked_like_a_parsed_one():
    with pytest.raises(FormatError, match="rate"):
        Series(rate=0, values=(1.0,), unit=Unit.RATIO)


def test_a_rule_across_fields_names_its_path_and_does_not_echo_the_whole_input():
    with pytest.raises(FormatError) as refused:
        Series.parse({**VALID, "values": [0.5, 1.5]})
    assert str(refused.value) == "Series: values[1]: 1.5 is outside 0..1 (unit 'fraction')"
