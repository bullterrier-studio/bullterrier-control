"""Command line: `bt-control validate`, `schema`, `list`, `sample` and `plot`.

A thin layer over the library; no logic of its own. `validate` loads each file as a Bundle (or, if it ends in `.toml`,
a Patch) and reports it; `schema` prints the JSON Schema of a Bundle or a Patch; `list` shows the registered sources,
shapers and operations, as text or as JSON; `sample` reads the signals of a patch at evenly spaced times and `plot`
draws them.
"""

import argparse
import csv
import json
import math
import sys
from collections.abc import Callable
from pathlib import Path

import numpy as np

from . import __version__
from .errors import BullterrierControlError, InvalidError, UnknownNameError
from .formats import Bundle, Series, Unit, bundle_schema, dumps, load
from .patch import load_patch, patch_schema
from .plotting import plot
from .registry import all_registered, get
from .rounding import TOLERANCE
from .signal import Signal

# a terminal or a pipe cannot use more rows than this, and a typo (`--rate 1e9`) should not fill the memory first
_MOST_SAMPLES = 10_000_000

_EXIT_CODES = """\
exit codes:
  0    success
  1    a file is not a valid bundle or patch, a name is unknown, or a file cannot be read ("error:" on stderr)
  2    wrong usage: an unknown command or option, or a range that makes no sense (argparse explains)

Errors go to stderr, results to stdout.
"""


def _seconds(text: str) -> float:
    """An argparse type: a finite number of seconds from 0 up."""
    value = float(text)
    if not math.isfinite(value) or value < 0:
        raise argparse.ArgumentTypeError(f"must be a finite number of seconds, 0 or more (got {text!r})")
    return value


def _hertz(text: str) -> float:
    """An argparse type: a finite number of samples a second, above 0."""
    value = float(text)
    if not math.isfinite(value) or value <= 0:
        raise argparse.ArgumentTypeError(f"must be a finite number above 0 (got {text!r})")
    return value


def main(argv: list[str] | None = None) -> int:
    """Run the command line with `argv` (default: `sys.argv[1:]`) and return the exit code.

    Results go to stdout, errors to stderr. Without arguments it prints the help. A failure the user can fix (a file
    that is not a valid bundle or patch or cannot be read, a name that is not registered) prints `error: <what is
    wrong>` and makes the exit code 1; `validate` still checks every file. See the exit codes in `--help`.
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
        added.set_defaults(handler=handler, usage_error=added.error)
        return added

    command("validate", _validate, "check that files are valid bundles or (.toml) patches").add_argument(
        "files", nargs="+", type=Path, metavar="FILE", help="a bundle (JSON) or a patch (TOML) file"
    )
    command("schema", _schema, "print the JSON Schema of a bundle or a patch").add_argument(
        "format", nargs="?", choices=["bundle", "patch"], default="bundle", help="which one (default: bundle)"
    )
    listing = command("list", _list, "list the sources, shapers and operations, or show some in full")
    listing.add_argument("names", nargs="*", metavar="NAME", help="show only these, with every parameter")
    listing.add_argument("--json", action="store_true", help="describe them as JSON, for another tool")
    sampling = _patch_command(command("sample", _sample, "read the signals of a patch at evenly spaced times"))
    sampling.add_argument("--rate", type=_hertz, required=True, metavar="HZ", help="samples a second")
    sampling.add_argument("--csv", action="store_true", help="write CSV (t, then a column per node), not a bundle")
    drawing = _patch_command(command("plot", _plot, "draw the signals of a patch as a PNG"))
    drawing.add_argument("--out", type=Path, required=True, metavar="FILE", help="the picture to write (PNG)")
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


def _patch_command(parser: argparse.ArgumentParser) -> argparse.ArgumentParser:
    """Add what `sample` and `plot` share: the patch, the nodes, the bundle and the range of time."""
    parser.add_argument("patch", type=Path, metavar="PATCH", help="a patch file (TOML)")
    parser.add_argument("nodes", nargs="*", metavar="NODE", help="the nodes to read (default: every signal)")
    parser.add_argument("--bundle", type=Path, metavar="FILE", help="the bundle its events, series and grid are in")
    parser.add_argument("--from", dest="start", type=_seconds, default=0.0, metavar="S", help="first time (default 0)")
    parser.add_argument("--to", dest="stop", type=_seconds, required=True, metavar="S", help="last time, in seconds")
    return parser


def _validate(args: argparse.Namespace) -> int:
    failed = False
    for path in args.files:
        try:
            load_patch(path) if path.suffix == ".toml" else load(path)
        # a file that cannot be read (OSError) or is not valid (BullterrierControlError) is the user's to fix;
        # the other files are still checked
        except (BullterrierControlError, OSError) as error:
            print(f"error: {error}", file=sys.stderr)
            failed = True
        else:
            print(f"{path}: ok")
    return 1 if failed else 0


def _schema(args: argparse.Namespace) -> int:
    schema = bundle_schema() if args.format == "bundle" else patch_schema()
    print(json.dumps(schema, indent=2, sort_keys=True))
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


def _signals(args: argparse.Namespace) -> dict[str, Signal]:
    """The signal nodes of the patch named by `args` (the ones asked for, else all), built against its bundle."""
    patch = load_patch(args.patch)
    built = patch.build(load(args.bundle) if args.bundle else None)
    names = args.nodes or [name for name, node in patch.nodes.items() if node.returns == "signal"]
    for name in names:
        if name not in patch.nodes:
            raise UnknownNameError(f"unknown node {name!r}; the patch has: {', '.join(patch.nodes) or 'none'}")
        if patch.nodes[name].returns != "signal":
            raise InvalidError(f"{name}: gives {patch.nodes[name].returns}, not a signal")
    return {name: built[name] for name in names}


def _times(args: argparse.Namespace) -> np.ndarray:
    """The times `--from` to `--to` at `--rate` a second; the end is the last one if it falls on a sample time to
    within `TOLERANCE`, else the last sample time before it."""
    if args.stop < args.start:
        args.usage_error(f"--to ({args.stop}) is before --from ({args.start})")
    intervals = (args.stop - args.start + TOLERANCE) * args.rate
    if intervals >= _MOST_SAMPLES:
        args.usage_error(
            f"that is {intervals:.3g} samples; at most {_MOST_SAMPLES} (use a lower --rate or a shorter range)"
        )
    return args.start + np.arange(int(np.floor(intervals)) + 1) / args.rate


def _sample(args: argparse.Namespace) -> int:
    times = _times(args)
    signals = _signals(args)
    values = {name: signal.sample(times) for name, signal in signals.items()}
    if args.csv:
        writer = csv.writer(sys.stdout, lineterminator="\n")
        writer.writerow(["t", *values])
        writer.writerows([float(t), *(float(column[i]) for column in values.values())] for i, t in enumerate(times))
        return 0
    series = {
        name: Series(start=args.start, rate=args.rate, values=tuple(column.tolist()), unit=Unit.RATIO)
        for name, column in values.items()
    }
    source = f"bt-control {__version__} sample {args.patch.name}"
    sys.stdout.write(dumps(Bundle(format="bullterrier-control/bundle", version=1, source=source, series=series)))
    return 0


def _plot(args: argparse.Namespace) -> int:
    if not args.stop > args.start:
        args.usage_error(f"--to ({args.stop}) must be after --from ({args.start})")
    plot(_signals(args), start=args.start, stop=args.stop, path=args.out, title=args.patch.name)
    print(args.out)
    return 0
