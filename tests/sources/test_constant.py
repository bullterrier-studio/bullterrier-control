import pytest

from bullterrier_control import FormatError, registry

constant = registry.get("constant")


def test_a_constant_is_its_value_at_every_time():
    assert constant(value=0.75).sample([0.0, 1.5, 3600.0]).tolist() == [0.75, 0.75, 0.75]


def test_the_default_is_zero():
    assert constant().sample([0.0, 5.0]).tolist() == [0.0, 0.0]


def test_a_negative_value_is_fine():
    assert constant(value=-3).sample(2.0).tolist() == -3.0


def test_a_value_outside_the_documented_range_is_refused():
    with pytest.raises(FormatError, match="value"):
        constant(value=1e9)
