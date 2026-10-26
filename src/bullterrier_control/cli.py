"""Command line: `bt-control`.

A thin layer over the library; no logic of its own. There are no commands yet: `--version` and `--help` answer, and any
other input is a usage error.
"""

import argparse

from . import __version__

_EXIT_CODES = """\
exit codes:
  0    success
  2    wrong usage: an unknown command or option (argparse explains)

Errors go to stderr, results to stdout.
"""


def main(argv: list[str] | None = None) -> int:
    """Run the command line with `argv` (default: `sys.argv[1:]`) and return the exit code.

    Results go to stdout, errors to stderr. Without arguments it prints the help. See the exit codes in `--help`.
    """
    parser = argparse.ArgumentParser(
        prog="bt-control",
        description="Control signals: keyframes, LFOs, envelopes and shaping, read at any moment in seconds.",
        epilog=_EXIT_CODES,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("--version", action="version", version=f"bt-control {__version__}")
    parser.parse_args(argv)
    parser.print_help()
    return 0
