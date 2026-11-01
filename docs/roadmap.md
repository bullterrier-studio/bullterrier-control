# Roadmap

Small batches of work, in order. Each batch gets its own plan before work starts, ends merged into `main` with the whole
gate green and an independent review done, and leaves the worked examples it names passing. Sizes: **S** about one
working session, **M** two or three, **L** four or more. Decisions behind the batches: [design-decisions.md](design-decisions.md).

| # | Batch | Size | State |
|---|---|---|---|
| — | Scaffold: gate, hooks, CI, release tooling, CLAUDE.md | S | done |
| C0 | Formats: Series, Event, Grid, the bundle file | M | done |
| C1 | Core: Signal, `Rendered`, parameters, registry; `constant`, `clip`, `slew` | M | done |
| C1.5 | Developer experience: `lib`, `builtin`, errors by kind, `poe new-source`, worked examples | S | done |
| C2a | Inputs for sources; easing; `keyframes` (example 2) | M | done |
| C2b | `envelope`, `latch` (examples 1 and 5) | M | done |
| C2c | `lfo`, `steps` (example 4) | S | done |
| C2d | `noise` (seeded, read at any time) | S | done |
| C3a | Causal shapers; the scenes example | M | done |
| C3b | Operations on events and series | M | done |
| C4 | Patch files, CLI (`sample`, `list`, `plot`), curve gallery | M | done |
| R0 | Rust core: workspace, gates, `slew` and `follow` in Rust | M | done |
| R0b | Rust core: CI and a wheel per platform | S | done (first GitHub run pending) |
| R0.5 | Code-first API: the spec in Python and Rust, then the Python improvements | M | done |
| R1 | Rust core: the formats | M | done |
| R2 | Rust core: parameters | M | done |
| R3 | Rust core: the signal graph | L | done |
| R4a | Rust core: the inputs; pydantic leaves | S | done |
| R4b | Rust core: registry, `lib`, patches | M | done |
| R4c | Rust core: the graph walks by loop, not recursion | S | done |
| R5a | Rust core: inputs on every kind; nine built-ins with data | M | done |
| R5b | Rust core: the eight built-ins left in Python | M | done |
| R5c | Rust core: derive the hand-kept lists; the empty Python `builtin` package goes | S | done |
| R6 | Rust core: cleanup, the public crate | S | |
| C5 | First release (after R6: it ships the Rust core) | S | |

C2a went first: it added the inputs mechanism every later source uses. C2b, C2c and C2d then depend only on C2a and can
run in any order. `bullterrier-audio` can run in parallel: the two share only the formats.

## C0 Formats

The contract `bullterrier-audio` writes and `bullterrier-video` reads. Spec first, then code.
- `docs/formats.md`: each format's fields, units, invariants, versioning.
- **Series**: `start` (s), `rate` (Hz), `values`, `unit`, documented `scaling` (how 0..1 was reached, if it was).
- **Event**: `t` (s), `strength` (0..1), optional `duration` (s), `label`, `pitch` (MIDI). One type covers onsets, beats,
  notes, sections, risers.
- **Grid**: beat times, meter and first downbeat (tempo is derived, never stored); fixes the meaning of beats ↔ seconds.
- One file type, the Bundle: named Series, named Event lists and an optional Grid (the shape audio's `analyse()` writes).
- JSON Schema in `schemas/` (generated, committed), plus `load` and `save` that validate at the boundary.

Also: `bt-control validate` and `bt-control schema`; CLAUDE.md gains a short Formats pointer to `docs/formats.md`.

Done when: schemas validate known-good and reject known-bad files; round-trip write/read is exact; the schema is
copyable by audio for its conformance test.

## C1 Core

- `Signal`: `sample(times) -> values` (vectorised, seconds); total: defined for every time (before 0 reads as 0, a Series holds
  its ends). `+ - *`, negation, `minimum`, `maximum` combine signals.
- Grid conversions (`seconds_at(beat)`, `beat_at(seconds)`) as specified in `docs/formats.md`; numpy joins here.
- Evaluation: analytic signals are exact at any time; stateful shapers (slew, followers) render on an internal grid at a
  declared rate and a read holds the latest sample at or before the time. Reading at t never depends on earlier reads
  or on input after t.
- Signal from a Series (interpolation: linear, hold); `Rendered` for causal stateful shapers (a read holds the latest sample).
- Built-ins that give the contract tests every kind of subject: source `constant`; shapers `clip` (stateless) and `slew`
  (stateful), moved here from C3.
- Parameters declared once (pydantic until R2, Rust tables since): unit, range, description, default. Unit `beats` is
  resolved through a Grid.
- Registry of sources and shapers by name, entry-point group `bullterrier_control.sources`.

Done when: contract tests run over every registered source and shaper (range, determinism, reading order independence).
Also: CLAUDE.md gains "The signal contract" (what every source and shaper promises, what the contract tests check), as image's
"The effect contract" does.

## C1.5 Developer experience

`lib` (every source and shaper by name, with `help()` built from its parameters); the built-in package renamed `builtin`;
errors by kind (`ParameterError` for a bad parameter value, `FormatError` for a bad file, both `InvalidError`s); the model
base and strict types in `validated`; `poe new-source` scaffolding; `docs/examples.md` with hand-computed numbers, run by a
test; `docs/ecosystem.md` so the repository stands on its own.

## C2a Inputs, easing, keyframes

- The input mechanism: a source or shaper declares named data as keyword-only arguments, from a closed table
  (`inputs.INPUTS`); the registry checks each value and `help()` lists them. First kind: `points`. Adding one is a table entry.
- `Keyframe` (time, value, the ease of the segment that starts there); dicts are accepted too.
- `easing`: linear, hold, in, out, in-out (smoothstep), cubic-in, cubic-out, cubic-in-out, overshoot (Penner back-out), and CSS
  `cubic-bezier(x1, y1, x2, y2)`. Formulas with sources cited (CSS Easing Level 1, Robert Penner).
- `keyframes`: values between points by each segment's ease; the first value holds before the first point, the last from the last.
- Example 2 passes.

## C2b Envelopes and the latch

- The `events` input (a list of `Event` or dicts in any order, checked into time order); `Event` joins the `plugin` names.
- `envelope`: on each event, attack, decay toward a sustain level and, for an event with a duration, release;
  exponential or linear; overlapping events take the larger (`max`) or add up (`sum`); a `label` picks events. Memory
  O(times + events).
- `latch`: one value before the first (labelled) event, another from it on.
- Examples 1 and 5 pass.

## C2c LFOs and step sequences

- `lfo`: sine, triangle, square (with duty), saw, saw-down; `rate` in cycles per second or per beat (through a Grid, so it
  follows tempo changes); `phase`; output between `low` and `high`.
- `steps`: a repeating list of values, one per step, steps per second or per beat.
- Adds the `grid` and `values` inputs.
- Example 4 passes.

## C2d Noise

- A counter-based hash (SplitMix64), so a random value is a pure function of seed and index: readable at any time, in any
  order, the same on every platform.
- `noise`: held, linear or smooth between random values at `rate` per second, a `seed`, output between `low` and `high`.

## C3 Shapers and operations

Split by the signal contract ([design-decisions.md](design-decisions.md), "Decided before C3"): a shaper is causal, so
whatever needs the whole data or its future (percentiles, a lead, snapping to the nearest beat) works on the data before
it becomes a signal. (`slew` and `clip` shipped in C1.)

### C3a Causal shapers; the scenes example (done)

- First, example 6: sections (events with a duration) through `envelope` give a scene intensity, with no new code.
- `follow` (attack/release follower; equal times are one-pole smoothing), `gate` (hysteresis), `hold` (sample and hold at
  events), `remap` (0..1 onto `low`..`high` through an ease), `delay` (a lag).

### C3b Operations on events and series (done)

- A third registered kind, the operation: data in, data out, parameters declared like a source's, in `lib`.
- Events: `select` (label, strength threshold), `every` (every Nth), `thin` (minimum gap), `shift` (lead or lag),
  `quantize` (to the grid), `beats` (events from a grid: beats, downbeats, subdivisions). Merging is concatenation, since
  the `events` input sorts; a signal quantized to the grid is `hold` at `beats`.
- Series: `normalise` (between two percentiles, to a `fraction` series that states its `scaling`).
- Example 7: strong kicks only, thinned, through `envelope`.

## C4 Patch files, CLI, gallery (done)

- Patch file (TOML): a named graph of sources, shapers and operations with their values, the way an image recipe is a list of steps.
- `bt-control sample PATCH --from --to --rate` (JSON/CSV Series out), `bt-control list` (sources, shapers, parameters),
  `bt-control plot` (PNG, matplotlib as an optional extra).
- For consumers: `bt-control list --json` gives every source, shaper and operation with its inputs and parameters (unit, range,
  default, doc), so another tool can check a pipeline before it runs. The patch file carries a `format` number and
  has a published JSON Schema, like the bundle. Unit names stay stable: a consumer maps a signal onto its own units
  (pixels, degrees, ...), and control's names (`seconds`, `beats`, `fraction`, ...) are its side of that mapping.
- A gallery: one plot per source and shaper, generated, in `docs/`.
- README example that runs.
- Parameter descriptions keep one shape across the ecosystem: if `bullterrier-image` marks parameters that wrap (a
  `cyclic` flag for angles), `list --json` carries it too (`lfo.phase` wraps).

## R0 to R6 Rust core

The package becomes a Rust crate, `bullterrier-control`, with this Python package as its interface (as polars is): for
speed, threads without the GIL, small memory, reading signals from a Rust video renderer without Python, and a core
that builds for WebAssembly. Foundation first, then the engine, the built-ins last, each ported once and its Python
body deleted. Until a built-in is ported, the engine calls its Python body, the path third-party Python plugins keep.
The Python suite runs unchanged against the Rust core throughout.

- **R0 Foundation (done).** Cargo workspace (`crates/core`: pure Rust, no Python, builds for wasm32; `crates/python`: the
  bindings), maturin as build backend, Rust gates as strict as the Python ones (each proven by a known-bad input),
  `slew` and `follow` rendered in Rust through a transitional boundary (`_core.render`, removed in R3). Done when their tests pass
  unchanged and the gate is green.
- **R0b CI and wheels (done; first GitHub run pending).** Every workflow that imports the package builds the core; a wheel per
  platform (Linux x86_64 and aarch64, macOS arm64 and x86_64, Windows x86_64), each installed and run before release.
- **R0.5 Code-first API.** The programmatic API is the product; see the section below.
- **R1 Formats (done).** `Series`, `Event`, `Grid` and `Bundle` are Rust types (`crates/core/src/formats/`) that load, check
  and save a bundle with no Python; the Python classes are the extension's own, not Pydantic models. Every broken rule is
  listed. The schema is generated from the Rust types, its meaning unchanged (`meter` and `first_downbeat` now state
  `minimum`, which validators enforce). The Python API changed where noted in
  [design-decisions.md](design-decisions.md), "The formats in Rust".
- **R2 Parameters (done).** All 21 built-ins declare their parameters in Rust (`#[derive(Params)]`: unit, range, doc,
  `advanced`, `cyclic`, rules between fields); the Rust `ParamTable` checks every declaration and every value, a Python
  plugin's included (its `class P(Params)` is unchanged); `help()`, `list --json` and `lib.pyi` read the table; pydantic
  is gone from `Params`. The Python API changed where noted in [design-decisions.md](design-decisions.md), "Parameters in
  Rust".
- **R3 Signal graph (done).** `Signal` is `Arc<dyn Node>` in Rust: constants, series, arithmetic, `maximum`/`minimum`,
  `Rendered` (history kept in Rust) and `sample` need no Python or GIL; easing, cadence, hashing, scaling and rounding are
  Rust primitives; `slew` and `follow` are Rust `Signal` shapers; `_core.render` is gone. Python's `Signal` is a handle
  over the graph, and a plugin's `compute` and `Rendered(run=...)` are nodes that call back into Python. `poe new-source
  --kind shaper` writes a Rust-backed shaper. Moved out: `Params.__getattr__` and the `PARAMS`/`SHAPERS` split of
  `build.rs` to R5 (19 built-ins still run in Python); `gen_api`'s own range text, pydantic for `Keyframe` and the inputs'
  models, and the duplicate `Finite`/`Whole`/`Switch`/`Text` aliases to R4 (they go with the inputs and `lib`). Decisions:
  [design-decisions.md](design-decisions.md), "The signal graph in Rust".
- **R4a Inputs (done).** The core's `inputs` module checks `points`, `events`, `grid`, `values` and `series` (`Error::Input`,
  at most 100 problems); `Keyframe` is a Rust class; pydantic, `validated` and the duplicate `Finite`/`Whole`/`Switch`/`Text`
  aliases are gone, and numpy is the one Python dependency. Decisions and accepted differences:
  [design-decisions.md](design-decisions.md), "Inputs in Rust".
- **R4b Registry and patches (done).** The core's `Registry` holds every source, shaper and operation (Rust things register
  themselves through the `Source`, `Shaper` and `Operation` traits, Python ones as callbacks); every way of making a signal
  from a thing is one checked door that records a recipe; `constant` and `select` run in Rust as the pilots; a patch is
  read, built and sampled with no Python (the `toml` crate), and a graph made through the registry saves as a patch
  (`Patch.from_signals`, `to_toml`, `save`). `gen_api` reads ranges from the core's table. Decisions and accepted
  differences: [design-decisions.md](design-decisions.md), "The registry and patches in Rust".
- **R4c Iterative walk (done).** A graph is read and dropped by loops on the heap, not by recursion, so `MAX_DEPTH` (1000)
  bounds memory, not stack, on every target; callbacks nested more than 48 deep per thread are refused; the
  smallest supported thread stack is 256 KiB. Decisions and numbers: [design-decisions.md](design-decisions.md), "The graph
  walks by loop, not recursion (batch R4c)".
- **R5a Inputs and operations (done).** `Source`, `Shaper` and `Operation` take `type Inputs`; the `lib::` signature of a
  source with inputs follows api.md rule 2; `Signal::retimed`; `follow`'s `exp` is `libm`'s. `delay`, `hold`, `latch`,
  `every`, `thin`, `shift`, `beats`, `quantize` and `normalise` run in Rust (13 of 21), each bit-identical to its old
  Python body in a differential harness. Decisions: [design-decisions.md](design-decisions.md), "Built-ins with data in Rust
  (batch R5a)".
- **R5b The rest (done).** `clip`, `gate`, `remap`, `noise`, `steps`, `keyframes`, `lfo` and `envelope` run in Rust
  (all 21), each matching its old Python body in the differential harness; `Signal::mapped`; `lib::` for a source whose
  inputs are a struct of several (`steps`); `sin` and `exp` are `libm`'s. The Python `builtin` package is left empty.
  Decisions: [design-decisions.md](design-decisions.md), "The last built-ins in Rust (batch R5b)".
- **R5c Cleanup (done).** Every list of the built-ins is derived from `crates/core/src/builtin/`; the empty Python
  `builtin` package, its entry point and contracts, `Params(builtin=...)`, `Params.__getattr__` and the crate's
  `parameters`/`builtins` are gone. Against the Python bodies no ported built-in is slower: a `lib` call takes 0.2 to
  0.9 of the time, `lfo` and `envelope` read in half the time. Decisions: [design-decisions.md](design-decisions.md),
  "The Python built-ins go (batch R5c)".
- **R6 Cleanup.** Crate documentation; final speed and memory measured against R0's baseline. The Python helpers no
  built-in uses any more (`counter`, `Positions`, `between`, `plugin.matching`): kept as author API or removed.
  Bundle-built graphs (`series.<name>`) cannot be saved, which a source that reads a series would fix.

## R0.5 Code-first API (done)

The programmatic API, in Python and later in Rust, is the product; patch files are a serialisation of the same graph,
built only through it ([design-decisions.md](design-decisions.md), "API first"). Weak spots in today's Python API: `lib`
is dynamic (no stubs, so no completion of `rise=` and mypy sees `Any`); shapers nest (`clip(slew(x))`) and read
inside-out, not as the pipeline; a measured curve needs three concepts (`from_series(Series(rate=..., values=...,
unit=Unit.RATIO))`).
- `docs/api.md`: the code-first API in Python and Rust side by side, one worked example per concept (a curve from values,
  keyframes, an envelope on events, shaping a chain, combining, sampling a timeline, a seeded noise, a beat grid, a
  plugin). The Rust column is the target R1 to R5 build to: Rust type and trait names, parameters as structs with
  `Default`, `signal.sample(&times)`, errors as `Result`.
- Python: a typed `lib` through a generated stub (`lib.pyi` from the parameter models: every source, shaper and operation
  with its keyword parameters, defaults and docstring; a drift test: regenerated equals committed); chaining (built-in
  shapers as typed methods on `Signal` from the same generator, `signal.pipe(shaper, **params)` for any shaper including
  plugins); a one-call entry for a curve from values (`btc.from_values(values, rate=10)`), keeping `Series` and
  `from_series` for data from files. The final shape is decided in the batch's plan, from the spec.
- The README's quick start uses the code-first API first; patch files come later, as the extra.

Done when: every example in `docs/api.md` runs as a test (Python now; the Rust column becomes doctests as R1 to R5 land);
`lib` completes its keywords in an IDE and mypy checks them; the stub's drift test passes; the gate is green.

## C5 First release

GitHub repository under `bullterrier-studio`, first CI run (the workflows have only been linted so far), trusted
publishing on TestPyPI and PyPI, `uv run poe release 0.1.0rc1`, then `0.1.0`.

## Later, with a trigger

- **Conventional names**, when `bullterrier-audio` writes its first analysis: list once in `docs/formats.md` the event
  labels and series names producers and consumers share (`kick`, `snare`, `hat`, section labels, band names, and C3b's
  `beat`, `downbeat`, `sub`), so the producer and every consumer do not each decide them. The format does not validate
  them.

## Not in v1

- Real-time output (a live clock driving MIDI or OSC). Offline first; later as an optional extra.
- Mapping a signal onto a parameter of something else: that is video's.
