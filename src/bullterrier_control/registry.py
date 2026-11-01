"""The table of sources, shapers and operations, the rules for getting into it, and the queries over it.

    @source(params=LevelParams)
    def level(params: LevelParams) -> Signal: ...

    @shaper(params=ClipParams)
    def clip(signal: Signal, params: ClipParams) -> Signal: ...

    @operation(params=SelectParams)
    def select(params: SelectParams, *, events: tuple[Event, ...]) -> tuple[Event, ...]: ...

    registry.get("clip")(signal, low=0.2)          # a Signal; the values are checked by the parameter model

A source makes a signal from its parameters; a shaper makes a signal from a signal and its parameters; an operation
makes data from data (events or a series, as its return annotation says) and its parameters. Each may also declare
inputs (`bullterrier_control.inputs`: keyframes, ...) as keyword-only arguments after `params`: required without a
default, optional with `= None`; an operation takes its data that way. A call names them beside the parameters and each
arrives checked. All three share one namespace of lower case names, and every rule is checked when one registers, so a
broken one fails when its module is imported, naming itself, instead of in some patch much later. The table and those
rules are the Rust core's (`_native._core.Registry`); this module checks what is about a Python function's shape (its
signature, its inputs, an operation's return annotation) and loads the packages.

Where they come from: what the Rust core runs (`Registry::builtin()`) is in the table from the start;
the first query (`get`, `sources`, `shapers`, `operations`, `all_registered`, `table`, `unregister` or `isolated`) loads
every installed package of the entry-point group `bullterrier_control.sources`, once. A package is a module whose import
registers sources, shapers and operations: bullterrier-control's own is one (declared in its `pyproject.toml`); a third
party declares its own (see `bullterrier_control.plugin`). A package that fails to import raises
`BullterrierControlError` naming its entry point, whatever it registered before failing is taken back, and the next
query tries again. The registry names no package itself, so importing bullterrier-control loads none.

Threads: loading happens once, under a lock; threads that make the first query together wait for it and all see the
whole table. After that queries only read. Registering is for import time, and so is nothing else: a package must not
query the registry while its module is being imported (the same thread sees the table so far; another thread that
queries meanwhile can wait for the import lock the package holds).
"""

import inspect
import threading
from collections.abc import Callable, Iterator, Mapping
from contextlib import contextmanager
from dataclasses import dataclass, field
from importlib import metadata
from types import MappingProxyType
from typing import Any, Concatenate, Literal

from ._native import _core
from .errors import BullterrierControlError
from .formats import Event, Series
from .inputs import INPUTS
from .params import Params
from .signal import Signal, connect_shapers

__all__ = [
    "Registered",
    "all_registered",
    "get",
    "isolated",
    "operation",
    "operations",
    "shaper",
    "shapers",
    "source",
    "sources",
    "table",
    "unregister",
]

_GROUP = "bullterrier_control.sources"
_SIGNALS = {"source": 0, "shaper": 1, "operation": 0}  # how many signals each kind takes
type _Kind = Literal["source", "shaper", "operation"]
type _Returns = Literal["signal", "events", "series"]
# what an operation may return, by its return annotation, and what `help()` calls it
_OPERATION_RESULTS: Mapping[Any, _Returns] = MappingProxyType({tuple[Event, ...]: "events", Series: "series"})

Registered = _core.Thing
"""A registered source, shaper or operation: call it with its input signals (a shaper takes one), its inputs and its
keyword values; a source and a shaper give a `Signal`, an operation its data (a tuple of `Event` or a `Series`).

A name in `INPUTS` is an input, any other a parameter; a value left out takes its default, and `None` for an optional
input is leaving it out. An input it does not declare, or a required one that is missing, is a `TypeError`; an input
that breaks the rules of its kind or a value that breaks a parameter's is a `ParameterError` naming the thing (and the
field). It says what it is: `name`, `kind`, `returns` (`signal`, `events` or `series`), `summary`, `inputs` (each
input's name to whether it is required, in declaration order, read-only), `__doc__` (what `help()` shows),
`__signature__`, `description()`; and for one whose body is Python its `function` and `params_model` (`None` for a
built-in the Rust core runs)."""

# The table: a registry of the Rust core, seeded with the things the core runs (`slew`, `constant`, ...). `isolated`
# and a package that fails to load change what it holds, never which object it is.
_REGISTRY = _core.Registry(builtin=True)


@dataclass
class _Loading:
    """Whether the packages of the group are loaded (`done`) or being loaded right now (`running`, by the thread holding
    the lock: its own queries meanwhile see the table as it is), and the lock that makes one thread do it."""

    lock: threading.RLock = field(default_factory=threading.RLock)
    done: bool = False
    running: bool = False


_LOADING = _Loading()


def source[P: Params](*, params: type[P]) -> Callable[[Callable[Concatenate[P, ...], Signal]], Registered]:
    """Decorator: register `def name(params) -> Signal` as the source `name`, taking `params`."""
    return lambda function: _register(function, "source", params)


def shaper[P: Params](*, params: type[P]) -> Callable[[Callable[Concatenate[Signal, P, ...], Signal]], Registered]:
    """Decorator: register `def name(signal, params) -> Signal` as the shaper `name`, taking `params`."""
    return lambda function: _register(function, "shaper", params)


def operation[P: Params, R: (tuple[Event, ...], Series)](
    *, params: type[P]
) -> Callable[[Callable[Concatenate[P, ...], R]], Registered]:
    """Decorator: register `def name(params, *, <inputs>) -> tuple[Event, ...]` (or `-> Series`) as an operation.

    An operation makes data from data: its inputs are the data it takes (`events`, `series`, `grid`), and the return
    annotation, checked here, says whether it gives events or a series. It takes no signal."""
    return lambda function: _register(function, "operation", params)


def _register(function: Callable[..., Any], kind: _Kind, params: type[Params]) -> Registered:
    """`function` registered as a thing of `kind` taking `params`, once its Python shape is checked here; the rules of
    the name, the parameters' names and a name taken are the core's. Each refusal is a `ValueError` naming it."""
    name = function.__name__
    if not (isinstance(params, type) and issubclass(params, Params)):
        raise ValueError(f"{kind} {name!r}: params must be a Params subclass (got {params!r})")
    inputs = _declared_inputs(function, kind)
    returns: _Returns = _operation_result(function) if kind == "operation" else "signal"
    return _REGISTRY.register(function, kind=kind, params=params, inputs=inputs, returns=returns)


def _operation_result(function: Callable[..., Any]) -> _Returns:
    """What an operation returns, as `help()` names it: `events` or `series`; `ValueError` if its return annotation is
    anything else. The annotation is resolved, so a module with postponed annotations (`from __future__ import
    annotations`, where it is the text `"tuple[Event, ...]"`) registers as one without."""
    name = function.__name__
    try:
        returned = inspect.signature(function, eval_str=True).return_annotation
    except NameError as error:
        raise ValueError(f"operation {name!r}: cannot resolve the return annotation ({error})") from None
    if returned not in _OPERATION_RESULTS:
        found = "none" if returned is inspect.Signature.empty else repr(returned)
        raise ValueError(
            f"operation {name!r}: the return annotation must be tuple[Event, ...] or Series (found {found})"
        )
    return _OPERATION_RESULTS[returned]


def _declared_inputs(function: Callable[..., Any], kind: _Kind) -> Mapping[str, bool]:
    """The inputs `function` declares, as name to whether it is required; `ValueError` if it is not written
    `name(params)` or `name(signal, params)` (a shaper), optionally followed by `*, <input>, ...` with each input named
    in `INPUTS` and a default of `None` or none."""
    name = function.__name__
    arguments = list(inspect.signature(function).parameters.values())
    positional = [a for a in arguments if a.kind in (a.POSITIONAL_ONLY, a.POSITIONAL_OR_KEYWORD)]
    if len(positional) != _SIGNALS[kind] + 1:
        shape = "(signal, params)" if kind == "shaper" else "(params)"
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


def unregister(name: str) -> None:
    """Remove the source, shaper or operation `name` (`UnknownNameError` if none). For test throwaways."""
    _load_packages()
    _REGISTRY.unregister(name)


@contextmanager
def isolated() -> Iterator[None]:
    """Inside the block nothing is registered and the packages count as not loaded; afterwards the registry is as it
    was. Entering it first loads the packages (as a query does), so a package that fails to load raises
    `BullterrierControlError` at entry, before the block runs, and the registry is unchanged. For tests. A package
    already imported does not register again when loaded again, so use throwaway registrations or packages that are
    made new each time."""
    _load_packages()  # first, so the packages register now and are among what is given back, not lost inside the block
    with _LOADING.lock:
        saved, was_done = _REGISTRY.replace(_core.Registry(builtin=False)), _LOADING.done
        _LOADING.done = False
    try:
        yield
    finally:
        with _LOADING.lock:
            _REGISTRY.replace(saved)
            _LOADING.done = was_done


def get(name: str) -> Registered:
    """The source, shaper or operation called `name`; `UnknownNameError` listing the available names."""
    _load_packages()
    return _REGISTRY.get(name)


def _shaper_named(name: str) -> Registered:
    """The shaper `name`, which `Signal` finds shapers by (`get`'s error for an unknown name; `TypeError` if it is not a
    shaper)."""
    found = get(name)
    if found.kind != "shaper":
        raise TypeError(f"{name!r} is a {found.kind}, not a shaper")
    return found


connect_shapers(_shaper_named)


def all_registered() -> dict[str, Registered]:
    """Every source, shaper and operation by name, sorted by name (a copy)."""
    _load_packages()
    return _REGISTRY.things()


def sources() -> dict[str, Registered]:
    """The sources by name, sorted by name."""
    return {name: item for name, item in all_registered().items() if item.kind == "source"}


def shapers() -> dict[str, Registered]:
    """The shapers by name, sorted by name."""
    return {name: item for name, item in all_registered().items() if item.kind == "shaper"}


def operations() -> dict[str, Registered]:
    """The operations by name, sorted by name."""
    return {name: item for name, item in all_registered().items() if item.kind == "operation"}


def table() -> _core.Registry:
    """What a patch is read and saved against (`bullterrier_control.patch`): a copy of the core's registry every query
    reads, its packages loaded, as it holds things now. A change to the copy reaches no query: things register through
    `source`, `shaper` and `operation`, which check the function first."""
    _load_packages()
    copy = _core.Registry(builtin=False)
    copy.replace(_REGISTRY)
    return copy


def _load_packages() -> None:
    with _LOADING.lock:
        if _LOADING.done or _LOADING.running:
            return
        _LOADING.running = True
        try:
            for entry in sorted(metadata.entry_points(group=_GROUP), key=lambda found: found.name):
                before = _core.Registry(builtin=False)
                before.replace(_REGISTRY)  # what it holds now: a change to one leaves the other as it is
                try:
                    entry.load()
                # a package may raise anything while it is imported: take back what it registered, chain the cause
                except Exception as error:
                    _REGISTRY.replace(before)
                    raise BullterrierControlError(
                        f"cannot load the package {entry.value!r} (entry point {entry.name!r} of {_GROUP}): {error}"
                    ) from error
            _LOADING.done = True
        finally:
            _LOADING.running = False
