"""Developer commands for a source checkout, not part of `bt-control`.

    uv run poe new-source NAME --kind source     # or --kind shaper

writes `src/bullterrier_control/builtin/NAME.py` and `tests/builtin/test_NAME.py` from `tools/templates/`: a working
source (or shaper) with one example parameter and one example test, which pass the gate as written. Replace the
parameter, the body and the test with the real ones (CLAUDE.md, "Adding a source or shaper").

Interface: main(argv) -> exit code: 0 written, 1 refused (the reason on stderr), 2 wrong usage (argparse explains).
"""

import argparse
import re
import sys
from pathlib import Path
from string import Template

ROOT = Path(__file__).resolve().parent.parent
TEMPLATES = Path(__file__).resolve().parent / "templates"
NAME = re.compile(r"[a-z][a-z0-9_]*")


def new_source(name: str, kind: str, root: Path) -> list[Path]:
    """Write the module and the test file of a new source or shaper; return their paths. ValueError if refused."""
    if not NAME.fullmatch(name):
        raise ValueError(f"{name!r}: a name is lower case letters, digits and _, starting with a letter")
    module = root / "src" / "bullterrier_control" / "builtin" / f"{name}.py"
    test = root / "tests" / "builtin" / f"test_{name}.py"
    for path in (module, test):
        if path.exists():
            raise ValueError(f"{path.relative_to(root)} exists already")
    values = {"name": name, "title": "".join(part.title() for part in name.split("_"))}
    for template, path in ((f"{kind}.py.tmpl", module), (f"test_{kind}.py.tmpl", test)):
        text = Template((TEMPLATES / template).read_text(encoding="utf-8")).substitute(values)
        path.write_text(text, encoding="utf-8")
    return [module, test]


def main(argv: list[str] | None = None) -> int:
    """Run a developer command; see the module docstring."""
    parser = argparse.ArgumentParser(prog="dev.py", description="Developer commands for a source checkout.")
    commands = parser.add_subparsers(dest="command", required=True)
    scaffold = commands.add_parser("new-source", help="write a new source or shaper and its test from the templates")
    scaffold.add_argument("name", help="the name it registers under, e.g. envelope")
    scaffold.add_argument("--kind", choices=["source", "shaper"], required=True)
    args = parser.parse_args(argv)
    try:
        written = new_source(args.name, args.kind, ROOT)
    except ValueError as error:
        print(f"new-source: {error}", file=sys.stderr)
        return 1
    for path in written:
        print(f"wrote {path.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
