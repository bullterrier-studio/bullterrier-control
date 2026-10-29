"""A Bundle's file (docs/formats.md): `load` and `save`.

    bundle = load(Path("analysis.json"))       # a Bundle, or a FormatError naming the file and what is wrong
    save(bundle, Path("analysis.json"))        # UTF-8 JSON, one object, ending in a newline

Promises:
- `load` accepts exactly what the format allows: valid UTF-8 JSON whose top level is an object, with no repeated key and
  no `NaN` or infinity, and every rule of the format met. Anything else is one `FormatError` whose message starts with
  the path and whose `problems` are the bundle's. A file that cannot be read at all raises the `OSError` the system
  gave (a missing file stays a `FileNotFoundError`).
- `save` writes what `load` reads back as an equal bundle, leaving out every optional field that is absent.
"""

from pathlib import Path

from .._native._core import Bundle, dumps
from ..errors import FormatError

__all__ = ["load", "read_text", "save"]


def read_text(path: Path) -> str:
    """The UTF-8 text of the file at `path`; a `FormatError` starting with the path if it is not UTF-8, the `OSError`
    the system gave if it cannot be read at all."""
    try:
        return path.read_text(encoding="utf-8")
    except UnicodeDecodeError as error:
        raise FormatError(f"{path}: not UTF-8 text ({error.reason})") from None


def load(path: Path) -> Bundle:
    """The Bundle in the JSON file at `path`, or a `FormatError` starting with the path when the file is not valid."""
    text = read_text(path)
    try:
        return Bundle.from_json(text)
    except FormatError as error:
        raise FormatError(f"{path}: {error}", error.problems) from None


def save(bundle: Bundle, path: Path) -> None:
    """Write `bundle` to `path` as UTF-8 JSON, replacing the file; absent optional fields are left out."""
    path.write_text(dumps(bundle), encoding="utf-8")
