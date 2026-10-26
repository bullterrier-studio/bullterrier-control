"""Command line: `bt-control validate FILE...` and `bt-control schema`.

A thin layer over the library; no logic of its own. `validate` loads each file as a Bundle and reports it; `schema`
prints the JSON Schema of a Bundle.
"""

import argparse
import json
import sys
from pathlib import Path

from . import __version__
from .errors import BullterrierControlError
from .formats import bundle_schema, load

_EXIT_CODES = """\
exit codes:
  0    success
  1    a file is not a valid bundle, or cannot be read (reason after "error:" on stderr)
  2    wrong usage: an unknown command or option (argparse explains)

Errors go to stderr, results to stdout.
"""


def main(argv: list[str] | None = None) -> int:
    """Run the command line with `argv` (default: `sys.argv[1:]`) and return the exit code.

    Results go to stdout, errors to stderr. Without arguments it prints the help. A file that is not a valid bundle, or
    cannot be read, prints `error: <path>: <what is wrong>` and makes the exit code 1; every file is still checked. See
    the exit codes in `--help`.
    """
    parser = argparse.ArgumentParser(
        prog="bt-control",
        description="Control signals: keyframes, LFOs, envelopes and shaping, read at any moment in seconds.",
        epilog=_EXIT_CODES,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("--version", action="version", version=f"bt-control {__version__}")
    commands = parser.add_subparsers(dest="command", metavar="COMMAND")
    validate = commands.add_parser("validate", help="check that files are valid bundles (docs/formats.md)")
    validate.add_argument("files", nargs="+", type=Path, metavar="FILE", help="a bundle file (JSON)")
    commands.add_parser("schema", help="print the JSON Schema of a bundle")
    args = parser.parse_args(argv)
    if args.command == "validate":
        return _validate(args.files)
    if args.command == "schema":
        print(json.dumps(bundle_schema(), indent=2, sort_keys=True))
        return 0
    parser.print_help()
    return 0


def _validate(files: list[Path]) -> int:
    failed = False
    for path in files:
        try:
            load(path)
        # a file that cannot be read (OSError) or is not a bundle (BullterrierControlError) is the user's to fix
        except (BullterrierControlError, OSError) as error:
            print(f"error: {error}", file=sys.stderr)
            failed = True
        else:
            print(f"{path}: ok")
    return 1 if failed else 0
