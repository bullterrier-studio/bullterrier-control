"""The package has no pydantic: the Rust core checks every input, number and field.

An architecture test: its subject is the package's imports and its declared dependencies, so it reads the source files
(with `ast`) and `pyproject.toml`.
"""

import ast
import re
import tomllib
from pathlib import Path

import pytest

ROOT = Path(__file__).parent.parent


def importing_pydantic(tree: Path) -> list[str]:
    """Each import of `pydantic` (or one of its submodules) in the Python files under `tree`, as 'file:line'."""
    found = []
    for path in sorted(tree.rglob("*.py")):
        for node in ast.walk(ast.parse(path.read_text(encoding="utf-8"))):
            if isinstance(node, ast.Import):
                modules = [alias.name for alias in node.names]
            elif isinstance(node, ast.ImportFrom) and node.level == 0:  # a relative import is the package's own
                modules = [node.module or ""]
            else:
                continue
            if any(module.split(".")[0] == "pydantic" for module in modules):
                found.append(f"{path.relative_to(tree)}:{node.lineno}")
    return found


def requirements_naming_pydantic(pyproject: dict) -> list[str]:
    """Each requirement of the project (runtime, optional or development) that asks for pydantic."""
    project = pyproject["project"]
    requirements = [
        *project.get("dependencies", []),
        *(r for group in project.get("optional-dependencies", {}).values() for r in group),
        *(r for group in pyproject.get("dependency-groups", {}).values() for r in group if isinstance(r, str)),
    ]
    return [r for r in requirements if re.match(r"\s*pydantic([\s<>=!~;\[]|$)", r, re.IGNORECASE)]


GOOD_TREES = {
    "no imports": "x = 1\n",
    "a name that only contains it": "import pydantic_settings_like\nfrom mypydantic import model\n",
    "a relative import of a module of that name": "from .pydantic import thing\n",
    "pydantic in a comment or a string": "# pydantic\nTEXT = 'import pydantic'\n",
}
BAD_TREES = {
    "import": "import pydantic\n",
    "from import": "from pydantic import BaseModel\n",
    "a submodule": "import pydantic.fields\n",
    "from a submodule": "from pydantic.functional_validators import AfterValidator\n",
    "an alias beside others": "import os, pydantic as pd\n",
    "inside a function": "def f():\n    from pydantic import Field\n",
}


@pytest.mark.parametrize("source", GOOD_TREES.values(), ids=GOOD_TREES.keys())
def test_the_check_passes_a_tree_without_pydantic(source, tmp_path):
    (tmp_path / "inner").mkdir()
    (tmp_path / "inner" / "module.py").write_text(source, encoding="utf-8")
    assert importing_pydantic(tmp_path) == []


@pytest.mark.parametrize("source", BAD_TREES.values(), ids=BAD_TREES.keys())
def test_the_check_finds_a_tree_that_imports_pydantic(source, tmp_path):
    (tmp_path / "inner").mkdir()
    (tmp_path / "inner" / "module.py").write_text(source, encoding="utf-8")
    assert len(importing_pydantic(tmp_path)) == 1


def test_no_file_of_the_package_imports_pydantic():
    assert importing_pydantic(ROOT / "src") == []


GOOD_REQUIREMENTS = {
    "numpy only": {"project": {"dependencies": ["numpy>=1.26.4"]}},
    "a name that only contains it": {"project": {"dependencies": ["pydantic-core-like>=1"]}},
    "nothing declared": {"project": {}},
}
BAD_REQUIREMENTS = {
    "a runtime dependency": {"project": {"dependencies": ["numpy", "pydantic>=2.7.1"]}},
    "bare name": {"project": {"dependencies": ["Pydantic"]}},
    "an extra of it": {"project": {"dependencies": ["pydantic[email]>=2"]}},
    "an optional dependency": {"project": {"optional-dependencies": {"x": ["pydantic==2.0"]}}},
    "a development dependency": {"project": {}, "dependency-groups": {"dev": ["pytest", "pydantic>=2"]}},
}


@pytest.mark.parametrize("pyproject", GOOD_REQUIREMENTS.values(), ids=GOOD_REQUIREMENTS.keys())
def test_the_check_passes_requirements_without_pydantic(pyproject):
    assert requirements_naming_pydantic(pyproject) == []


@pytest.mark.parametrize("pyproject", BAD_REQUIREMENTS.values(), ids=BAD_REQUIREMENTS.keys())
def test_the_check_finds_a_requirement_for_pydantic(pyproject):
    assert len(requirements_naming_pydantic(pyproject)) == 1


def test_the_project_does_not_depend_on_pydantic():
    pyproject = tomllib.loads((ROOT / "pyproject.toml").read_text(encoding="utf-8"))
    assert requirements_naming_pydantic(pyproject) == []
