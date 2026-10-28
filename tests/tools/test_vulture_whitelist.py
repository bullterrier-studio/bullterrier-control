"""tools/vulture_whitelist.py: the fields of the parameter models of the shapers the Rust core implements, and only
those, as a vulture whitelist.

An architecture test of the tooling: only the core can say which shapers it implements, so it asks the extension.
"""

import importlib.util
from pathlib import Path

from bullterrier_control import registry
from bullterrier_control._native import _core  # noqa: PLC2701  architecture: the shapers the Rust core implements
from bullterrier_control.plugin import Finite, Params, param

ROOT = Path(__file__).resolve().parents[2]


def _load_tool():
    spec = importlib.util.spec_from_file_location("vulture_whitelist", ROOT / "tools" / "vulture_whitelist.py")
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


vulture_whitelist = _load_tool()


class ProbeParams(Params):
    """Two fields, as a shaper's model declares them."""

    zz_attack: Finite = param(0.1, doc="An attack.", unit="seconds", min=0, max=1)
    zz_depth: Finite = param(0.5, doc="A depth.", unit="fraction", min=0, max=1)


def test_the_whitelist_names_every_field_of_each_model_given():
    source = vulture_whitelist.whitelist([ProbeParams])
    compile(source, "whitelist.py", "exec")  # vulture reads it as Python
    assert {line.split()[0] for line in source.splitlines()} == {"_.zz_attack", "_.zz_depth"}


def test_the_written_whitelist_names_the_fields_of_the_rust_shapers_and_of_no_other(tmp_path):
    target = tmp_path / "whitelist.py"
    assert vulture_whitelist.main([str(target)]) == 0
    written = {line.split()[0].removeprefix("_.") for line in target.read_text(encoding="utf-8").splitlines()}
    assert written == {field for name in _core.shapers() for field in registry.get(name).params_model.model_fields}
