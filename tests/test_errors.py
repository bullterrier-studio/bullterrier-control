import pytest

import bullterrier_control
from bullterrier_control import BullterrierControlError


def test_the_base_error_is_an_exception_a_caller_can_catch():
    with pytest.raises(BullterrierControlError, match="no signal"):
        raise BullterrierControlError("no signal")


def test_the_base_error_is_exported_from_the_package():
    assert bullterrier_control.BullterrierControlError is BullterrierControlError
