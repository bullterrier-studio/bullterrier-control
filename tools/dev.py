"""Developer commands for a source checkout, not part of `bt-control`.

    uv run poe new-source NAME --kind source     # or --kind shaper, or --kind operation

writes `crates/core/src/builtin/NAME.rs` and `tests/builtin/test_NAME.py` from `tools/templates/`: a working source (or
shaper, or operation) with one example parameter and example tests, which pass the gate as written. A source or shaper
also gets `examples/gallery/NAME.toml`, the patch its gallery picture is drawn from. There is no Python module: the Rust
file declares the parameters and defines the body, and the build script finds it and registers it in the core's table,
which the package's registry starts from. Replace the parameter, the body, the tests and the patch with the real ones
(CLAUDE.md, "Adding a source or shaper"), then run `uv run poe api`, which writes its typed function in `lib.pyi` (and a
shaper's method in `_chain.py`).

Interface: main(argv) -> exit code: 0 written, 1 refused (the reason on stderr), 2 wrong usage (argparse explains).
"""

import argparse
import json
import re
import sys
from pathlib import Path
from string import Template

ROOT = Path(__file__).resolve().parent.parent
TEMPLATES = Path(__file__).resolve().parent / "templates"


def new_source(name: str, kind: str, root: Path) -> list[Path]:
    """Write the Rust file, the test file and, for a source or shaper, the gallery patch of a new thing; return the
    paths.

    `ValueError` if refused: a name the core's patch schema refuses (as a thing's name, or as the name of the node
    its gallery patch gives it), or a file that exists already (nothing is written then)."""
    schema = json.loads((root / "schemas" / "patch-1.schema.json").read_text(encoding="utf-8"))
    (node_name,) = schema["patternProperties"]
    if not re.fullmatch(schema["$defs"]["node"]["properties"]["use"]["pattern"], name):
        raise ValueError(f"{name!r}: a name is lower case letters, digits and _, starting with a letter")
    if not re.fullmatch(node_name, name):
        raise ValueError(f"{name!r} is reserved: a patch cannot name a node so")
    made = [
        (f"test_{kind}.py.tmpl", root / "tests" / "builtin" / f"test_{name}.py"),
        (f"{kind}.rs.tmpl", root / "crates" / "core" / "src" / "builtin" / f"{name}.rs"),
    ]
    if kind != "operation":  # an operation gives events or a series: there is no signal to draw
        made.append((f"gallery_{kind}.toml.tmpl", root / "examples" / "gallery" / f"{name}.toml"))
    for _, path in made:
        if path.exists():
            raise ValueError(f"{path.relative_to(root).as_posix()} exists already")
    values = {"name": name, "title": "".join(part.title() for part in name.split("_"))}
    for template, path in made:
        text = Template((TEMPLATES / template).read_text(encoding="utf-8")).substitute(values)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
    return [path for _, path in made]


def main(argv: list[str] | None = None) -> int:
    """Run a developer command; see the module docstring."""
    parser = argparse.ArgumentParser(prog="dev.py", description="Developer commands for a source checkout.")
    commands = parser.add_subparsers(dest="command", required=True)
    scaffold = commands.add_parser(
        "new-source", help="write a new source, shaper or operation and its test from the templates"
    )
    scaffold.add_argument("name", help="the name it registers under, e.g. envelope")
    scaffold.add_argument("--kind", choices=["source", "shaper", "operation"], required=True)
    args = parser.parse_args(argv)
    try:
        written = new_source(args.name, args.kind, ROOT)
    except ValueError as error:
        print(f"new-source: {error}", file=sys.stderr)
        return 1
    for path in written:
        print(f"wrote {path.relative_to(ROOT).as_posix()}")
    gallery = "" if args.kind == "operation" else "`uv run poe gallery` and "
    print(
        f"next: fill the files in, then run {gallery}`uv run poe api` (it types `lib`, and a shaper's method of Signal)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
