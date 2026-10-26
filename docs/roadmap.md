# Roadmap

Small batches of work, in order. Each batch gets its own plan before work starts, ends merged into `main` with the whole
gate green and an independent review done, and leaves the worked examples it names passing. Sizes: **S** about one
working session, **M** two or three. Decisions behind the batches: [design-decisions.md](design-decisions.md).

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
| C3a | Causal shapers; the scenes example | M | |
| C3b | Operations on events and series | M | |
| C4 | Patch files, CLI (`sample`, `list`, `plot`), curve gallery | M | |
| C5 | First release | S | |

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
- Parameters declared once with pydantic (as bullterrier-image): unit, range, description, default. Unit `beats` is
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

### C3a Causal shapers; the scenes example

- First, example 6: sections (events with a duration) through `envelope` give a scene intensity, with no new code.
- `follow` (attack/release follower; equal times are one-pole smoothing), `gate` (hysteresis), `hold` (sample and hold at
  events), `remap` (0..1 onto `low`..`high` through an ease), `delay` (a lag).

### C3b Operations on events and series

- A third registered kind, the operation: data in, data out, parameters declared like a source's, in `lib`.
- Events: `select` (label, strength threshold), `every` (every Nth), `thin` (minimum gap), `shift` (lead or lag),
  `quantize` (to the grid), `beats` (events from a grid: beats, downbeats, subdivisions). Merging is concatenation, since
  the `events` input sorts; a signal quantized to the grid is `hold` at `beats`.
- Series: `normalise` (between two percentiles, to a `fraction` series that states its `scaling`).
- Example 7: strong kicks only, thinned, through `envelope`.

## C4 Patch files, CLI, gallery

- Patch file (TOML): a named graph of sources and shapers with their values, the way an image recipe is a list of steps.
- `bt-control sample PATCH --from --to --rate` (JSON/CSV Series out), `bt-control list` (sources, shapers, parameters),
  `bt-control plot` (PNG, matplotlib as an optional extra).
- For consumers: `bt-control list --json` gives every source and shaper with its inputs and parameters (unit, range,
  default, doc), so another tool can check a pipeline before it runs. The patch file carries a `format` number and
  has a published JSON Schema, like the bundle. Unit names stay stable: a consumer maps a signal onto its own units
  (pixels, degrees, ...), and control's names (`seconds`, `beats`, `fraction`, ...) are its side of that mapping.
- A gallery: one plot per source and shaper, generated, in `docs/`.
- README example that runs.
- Parameter descriptions keep one shape across the ecosystem: if `bullterrier-image` marks parameters that wrap (a
  `cyclic` flag for angles), `list --json` carries it too (`lfo.phase` wraps).

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
