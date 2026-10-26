"""The table of sources and shapers, the rules for getting into it, and the queries over it.

    @source(params=LevelParams)
    def level(params: LevelParams) -> Signal: ...

    @shaper(params=ClipParams)
    def clip(signal: Signal, params: ClipParams) -> Signal: ...

    registry.get("clip")(signal, low=0.2)          # a Signal; the values are checked by the parameter model

A source makes a signal from its parameters; a shaper makes a signal from a signal and its parameters. Either may also
declare inputs (`bullterrier_control.inputs`: keyframes, ...) as keyword-only arguments after `params`: required without
a default, optional with `= None`. A call names them beside the parameters and each arrives checked. Both share one
namespace of lower case names, and every rule is checked when one registers, so a broken one fails when its module is
imported, naming itself, instead of in some patch much later.

Where they come from: the first query (`get`, `sources`, `shapers`, `all_registered`) loads every installed package of
the entry-point group `bullterrier_control.sources`, once. A package is a module whose import registers sources and
shapers: bullterrier-control's own is one (declared in its `pyproject.toml`); a third party declares its own (see
`bullterrier_control.plugin`). A package that fails to import raises `BullterrierControlError` naming its entry point,
whatever it registered before failing is taken back, and the next query tries again. The registry names no package
itself, so importing bullterrier-control loads none.

Threads: loading happens once, under a lock; threads that make the first query together wait for it and all see the
whole table. After that queries only read. Registering is for import time, and so is nothing else: a package must not
query the registry while its module is being imported (the same thread sees the table so far; another thread that
queries meanwhile can wait for the import lock the package holds).
"""

import inspect
import re
import threading
from collections.abc import Callable, Iterator, Mapping
from contextlib import contextmanager
from dataclasses import dataclass, field
from importlib import metadata
from types import MappingProxyType
from typing import Any, Concatenate, Literal

from .errors import BullterrierControlError, ParameterError, UnknownNameError
from .inputs import INPUTS
from .params import Params
from .signal import Signal

__all__ = ["Registered", "all_registered", "get", "isolated", "shaper", "shapers", "source", "sources", "unregister"]

_GROUP = "bullterrier_control.sources"
_NAME = re.compile(r"[a-z][a-z0-9_]*")
_SIGNALS = {"source": 0, "shaper": 1}  # how many signals each kind takes


class Registered[P: Params]:
    """A registered source or shaper: call it with its input signals (a shaper takes one) and keyword values.

    `inputs` maps each input the function declares to whether it is required, in declaration order."""

    def __init__(
        self,
        function: Callable[..., Signal],
        kind: Literal["source", "shaper"],
        params_model: type[P],
        inputs: Mapping[str, bool],
    ) -> None:
        """Wrap `function`; the registry does this, after checking every rule."""
        self.function, self.kind, self.params_model, self.inputs = function, kind, params_model, inputs
        self.name = function.__name__
        first_paragraph = (inspect.getdoc(function) or "").split("\n\n")[0]
        self.summary = " ".join(first_paragraph.split())
        self.__doc__ = _documentation(self, inspect.getdoc(function) or "")

    def __call__(self, *signals: Signal, **values: object) -> Signal:
        """The signal for these input signals, inputs and parameter values; unset parameters take their defaults.

        A name in `INPUTS` is an input, any other a parameter. An input the function does not declare, or a required
        one that is missing, is a `TypeError`; one that breaks the rules of its kind is a `ParameterError`. `None` for
        an optional input is the same as leaving it out."""
        wanted = _SIGNALS[self.kind]
        if len(signals) != wanted or not all(isinstance(signal, Signal) for signal in signals):
            raise TypeError(f"{self.kind} {self.name!r} takes {wanted} signal(s), not {len(signals)}")
        undeclared = sorted(values.keys() & (INPUTS.keys() - self.inputs.keys()))
        if undeclared:
            raise TypeError(f"{self.kind} {self.name!r} takes no input {undeclared[0]!r}")
        given: dict[str, object] = {}
        for name, required in self.inputs.items():
            if name in values and (required or values[name] is not None):
                given[name] = self._named(INPUTS[name].check, values[name])
            elif required:
                raise TypeError(f"{self.kind} {self.name!r} needs the input {name!r}")
        parameters = self.validate({name: value for name, value in values.items() if name not in INPUTS})
        return self.function(*signals, parameters, **given)

    def validate(self, values: Mapping[str, object]) -> P:
        """The parameters for `values` (name to value), or a `ParameterError` naming this thing and each problem."""
        return self._named(self.params_model.parse, values)

    def _named[V, R](self, check: Callable[[V], R], value: V) -> R:
        """`check(value)`, with a `ParameterError` it raises saying which source or shaper refused."""
        try:
            return check(value)
        except ParameterError as error:
            raise ParameterError(f"{self.kind} {self.name!r}: {error}", error.problems) from None


def _documentation(registered: "Registered[Any]", described: str) -> str:
    """What `help()` shows: how to call it, the function's own description, its inputs, then every parameter in its own
    terms."""
    takes = ["signal"] if registered.kind == "shaper" else []
    if registered.inputs:
        takes += ["*", *(name if required else f"{name}=None" for name, required in registered.inputs.items())]
    lines = [f"{registered.kind} {registered.name}({', '.join([*takes, '**values'])}) -> Signal", "", described, ""]
    if registered.inputs:
        lines.append("Inputs:")
        for name, required in registered.inputs.items():
            lines += [f"  {name} ({'required' if required else 'optional'})", f"      {INPUTS[name].doc}"]
        lines.append("")
    fields = registered.params_model.fields_doc()
    if fields:
        lines.append("Parameters:")
    for one in fields:
        measure = f" ({one.unit}, {one.minimum:g} to {one.maximum:g})" if one.unit else ""
        lines.append(f"  {one.name}{measure}, default {one.default!r}{', advanced' if one.advanced else ''}")
        lines.append(f"      {one.doc}")
    return "\n".join(lines).rstrip()


_TABLE: dict[str, Registered[Any]] = {}


@dataclass
class _Loading:
    """Whether the packages of the group are loaded (`done`) or being loaded right now (`running`, by the thread holding
    the lock: its own queries meanwhile see the table as it is), and the lock that makes one thread do it."""

    lock: threading.RLock = field(default_factory=threading.RLock)
    done: bool = False
    running: bool = False


_LOADING = _Loading()


def source[P: Params](*, params: type[P]) -> Callable[[Callable[Concatenate[P, ...], Signal]], Registered[P]]:
    """Decorator: register `def name(params) -> Signal` as the source `name`, taking `params`."""
    return lambda function: _register(function, "source", params)


def shaper[P: Params](*, params: type[P]) -> Callable[[Callable[Concatenate[Signal, P, ...], Signal]], Registered[P]]:
    """Decorator: register `def name(signal, params) -> Signal` as the shaper `name`, taking `params`."""
    return lambda function: _register(function, "shaper", params)


def _register[P: Params](
    function: Callable[..., Signal], kind: Literal["source", "shaper"], params: type[P]
) -> Registered[P]:
    name = function.__name__
    if not (isinstance(params, type) and issubclass(params, Params)):
        raise ValueError(f"{kind} {name!r}: params must be a Params subclass (got {params!r})")
    if not _NAME.fullmatch(name):
        raise ValueError(
            f"{kind} {name!r}: a name is lower case letters, digits and underscores, starting with a letter"
        )
    clashing = sorted(INPUTS.keys() & params.model_fields.keys())
    if clashing:
        raise ValueError(f"{kind} {name!r}: the parameter {clashing[0]!r} has the name of an input")
    inputs = _declared_inputs(function, kind)
    existing = _TABLE.get(name)
    if existing is not None and not _same_function(existing, function, kind):
        raise ValueError(
            f"{kind} {name!r}: the name is taken by the {existing.kind} defined as"
            f" {existing.function.__module__}.{existing.function.__qualname__}"
        )
    registered = Registered(function, kind, params, inputs)
    _TABLE[name] = registered
    return registered


def _declared_inputs(function: Callable[..., Signal], kind: Literal["source", "shaper"]) -> Mapping[str, bool]:
    """The inputs `function` declares, as name to whether it is required; `ValueError` if it is not written
    `name(params)` or `name(signal, params)`, optionally followed by `*, <input>, ...` with each input named in `INPUTS`
    and a default of `None` or none."""
    name = function.__name__
    arguments = list(inspect.signature(function).parameters.values())
    positional = [a for a in arguments if a.kind in (a.POSITIONAL_ONLY, a.POSITIONAL_OR_KEYWORD)]
    if len(positional) != _SIGNALS[kind] + 1:
        shape = "(params)" if kind == "source" else "(signal, params)"
        extra = positional[_SIGNALS[kind] + 1 :]
        hint = f"; {extra[0].name!r} is positional, and an input is keyword-only (after `*`)" if extra else ""
        raise ValueError(f"{kind} {name!r}: the function must be written {name}{shape}{hint}")
    inputs = {}
    for argument in arguments[len(positional) :]:
        if argument.kind is not argument.KEYWORD_ONLY or argument.name not in INPUTS:
            raise ValueError(f"{kind} {name!r}: {argument.name!r} is not an input; inputs are {', '.join(INPUTS)}")
        if argument.default is not argument.empty and argument.default is not None:
            raise ValueError(f"{kind} {name!r}: the input {argument.name!r} takes no default, or None to be optional")
        inputs[argument.name] = argument.default is argument.empty
    return MappingProxyType(inputs)


def _same_function(existing: Registered[Any], function: Callable[..., Signal], kind: str) -> bool:
    """Whether `function` is what is registered under its name, defined again as reloading its module does. Only a
    function defined at the top of a module can be: two made by one factory share a name and are different things."""
    old = existing.function
    if old is function:
        return existing.kind == kind
    top_level = "<locals>" not in function.__qualname__
    return (
        top_level
        and existing.kind == kind
        and (old.__module__, old.__qualname__) == (function.__module__, function.__qualname__)
    )


def unregister(name: str) -> None:
    """Remove the source or shaper `name` (`UnknownNameError` if there is none). For tests that register throwaways."""
    get(name)
    del _TABLE[name]


@contextmanager
def isolated() -> Iterator[None]:
    """Inside the block nothing is registered and the packages count as not loaded; afterwards the registry is as it
    was. For tests. A package already imported does not register again when loaded again, so use throwaway
    registrations or packages that are made new each time."""
    with _LOADING.lock:
        saved, was_done = dict(_TABLE), _LOADING.done
        _TABLE.clear()
        _LOADING.done = False
    try:
        yield
    finally:
        with _LOADING.lock:
            _TABLE.clear()
            _TABLE.update(saved)
            _LOADING.done = was_done


def get(name: str) -> Registered[Any]:
    """The source or shaper called `name`; `UnknownNameError` listing the available names if there is none."""
    _load_packages()
    try:
        return _TABLE[name]
    except KeyError:
        raise UnknownNameError(f"unknown source or shaper {name!r}; available: {', '.join(sorted(_TABLE))}") from None


def all_registered() -> dict[str, Registered[Any]]:
    """Every source and shaper by name, sorted by name (a copy)."""
    _load_packages()
    return {name: _TABLE[name] for name in sorted(_TABLE)}


def sources() -> dict[str, Registered[Any]]:
    """The sources by name, sorted by name."""
    return {name: item for name, item in all_registered().items() if item.kind == "source"}


def shapers() -> dict[str, Registered[Any]]:
    """The shapers by name, sorted by name."""
    return {name: item for name, item in all_registered().items() if item.kind == "shaper"}


def _load_packages() -> None:
    with _LOADING.lock:
        if _LOADING.done or _LOADING.running:
            return
        _LOADING.running = True
        try:
            for entry in sorted(metadata.entry_points(group=_GROUP), key=lambda found: found.name):
                before = dict(_TABLE)
                try:
                    entry.load()
                # a package may raise anything while it is imported: take back what it registered, chain the cause
                except Exception as error:
                    _TABLE.clear()
                    _TABLE.update(before)
                    raise BullterrierControlError(
                        f"cannot load the package {entry.value!r} (entry point {entry.name!r} of {_GROUP}): {error}"
                    ) from error
            _LOADING.done = True
        finally:
            _LOADING.running = False
