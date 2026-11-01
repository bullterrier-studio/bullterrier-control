"""How a source, shaper or operation declares its parameters: a class whose every field is described, ranged and
defaulted. The declaration is a parameter table of the Rust core, which holds every rule, once.

    class WaveParams(Params):
        \"\"\"A repeating wave.\"\"\"

        depth: Finite = param(0.5, doc="How far the wave moves the value.", unit="fraction", min=0, max=1)

Promises:
- A field is annotated `Finite` (a decimal), `Whole`, `Switch`, `Text` (or plain `float`, `int`, `bool`, `str`) or a
  `Literal` of strings, and declared with `param`. The class is checked when it is made: a broken declaration (no
  description, a number without a unit or without a finite range that holds its default, a field that is not a number
  with one, a discouraged name or a unit's name, a cyclic field that is not one cycle in `cycles`, another type) raises
  `ParameterError` naming each field.
- A number is *cyclic* (`cyclic=True`) when the value one cycle further is the same position (a phase): any finite
  number is wrapped into its range (1.25 on 0..1 is 0.25); a value inside the range, either end included, is kept.
- `P(**values)` and `P.parse(mapping)` are the one way in, strict whatever the annotation: an unknown field, `true` or a
  string for a number, a float for a whole number, 1 for a switch, a value out of range or not finite is refused with
  a `ParameterError` listing every broken rule (`problems`: (field, rule) pairs); a field left out takes its default.
  The result is frozen: read-only attributes, `==`, `hash`, `repr` and `to_dict()`.
- `P.describe()` describes each field for docs, listings and tests, basic fields first, then the advanced ones
  (`advanced=True`: they tune the technique rather than choose the shape), each in declaration order: per field
  `type` or `enum`, `default`, `description`, `title`, `minimum` and `maximum`, the extras `unit`, `advanced` and
  `cyclic` (the JSON Schema properties bullterrier-image gives its parameters), and `range`, the range as help says it.
"""

import inspect
from collections.abc import Mapping
from dataclasses import asdict, dataclass
from typing import TYPE_CHECKING, Any, ClassVar, Literal, Self, get_args, get_origin

from ._native import _core

__all__ = ["Finite", "Params", "Switch", "Text", "Whole", "param"]

# What a field is annotated with. The core reads a value strictly whatever the annotation; these name the intent.
Finite = float  # a decimal: a whole number is fine, a bool, text, NaN or an infinity is not
Whole = int  # a whole number: not a bool, not a float
Switch = bool  # true or false: not 1
Text = str  # text that can be saved as UTF-8
_KINDS: dict[object, str] = {float: "float", int: "whole", bool: "switch", str: "text"}
_NOTHING = object()  # what a class body gave a field it only annotated


@dataclass(frozen=True)
class _Declared:
    """What `param` was given for one field."""

    default: Any
    doc: str
    unit: str | None
    min: float | None
    max: float | None
    advanced: bool
    cyclic: bool


def param(
    default: Any,
    *,
    doc: str,
    unit: str | None = None,
    min: float | None = None,  # noqa: A002  (the documented name of the range's lower bound)
    max: float | None = None,  # noqa: A002  (the documented name of the range's upper bound)
    advanced: bool = False,
    cyclic: bool = False,
) -> Any:
    """Declare one field of a `Params` class.

    default: the value used when none is given; for a number it lies in the range. doc: what the field does, its unit
    and range in words, and what raising it does. unit: required for a number (`seconds`, `beats`, `hertz`, `cycles`,
    `fraction`, `decibels`, `ratio`, `semitones`, `count`), not allowed otherwise. min, max: the inclusive range, both
    required for a number, not allowed otherwise. advanced: the field tunes the technique rather than the shape; it is
    listed after the basic ones. cyclic: a decimal in `cycles` whose range is one cycle wide and which wraps into it.
    """
    return _Declared(default, doc, unit, min, max, advanced, cyclic)


def _entry(name: str, annotation: object, declared: object) -> dict[str, object]:
    """The core's declaration of one field from its annotation and what the class body gave it (`param(...)`, a plain
    default, or `_NOTHING`). A type the core has no kind for is handed on by its text, for the core to refuse."""
    if get_origin(annotation) is Literal:
        entry: dict[str, object] = {"name": name, "kind": "choice", "choices": list(get_args(annotation))}
    else:
        entry = {"name": name, "kind": _KINDS.get(annotation, str(annotation))}
    if isinstance(declared, _Declared):
        return {**entry, **asdict(declared)}
    return entry if declared is _NOTHING else {**entry, "default": declared}


class Params:
    """The parameters of a source, shaper or operation: subclass it with a docstring, declare fields with `param`."""

    # the core's table of the class, made with it; the registry hands it to the core with the class
    _table: ClassVar[_core.ParamTable]

    def __init_subclass__(cls, **kwargs: Any) -> None:
        """Build the class's table from its annotations and `param` declarations (`ParameterError` naming each broken
        field)."""
        super().__init_subclass__(**kwargs)
        annotations = inspect.get_annotations(cls, eval_str=True)
        declared = {name: cls.__dict__.get(name, _NOTHING) for name in annotations}
        cls._table = _core.ParamTable(
            [_entry(name, annotation, declared[name]) for name, annotation in annotations.items()], name=cls.__name__
        )
        for name in annotations:  # the class holds no values: `P.depth` would be a declaration, not a number
            if name in cls.__dict__:
                delattr(cls, name)

    def __init__(self, **values: object) -> None:
        """The parameters for these values; see `parse`."""
        self.__dict__.update(self._table.parse(values))

    @classmethod
    def parse(cls, values: Mapping[str, object]) -> Self:
        """The parameters for `values` (a mapping of field to value), defaults filled in; or a `ParameterError` listing
        every broken rule."""
        params = cls.__new__(cls)
        params.__dict__.update(cls._table.parse(values))
        return params

    @classmethod
    def describe(cls) -> dict[str, dict[str, Any]]:
        """Each field's description by name, basic fields first, then advanced, each in declaration order (a copy)."""
        return cls._table.describe()

    def to_dict(self) -> dict[str, Any]:
        """Every field's value by name."""
        return dict(self.__dict__)

    def __setattr__(self, name: str, value: object) -> None:
        raise AttributeError(f"{type(self).__name__} is frozen: {name!r} cannot be set")

    def __delattr__(self, name: str) -> None:
        raise AttributeError(f"{type(self).__name__} is frozen: {name!r} cannot be deleted")

    def __eq__(self, other: object) -> bool:
        if type(other) is not type(self):
            return NotImplemented
        return self.__dict__ == other.__dict__

    def __hash__(self) -> int:
        return hash((type(self), tuple(self.__dict__.items())))

    def __repr__(self) -> str:
        return f"{type(self).__name__}({', '.join(f'{name}={value!r}' for name, value in self.__dict__.items())})"

    if TYPE_CHECKING:  # a built-in's fields are the core's, unknown to a type checker (R3 types them)

        def __getattr__(self, name: str) -> Any: ...
