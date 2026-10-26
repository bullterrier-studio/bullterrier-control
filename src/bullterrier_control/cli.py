"""Command line: `bt-control validate`, `schema` and `list`.

A thin layer over the library; no logic of its own. `validate` loads each file as a Bundle and reports it; `schema`
prints the JSON Schema of a Bundle; `list` shows the registered sources, shapers and operations, as text or as JSON.
"""

import argparse
import json
import sys
from collections.abc import Callable
from pathlib import Path

from . import __version__
from .errors import BullterrierControlError
from .formats import bundle_schema, load
from .registry import all_registered, get

_EXIT_CODES = """\
exit codes:
  0    success
  1    a file is not a valid bundle, a name is unknown, or a file cannot be read (reason after "error:" on stderr)
  2    wrong usage: an unknown command or option (argparse explains)

Errors go to stderr, results to stdout.
"""


def main(argv: list[str] | None = None) -> int:
    """Run the command line with `argv` (default: `sys.argv[1:]`) and return the exit code.

    Results go to stdout, errors to stderr. Without arguments it prints the help. A failure the user can fix (a file
    that is not a valid bundle or cannot be read, a name that is not registered) prints `error: <what is wrong>` and
    makes the exit code 1; `validate` still checks every file. See the exit codes in `--help`.
    """
    parser = argparse.ArgumentParser(
        prog="bt-control",
        description="Control signals: keyframes, LFOs, envelopes and shaping, read at any moment in seconds.",
        epilog=_EXIT_CODES,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("--version", action="version", version=f"bt-control {__version__}")
    parser.set_defaults(handler=None)
    commands = parser.add_subparsers(dest="command", metavar="COMMAND")

    def command(name: str, handler: Callable[[argparse.Namespace], int], summary: str) -> argparse.ArgumentParser:
        added = commands.add_parser(name, help=summary)
        added.set_defaults(handler=handler)
        return added

    command("validate", _validate, "check that files are valid bundles (docs/formats.md)").add_argument(
        "files", nargs="+", type=Path, metavar="FILE", help="a bundle file (JSON)"
    )
    command("schema", _schema, "print the JSON Schema of a bundle")
    listing = command("list", _list, "list the sources, shapers and operations, or show some in full")
    listing.add_argument("names", nargs="*", metavar="NAME", help="show only these, with every parameter")
    listing.add_argument("--json", action="store_true", help="describe them as JSON, for another tool")
    args = parser.parse_args(argv)
    if args.handler is None:
        parser.print_help()
        return 0
    try:
        return args.handler(args)
    # an unregistered name (BullterrierControlError) or a file that cannot be read (OSError) is the user's to fix
    except (BullterrierControlError, OSError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


def _validate(args: argparse.Namespace) -> int:
    failed = False
    for path in args.files:
        try:
            load(path)
        # a file that cannot be read (OSError) or is not a bundle (BullterrierControlError) is the user's to fix;
        # the other files are still checked
        except (BullterrierControlError, OSError) as error:
            print(f"error: {error}", file=sys.stderr)
            failed = True
        else:
            print(f"{path}: ok")
    return 1 if failed else 0


def _schema(args: argparse.Namespace) -> int:
    print(json.dumps(bundle_schema(), indent=2, sort_keys=True))
    return 0


def _list(args: argparse.Namespace) -> int:
    chosen = {name: get(name) for name in args.names} if args.names else all_registered()
    if args.json:
        print(json.dumps({name: found.description() for name, found in chosen.items()}, indent=2, sort_keys=True))
    elif args.names:
        print("\n\n".join(found.__doc__ or "" for found in chosen.values()))
    else:
        width = max(len(name) for name in chosen)
        kind_width = max(len(found.kind) for found in chosen.values())
        for name, found in chosen.items():
            print(f"{name:<{width}}  {found.kind:<{kind_width}}  {found.summary}")
    return 0
