import tomllib
from pathlib import Path

import pytest

import bullterrier_control

ROOT = Path(__file__).resolve().parents[1]


@pytest.mark.parametrize("name", bullterrier_control.__all__)
def test_every_public_name_resolves(name):
    assert hasattr(bullterrier_control, name)


@pytest.mark.parametrize("name", ["from_values", "events"])
def test_the_one_call_helpers_are_public(name):
    assert name in bullterrier_control.__all__


def test_the_version_is_the_one_in_pyproject():
    project = tomllib.loads((ROOT / "pyproject.toml").read_text(encoding="utf-8"))["project"]
    assert bullterrier_control.__version__ == project["version"]
