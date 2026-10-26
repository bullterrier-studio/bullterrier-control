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
| C2a | Inputs for sources; easing; `keyframes` (example 2) | M | next |
| C2b | `envelope`, `latch` (examples 1 and 5) | M | |
| C2c | `lfo`, `steps` (example 4) | S | |
| C2d | `noise` (seeded, read at any time) | S | |
| C3 | Shapers and event operations | M | |
| C4 | Patch files, CLI (`sample`, `list`, `plot`), curve gallery | M | |
| C5 | First release | S | |

C2a goes first: it adds the inputs (events, a grid, keyframes, values) every later source takes. C2b, C2c and C2d then
depend only on C2a and can run in any order. `bullterrier-audio` can run in parallel: the two share only the formats.

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

- A source may take named inputs besides its parameters, by keyword: `events` (Events, sorted), `grid` (a Grid), `points`
  (Keyframes) and `values` (numbers); a closed set, declared as keyword-only arguments of the source function.
- `Keyframe` (time, value, the ease of the segment that starts there).
- `easing`: linear, in, out, in-out (smoothstep), cubics, sine, CSS cubic-bezier (named curves and `bezier(x1,y1,x2,y2)`),
  back (overshoot), step, hold. Formulas written here with their sources cited (CSS Easing Level 1, Robert Penner).
- `keyframes`: values between points by each segment's ease; holds (or zero) before the first and after the last.
- Example 2 passes.

## C2b Envelopes and the latch

- `envelope`, started by events: a moment (no duration) rises over `attack` and falls to 0 over `decay`; an event with a
  duration rises, falls toward `sustain` while it lasts, then falls to 0 over `release`. Exponential or linear; overlapping
  events take the larger (`max`) or add up (`sum`); a `label` picks events. Memory O(times + events).
- Ducking is arithmetic: `1 - depth * envelope`.
- `latch`: one value before the first (labelled) event, another from it on.
- Examples 1 and 5 pass.

## C2c LFOs and step sequences

- `lfo`: sine, triangle, square (with duty), saw, saw-down; `rate` in cycles per second or per beat (through a Grid, so it
  follows tempo changes); `phase`; output between `low` and `high`.
- `steps`: a repeating list of values, one per step, steps per second or per beat.
- Example 4 passes.

## C2d Noise

- A counter-based hash (SplitMix64), so a random value is a pure function of seed and index: readable at any time, in any
  order, the same on every platform.
- `noise`: held, linear or smooth between random values at `rate` per second, a `seed`, output between `low` and `high`.

## C3 Shapers and event operations

- Signal shapers: attack/release follower, one-pole smoothing, gate with hysteresis, sample and hold, quantize to the grid,
  range map with response curve (linear, exponential, gamma), normalise (percentiles), time shift (lead/lag). (`slew` and
  `clip` ship in C1.)
- Event operations: filter by label or strength, every Nth, downbeats only, minimum gap (thinning), shift, quantize to the
  grid, merge.

## C4 Patch files, CLI, gallery

- Patch file (TOML): a named graph of sources and shapers with their values, the way an image recipe is a list of steps.
- `bt-control sample PATCH --from --to --rate` (JSON/CSV Series out), `bt-control list` (sources, shapers, parameters),
  `bt-control plot` (PNG, matplotlib as an optional extra).
- A gallery: one plot per source and shaper, generated, in `docs/`.
- README example that runs.

## C5 First release

GitHub repository under `bullterrier-studio`, first CI run (the workflows have only been linted so far), trusted
publishing on TestPyPI and PyPI, `uv run poe release 0.1.0rc1`, then `0.1.0`.

## Not in v1

- Real-time output (a live clock driving MIDI or OSC). Offline first; later as an optional extra.
- Mapping a signal onto a parameter of something else: that is video's.
