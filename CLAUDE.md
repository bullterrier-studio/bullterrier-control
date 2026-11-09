# CLAUDE.md: bullterrier-control

Python library and CLI that produces **control signals**: numbers that change over time and drive the parameters of anything. It is
what DAWs call automation, animation tools keyframes and synthesizers modulation. `README.md` (usage), `docs/ecosystem.md` (the
five packages and their rules), `docs/design-decisions.md` (why), `docs/roadmap.md` (what is next), `docs/formats.md` (the files).

**Write docs tersely.** This file, the README, `docs/`, docstrings, reports and briefs: state the rule or fact, drop preamble,
repetition and justification a reader does not need to act. Reasons go in `docs/design-decisions.md`, once. Committed files are
public: no absolute paths, no references to folders outside the repository.

## Start here

```bash
uv sync && uv run poe setup          # once per clone: environment (builds the Rust extension: needs rustup), git hooks, commit template, cargo tools
uv run pytest tests/builtin -n 0     # the tests near your change, while you work
uv run poe fast                      # the inner loop, as often as you like: format, lint, types, quick tests
uv run poe check                     # the gate, before you report or merge (then `uv run lint-imports`)
uv run poe full                      # `check` and the gate proofs: before a tag (`poe release` runs it) and in CI
```

| To ... | Look at |
|---|---|
| use a source, shaper or operation | `docs/api.md` (the code-first API); shapers are methods (`energy.slew(rise=0.2).clip(high=0.8)`), all three kinds are in `lib` (`lib.slew(signal, rise=0.2)`; `dir(lib)`, `help(lib.slew)`) |
| add a source, shaper or operation | "Adding a source or shaper" below; one Rust file in `crates/core/src/builtin/`, one test file in `tests/builtin/` |
| write a shaper's recurrence in Rust | "Rust code" below; `crates/core/src/builtin/slew.rs` |
| see what every source and shaper promises | "The signal contract" below; `tests/builtin/test_contract.py` checks it over all of them |
| declare parameters | a built-in's: the `Params` struct in `crates/core/src/builtin/<name>.rs`; a plugin's: `params.py` (`Params`, `param`); the rules in "Parameters" below |
| write a signal that needs history | `rendered.py` (`Rendered`); in Rust `Signal::rendered` |
| read or write a file | `formats/` (`load`, `save`, `Bundle`); `docs/formats.md` |
| write, read or save a patch (sources, shapers and operations as data) | `patch.py` (`Patch`, `load_patch`; the rules are `crates/core/src/patch.rs`); `docs/formats.md`, "Patch"; `examples/glow.toml` |
| use the core from a browser or Node | `crates/wasm/src/lib.rs`, `tests/wasm/`; README, "In the browser and Node"; `uv run poe wasm-test` builds `pkg/` and compares it with the native extension |
| see what a source or shaper looks like | `docs/gallery.md`; `uv run poe gallery` redraws it from `examples/gallery/` |
| see a worked example with numbers | `docs/examples.md` and `examples/` |
| find what is planned | `docs/roadmap.md`: work goes in small batches, each planned before it starts |

**State:** the formats, the core (signals, `Rendered`, parameters, registry, `lib`), the code-first API (`from_values`, `events`, shapers as typed methods and `Signal.pipe`; `docs/api.md`), inputs (`points`, `events`, `grid`, `values`, `series`), easing, cadence, hashing and scaling, and
the sources `constant`, `keyframes`, `envelope`, `latch`, `lfo`, `steps` and `noise`, the shapers `clip`, `slew`, `follow`,
`gate`, `hold`, `remap`, `delay`, `repeat`, `integrate` and `spring`, the operations `select`, `every`, `thin`, `shift`, `beats`, `quantize` and `normalise`,
patch files, the command line (`validate`, `schema`, `list`, `sample`, `plot`) and the gallery exist. The Rust core's foundation
(R0) exists: `slew` and `follow` run in Rust, and CI builds the core and a wheel per platform (R0b; first GitHub run pending).
The formats are in Rust (R1): `Series`, `Event`, `Grid`, `Bundle` and `Unit` are classes of the extension. Parameters are in Rust (R2): every built-in declares them there (`#[derive(Params)]`), and the core checks every parameter, a plugin's too. The signal graph is in Rust (R3): `Signal`, arithmetic, `Rendered`, `slew` and `follow` (as Rust `Signal` shapers) and easing, cadence, hashing, scaling and rounding run in the core, and Python's `Signal` is a handle over it. The inputs are in Rust (R4a): the core's `inputs` module checks every input, `Keyframe` is a class of the extension, and pydantic is gone. The registry and patches are in Rust (R4b): the core's `registry` module holds every source, shaper and operation (Rust things register themselves, Python ones as callbacks), a patch is read, built and saved by the core with no Python, and a graph made through the registry saves as a patch (`Patch.from_signals`). Built-ins with data are in Rust (R5a): every kind takes `type Inputs` and `Signal::retimed` moves time. All 21 built-ins run in Rust (R5b): `Signal::mapped` maps values, and a source's `lib::` function takes a struct of several inputs. The Python built-ins are gone (R5c): no `builtin` package or entry point, and the lists of built-ins are derived from the files. The crate is documented and the Python helpers nothing used are gone (R6). Next: C5, the first release (`docs/roadmap.md`).
Maintainers may keep machine-specific notes in a git-ignored `.local/` folder (for example `.local/prior-work.md`, earlier
experiments worth reusing) and batch plans in the git-ignored `.superpowers/plans/`: read them when present; never commit or
cite them.

## Rules

@docs/rules.md

## Responsibility

`bullterrier-control` is the **control package** of the Bullterrier ecosystem (GitHub: `bullterrier-studio/bullterrier-control`).
It answers "what is this value at 12.4 s?". Siblings: `audio` (measures sound), `image`, `vector` (draw), `video` (decides what
happens when, maps signals onto parameters, renders frames). Image, vector, audio and control never depend on each other; only
video knows the others.

| Responsible for | Not responsible for |
|---|---|
| What a control signal and an event are, for the whole ecosystem | Detecting beats, onsets or loudness in sound → audio |
| Modulation sources: keyframes with easing, automation, LFOs, envelopes, step sequencers, seeded noise | Knowing what a signal drives (an effect parameter, a light) → the consumer |
| Shaping: slew, attack and release smoothing, gates, sample and hold, quantizing to a beat grid, range mapping, combining signals | Routing a signal to a parameter → video |
| Reading any signal at any moment, in seconds | Frames: the consumer decides when to read → video |
| The same result for the same inputs and seed | Drawing or rendering pictures, shapes or video → image, vector, video (a chart of a signal, `bt-control plot`, is the one picture it makes) |

**Measures vs shapes:** audio *measures* ("a kick at 1.20 s"); control *shapes* ("on each kick, jump to 1 and release over 0.3 s").
Test for where a number belongs: would it change if the look changed? If yes, it is control's (smoothing times, ranges, gates).

**No sound, no pixels, no frames:** time is in seconds everywhere. No fps, frame number, sample rate of audio, or file format of a
picture or sound in a signal, a parameter or a format (the chart `bt-control plot` draws is for reading values and is the one
exception). A reader asks for a value at a time; the consumer chooses the times. Musical time is
a parameter unit, not a second clock: a value in `beats` is turned into seconds through a Grid.

**Formats this package owns** (plain data, documented and versioned here; changing one is a breaking release; carried by Rust-backed
classes, `crates/core/src/formats/`, the one definition of every rule): **Series** (values at
a fixed rate from a start time, with a unit and a documented scaling), **Event** (a moment in seconds, a strength, an optional
duration, label and pitch: one type for onsets, beats, notes and sections), **Grid** (beat times, meter and first downbeat;
converts beat positions and seconds), carried in one file, the **Bundle**; the rules are in `docs/formats.md`, the machine-readable form is
`schemas/bundle-1.schema.json`, generated from the Rust types. `bullterrier-audio` writes them **without importing this package**:
the schema is a file here, audio checks its output against a copy. A format is never shared as code.

## Names

`bullterrier-control` (PyPI, repository), `bullterrier_control` (import; docs alias `btc`), `bt-control` (command, `bt-control.toml`,
cache folder), `BT_CONTROL_*` (env vars), `BullterrierControlError` (base exception), `bullterrier_control.sources` (entry-point
group for packages of sources, shapers and operations). Full name for identity, `bt-` for what a person types. Names inside the package
(`Series`, `lib`, sources, shapers, operations) carry no prefix.

## Architecture

Layers, imports flow **downward only** (import-linter: contracts in `pyproject.toml`, run by `tests/test_layering.py` and
`uv run lint-imports`). A forbidden import means the code is in the wrong layer: move it.

```
app        __main__, cli
lib        every registered source, shaper and operation by name (`lib.slew`), with `help()` built from its parameters; `lib.pyi` types it
patch      Patch, load_patch: a graph of them as data (TOML), read, built against a bundle and saved by the Rust core; imports only registry, formats, errors and `_native`; the patch schema
plotting   plot: signals drawn as a PNG (matplotlib, an optional extra, imported when a picture is drawn)
plugin     the author API: everything a plugin's source, shaper or operation needs
registry   the package's one registry (the core's `Registry`; `Registered` is the core's `Thing`), `table()`; `@source`, `@shaper`, `@operation` (they check a Python function's shape, the core checks the rest); entry-point loading
rendered   Rendered: a signal that needs history, rendered on a grid and read by holding the latest sample  |  inputs   `INPUTS`, Keyframe: the Rust core's input rules, named for Python
signal     Signal (a handle over the Rust graph), Constant, from_series, from_values, minimum, maximum  |  params   Params, param: a plugin's declaration, built into the core's `ParamTable`  |  easing   `curve`: named and Bezier eases  |  hashing   `uniform`: seeded random numbers by index  |  rounding   decisions at a time boundary
_chain     Chain: a typed method per built-in shaper, which Signal inherits (generated; rounding > _chain > formats)
formats    Bundle > Series | Event | Grid > Unit: the Rust-backed classes of the extension, re-exported; `load`, `save`, `events`
_native    home of the Rust extension `_native._core`      (imports `errors` only, lazily, to raise its errors)
errors     BullterrierControlError > InvalidError > FormatError | ParameterError; SignalError; UnknownNameError
```

**Rust core:** `crates/core` (pure Rust: no PyO3, no numpy, safe code, builds for `wasm32-unknown-unknown`; one
`src/builtin/<name>.rs` per built-in, found by `build.rs`; `signal.rs` and `signal/` the graph; `easing`, `cadence`, `hashing`, `scaling`, `rounding` the primitives; `inputs.rs` the one place that says what each input (`points`, `events`, `grid`, `values`, `series`) is and checks it, with `Error::Input` listing up to 100 problems; `registry.rs` and `registry/` the `Registry` (a value, not a global), its `Thing`s, the `Source`, `Shaper` and `Operation` traits and the `Recipe` a signal made by a thing carries; `patch.rs` and `patch/` reading, building and saving a patch, using only public items of the other modules, which `patch/privacy.rs` scans for), `crates/macros` (the `Params` and `Choice` derives; the core
re-exports them; they only write code) and `crates/wasm` (the WebAssembly bindings: `Patch`, `Built`, `things`; names no shaper, passes arguments to the core unchanged and names each error as Python does; `poe wasm-test`), `crates/python` (PyO3 bindings building
`bullterrier_control._native._core`; generic, names no shaper; `signal.rs` is the `SignalBase` handle, `callback.rs` the nodes that call Python). The formats are `crates/core/src/formats/` (rules, JSON
reading and writing, schema); the bindings are `crates/python/src/{formats,params,inputs,data,error,registry,patch}.rs` (`registry.rs`: `Registry` and `Thing`, and a Python function registered as a callback thing; `patch.rs`: `Patch`, `Node`; `inputs.rs`: `Keyframe`, `check_input`; `data.rs` reads the package's own objects as their file data, and an exception a user's object raises while an input is read reaches the caller unchanged; formats and parameters turn it into a refusal message). They read a Python object with
their own deserializer over Python objects (`data.rs`), so a NaN reaches the core and is listed with every other problem,
and `FormatError.problems` lists the broken rules, the first 100 (`FormatError::omitted()` counts the others; a value of the wrong JSON type, an unknown field and the like stop
the reading at that value). Every built-in runs in Rust and has no Python module: its Rust file defines it, `Registry::builtin()` holds it and the package's
registry starts from that; a plugin's Python function registers as a callback thing. The Rust registry is the authority: it checks the name, a parameter named like an input or a patch key, the taken
name and every call; Python checks only what is about a Python function's shape. A call returns the core's signal, never the body's
own object, and a Python body gets a new handle over the caller's signal; an exception a Python body raises reaches the caller
unchanged. There is no other boundary: a Rust `Signal` is sampled without Python or the GIL, and Python code is a node in the graph only
where a plugin wrote a `compute` or a `Rendered(run=...)`. A Python `compute` is called with 1-D float64 times; its exception
reaches `sample`'s caller unchanged. `Constant` and `Rendered` list their native base first
(`class Rendered(_core.Rendered, Signal)`). `_native` is below every layer but `errors`, so any layer may call it. Rust built-ins
never refer to each other.

A source is `def name(params) -> Signal`, a shaper `def name(signal, params) -> Signal`, each registered with `@source` or
`@shaper` and its `Params` class (in Rust: `impl Source` or `impl Shaper` for the `Params` struct); packages are entry points of
the group `bullterrier_control.sources`. Pipes-and-filters: a
shaper maps a signal to a signal. An **operation** is data in, data out: `def name(params, *, events) -> tuple[Event, ...]` or
`-> Series`, taking its data as inputs (`events`, `series`, `grid`), registered with `@operation` (in Rust: `impl Operation` for `Params`, with `type Inputs` and `const RETURNS`); it may use the whole data (percentiles, a lead, the nearest
beat), which a signal may not. Each thing arrives with its first consumer (`poe dead-code` fails on unused code).

## The signal contract

Every signal, and so every source and shaper, promises this (`tests/builtin/test_contract.py` checks every registered one):
- **Total:** a value at every time. A time before 0 reads as time 0; a Series-backed signal holds its first and last value.
- **`sample(times)`** (never overridden) takes a number or an array of any shape, refuses what is not a finite number
  (text, a bool, NaN, an infinity) with `SignalError`, and returns float64 in the same shape. A signal implements
  `compute(times)`, which only ever sees a 1-D float64 array of finite times from 0 up.
- **Finite:** a value that is not finite (an overflow) is a `SignalError`, never returned. At any time a signal gives a finite
  value or raises `SignalError` (a stateful shaper refuses a time that needs more samples than its limit, and a read
  holding more than 2^26 samples across its stacked shapers).
- **Deterministic and read-independent:** the same times give the same values, whatever the order, size or number of earlier
  reads. A stateful shaper is a causal recurrence run through `Rendered` (output *i* depends on inputs 0 to *i*): never a
  field that remembers the last read.
- **Causal:** a read at time t depends on the input only up to t (a float rounding of a billionth of a sample aside). What
  comes later in a shaper's input never changes an earlier output.
- **Parameters declared** with `param` (unit, range, description): every number works at both ends of its range and is refused
  beyond it, `true` or a string is not a number, an unknown name is refused, and the refusal names the field.
A new source or shaper meets all of this by registering: the contract tests find it and no test names it. An operation makes
data, not a signal: it is covered by the parameter rules above, by `tests/builtin/test_operation_contract.py` (the same data
when called again, with events reversed or as dicts; valid, time-ordered events or a valid Series) and by its own tests.

## The project's rules

1. **Never import audio, image, vector or video.** Nor optionally, nor lazily. A format shared with a sibling is a plain data file
   owned by one package, not code.
2. **Sources, shapers and operations never import each other.** Shared numeric code goes to a module named for its concept (`interpolation`,
   `smoothing`, `noise`), never `utils`/`common`/`helpers`. Extract when a second consumer needs it.
3. **Parameters are declared once**, with unit, range and description, and drive
   validation, defaults and generated docs: a built-in's in Rust (`#[derive(Params)]`), a plugin's as a Python `class P(Params)` with `param()`; the rules are
   checked in Rust, in `ParamTable`, for both. Units include `seconds`, `beats` (through a Grid), `hertz`, `cycles`, `fraction`.
4. **Open/closed:** adding a source, shaper or operation creates files named for it and modifies none. The exceptions are
   generated: `docs/gallery.md`, which lists every source and shaper and is rewritten by `uv run poe gallery`; and
   `lib.pyi` (and, for a shaper, `_chain.py`), rewritten by `uv run poe api`. The drift test
   (`tests/tools/test_gen_api.py`) fails while either is stale.
5. **No hidden tunables.** Every value that shapes a result is a named, documented parameter in the field's own terms (attack,
   release, rate, depth, phase, slew). Exempt: mathematical and standard constants, each cited in a comment.
6. **Determinism:** same inputs and `seed` give the same signal on the same machine and library versions; across platforms equal
   within float tolerance. `numpy.random.default_rng(seed)`; no global random state. Reading a signal at a time never depends
   on what was read before.
7. **Layering and the contract are tests**, run over every source, shaper and operation automatically.
8. **Errors:** callers catch `BullterrierControlError` (`bullterrier_control.errors`). Library code raises a subclass, never a bare
   `ValueError`, for anything a caller or an input file can get wrong; each also inherits the matching builtin. A bug in the
   calling code (a wrong registration, a wrong argument to `Rendered`, a wrong call shape) is a plain `ValueError`/`TypeError`.

## Rust code of this project

Rust follows the same rules. Built-ins never refer to each other; what two share goes in a module of the crate named for its
concept, extracted when a second built-in needs it. A built-in's file declares its parameters (rule 3) as `pub struct Params`
with `#[derive(Params)]` (`crates/core/src/builtin/slew.rs` is the model): `pub` fields, a doc comment each (the one text of
rustdoc and `help()`) and `#[param(default = .., unit = .., min = .., max = .., advanced, cyclic)]`, a `unit` being a
`ParamUnit` variant. The derive writes `Default` and the `ParamTable`; a choice is a unit-only enum with `#[derive(Choice)]`
(`#[choice(name = "saw-down")]` renames a variant). A rule between fields is a function `fn(&Params) -> Vec<Problem>` named by
`#[param(rule = path)]` on the struct; it runs only once every field is valid. The table checks the declaration
(`ParamTable::new`) and reads the values (`parse`, strict, listing every broken rule), so the kernel gets typed values already
valid. A thing that runs in Rust also implements `Source`, `Shaper` or `Operation` for its `Params` (`impl Shaper for Params
{ const NAME: &'static str = "<file name>"; type Inputs = (); fn shape(&self, input: &Signal, (): ()) -> Result<Signal, Error> }`;
a source has `fn source(&self, inputs: Self::Inputs) -> Result<Signal, Error>`, an operation `const RETURNS` and
`fn run(&self, inputs: Self::Inputs) -> Result<Output, Error>`; `NAME` is its one name, checked against the file name at
compile time; `type Inputs` is its data, as for every kind: `()`, `Vec<Event>`, `Option<Grid>`, a struct for several,
recorded in its recipe); a stateful shaper builds its signal with `Signal::rendered`
over a private causal `run(inputs: &[f64], params: &Params) -> Vec<f64>` (output *i* depends on inputs 0 to *i*), and a
stateless one combines signals (`input * self.depth`) or retimes one (`Signal::retimed`: `delay`, `hold`, `repeat`) or maps its values (`Signal::mapped`: `remap`). The file's leading `//!` lines are its doc (`help()` in Rust). A typed
`Signal::<name>(&self, Params)` method is `self.pipe(params)` (with inputs, `self.pipe_with(params, inputs)`, its arguments in
`docs/api.md` rule 2's order): every door (`Thing::call`, `shape`, `pipe`, `pipe_with`, a typed method,
a `lib::` function) checks, runs, names and records the recipe (`Signal::recipe`, kept by a wrapper node, which
`Patch::from_signals` saves) through one private call. `build.rs` registers every file of `src/builtin/` in
`Registry::builtin()` as the kind its code (not a comment or a string) implements, and refuses to build a file with no
such `impl` or two (a source is also a `lib::` function, its arguments as `docs/api.md` rule 2 says); every list of the
built-ins is derived from that, and a thing's table is `Thing::table`. A signal is `Arc<dyn Node>` with `Node`
sealed; code outside the crate adds nodes with `Signal::from_fn`, `Signal::rendered`, `Signal::retimed` and `Signal::mapped`; no Rust lock is held while a
callback into Python runs. A node implements `start` and `finish` and never reads an input itself: the one loop in
`signal/walk.rs` does (a scan test fails otherwise), and drops go through a per-thread queue, so a graph of any depth
(`MAX_DEPTH`, 1000, a memory bound) is read and dropped on any stack. Callbacks (`from_fn`, `run`, a retiming's `map`, a mapping's `f`, a Python
`compute` or body) are the one recursion and are capped at 48 nested per thread (`Error::Depth`); the smallest supported thread
stack is 256 KiB. `Signal`'s `Debug` prints a node label and the depth only (`Node::label`). No hidden tunables (rule 5): a number is a `Params`
field, trivial, or a cited constant. No `unwrap`, `expect`, `panic!`, `todo!` or printing (clippy fails them): return an
`Error`; an error in the core is a bug in the caller, mapped to `ValueError`. `pub` only for what the bindings use, else
`pub(crate)`. A NaN in the input comes through (`sample` reports it as `SignalError`); the kernel never panics on it. A
built-in has no Python body. The `patch` module names only
public items of the others (a scan test fails otherwise).

## Parameters

A built-in declares its parameters in Rust ("Rust code"); a plugin, in Python: a class per source, shaper or operation, a
subclass of `Params` (frozen, strict whatever the annotation), each field `param(default, doc=...)`. Python builds the same
`ParamTable` from it that the derive builds, when the class is made (`ParameterError` naming each broken field); the rules
below live only in Rust (`ParamTable::new` and `parse`). Every field has a default and a description (what it does, unit and
range in words, what raising it does). A field is a number, a `bool`, a `str` or a `Literal` choice of strings, nothing else.
- **Numeric** (`float`, `int`; Rust `f64`, `i64`): also a `unit` (the Rust enum `ParamUnit`: `seconds`, `beats`, `hertz`,
  `cycles`, `fraction`, `decibels`, `ratio`, `semitones`, `count`) and inclusive `min`/`max` that hold the default. A parameter
  in `beats` is turned into seconds through a Grid; the declaration names the unit, the reader converts. `cyclic` (a phase) is a
  decimal in `cycles` one cycle wide: a value outside is wrapped into it.
- **Other** fields take no unit/min/max: `bool`, a choice (the doc mentions every choice), `str`.
- **Basic and advanced:** `advanced=True` marks a field that tunes the technique rather than chooses the shape; listed apart, still
  settable and validated. Aim at six or fewer basic fields.
- **Seeded where random:** a source that uses randomness has a `seed` field; none reads global state.
- **Values** are read strictly: `true` or `"1"` is not a number, `1.0` is not a whole number, `1` is not a switch, NaN and
  infinities and integers beyond 64 bits are refused, an unknown name is refused, a left-out field takes its default. The
  `ParameterError` lists every broken rule (`problems`).
- `ParamTable::describe` (Rust) gives each field's JSON-Schema-like properties as an object with sorted keys
  (`params()` keeps declaration order); `P.describe()` (Python) lists the basic fields before the advanced ones, each in
  declaration order, for `help()`, `list --json` and `lib.pyi`; `P(**values).to_dict()` gives the values.

Use the field's own terms (attack, decay, sustain, release, rate, depth, phase, slew, threshold, hysteresis, hold, ease, overshoot,
swing, seed, offset), never `strength`, `intensity`, `factor`, `val`, `k`, or the unit's name. A bad value fails at construction or
load, naming source, field, constraint and value. Not parameters: the fields of a format (`Event.strength`, `Series.rate`), which
`docs/formats.md` defines.

## Adding a source or shaper

1. **Scaffold:** `uv run poe new-source <name> --kind source|shaper|operation` writes `crates/core/src/builtin/<name>.rs`
   (the `Params` struct, the trait impl and its tests) and `tests/builtin/test_<name>.py`, working as written (the build script
   finds the file and registers it: no list to edit). A source or shaper also gets `examples/gallery/<name>.toml`, the patch its
   gallery picture is drawn from. Every list of the built-ins is derived from the files: none to edit.
2. **Fill in:** the `Params` struct (the field's own terms, unit, range, description: "Parameters"), the body (analytic
   where it can be; a causal recurrence through `Signal::rendered` where it needs history), the leading `//!` lines (their first
   sentence is what `help(lib.<name>)` shows). An operation takes its data as `type Inputs` (`Vec<Event>` for `events`, `Series`,
   `Grid`, `Option` of one for an optional input, or a struct of the operation's own for several) and declares its result in
   `const RETURNS`; a source or shaper takes its data the same way (`type Inputs = Vec<Event>;`, `()` for none; a source's
   `lib::` function then borrows it: `docs/api.md` rule 2), and a shaper with inputs goes into a chain by `pipe_with`.
   A built-in that needs data is written in Rust like this, never in Python. A plugin is Python, registered with `@source`,
   `@shaper` or `@operation`: its data is a keyword-only argument after `params` named in `inputs.INPUTS` (`def name(params, *,
   points)`; `= None` makes it optional), which the registry checks and passes on. A new kind of input is a variant of `InputKind` and of `Input` in
   `crates/core/src/inputs.rs` (its rules and tests), its conversion in `crates/python/src/inputs.rs`, a stub annotation in
   `inputs.py`, and one row in each of `SAMPLE_INPUTS` and `SMALLEST_INPUTS` in `tests/builtin/test_contract.py` (and in
   `INPUTS` of `test_operation_contract.py`, if an operation takes it).
3. **Test:** what it promises, each expected number worked out by hand; the contract tests already cover shapes, ranges,
   determinism, read order and causality for every registered one. If `docs/examples.md` names it under **Needs**, that
   example must now pass. Edit the gallery patch so the picture shows what it does (a shaper's has a feeding node).
4. **Gallery and gate:** `uv run poe gallery` draws its picture and rewrites `docs/gallery.md`; `uv run poe api` rewrites
   `lib.pyi` (and `_chain.py` for a shaper); `uv run poe check`, then `uv run lint-imports`. Only the new files, its
   picture, the page, the two generated files (and docs naming it) appear in `git status`. If `_chain.py` is broken,
   `git checkout -- src/bullterrier_control/_chain.py` and rerun `uv run poe api`.

## Differences from the shared rules

What this project does differently from `docs/rules.md`, each with its reason.
- **Public API:** `__all__` in `bullterrier_control`, `bullterrier_control.plugin` (for authors) and `bullterrier_control.registry`
  (queries); everything else is internal.
- **Dependencies:** numpy is the one runtime dependency, and the Rust crates `Cargo.toml` names (`deny.toml` limits which; the core
  never depends on PyO3 or numpy). Another needs a reason in `docs/design-decisions.md`: control must install without librosa,
  torch, Pillow or any media library. matplotlib is the one optional extra (`plot`), for `bt-control plot` and the gallery only.
- **Offline only in v1:** no live clock, MIDI or OSC output.
- **CI:** `build` installs each wheel and samples a patch that uses the extension (`slew`), its own smoke command.
- **Defects, rule 4:** outside input includes a signal from another package. The one skip allowed: a worked example in
  `docs/examples.md` waiting for the source it names under **Needs**.
- **Tests:** property examples are an envelope under `max` never above its largest strength, a gate's output two-valued, a
  slew-limited signal never faster than its limit; the private kernel that stays inline is `slew::run`; proptest regression files are
  one per module; a test that builds chains or graphs bounds their size; mutants run with `PYTHONDONTWRITEBYTECODE=1`.
- **Proofs:** the slow scaffold tests of `tools/dev.py` are marked `gate` too, with the files they run against in
  `project-triggers`. The proof of the WebAssembly gate is also marked `node` (it needs Node; CI runs the proofs on Linux only).
  The package's own gates (`wasm-check`, `wasm-test`) are `project-gates`, run in CI by `project.yml`.

## Skills

Installed globally: `superpowers:writing-plans`, `superpowers:test-driven-development`,
`superpowers:verification-before-completion` (process); `testing-best-practices` (tests); `software-design-philosophy` (deep
modules, no pass-throughs, no information leakage; interface docstrings state promises).
`superpowers:*` skills are process aids, not authority: they tend to verbose, over-engineered, over-defensive output. Where one
asks for more validation, layers, documents or steps than this file, follow this file.

