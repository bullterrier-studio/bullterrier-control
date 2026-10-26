# Roadmap

Batches of work, in order. Each batch gets its own plan before work starts. Sizes: **S** about one working session,
**M** two or three, **L** four or more. Decisions behind the batches: [design-decisions.md](design-decisions.md).
Prior experiments with reusable code: `.local/prior-work.md` (git-ignored; machine-specific paths).

| # | Batch | Size | State |
|---|---|---|---|
| — | Scaffold: gate, hooks, CI, release tooling, CLAUDE.md | S | done |
| C0 | Formats: Series, Event, Grid | M | done (merged) |
| C1 | Core: Signal, evaluation, parameters, registry | M | done (merged) |
| C2 | Sources | M–L | next |
| C3 | Shapers and event operations | M | |
| C4 | Patch files, CLI, curve gallery | M | |
| C5 | First release | S | |

After C0, `bullterrier-audio` (A1, A2) can run in parallel with C1–C3: the two share only the formats.

## C0 Formats

The contract `bullterrier-audio` writes and `bullterrier-video` reads. Spec first, then code.
- `docs/formats.md`: each format's fields, units, invariants, versioning.
- **Series**: `start` (s), `rate` (Hz), `values`, `unit`, documented `scaling` (how 0..1 was reached, if it was).
- **Event**: `t` (s), `strength` (0..1), optional `duration` (s), `label`, `pitch` (MIDI). One type covers onsets, beats,
  notes, sections, risers.
- **Grid**: beat times, meter and first downbeat (tempo is derived, never stored); fixes the meaning of beats ↔ seconds.
- One file type, the Bundle: named Series, named Event lists and an optional Grid (the shape audio's `analyse()` writes).
- JSON Schema in `schemas/` (generated, committed), plus `load` and `save` that validate at the boundary.

Plan: `.superpowers/plans/2026-10-26-c0-formats.md` (git-ignored).
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

## C2 Sources

- Keyframes with easing: linear, smooth, ease in/out, cubic bezier, step/hold, overshoot (the title "slam").
- LFO: sine, triangle, square, saw (rising, falling), phase, free-running (Hz) or tempo-synced (cycles per beat).
- Envelopes started by events: AR / ADSR / exponential decay; retrigger rules; the inverted one is sidechain ducking.
- Step sequencer: a repeating list of values, one per grid step.
- Seeded noise (value noise, smooth noise), sample-and-hold random.
- Latch: 0 before an event, 1 after (the "wake" at a drop).
- Also: `poe new-source NAME` scaffolds a source's files from a template, and CLAUDE.md gains "Adding a source or shaper",
  as image's "Adding an effect" does (the template and the command arrive together).
- **Worked examples first** (they are C2's acceptance tests, written before the sources): `docs/examples.md` with a small
  input Bundle per scenario in `examples/`, the signal built from it, and the output as plain numbers at the times a
  consumer would ask. Output is numbers: no picture, no sound, and the examples never import another package; what the
  consumer does with the numbers is a line of arithmetic in a comment, labelled as the consumer's. Scenarios, from the
  ecosystem's "Where does it go?" ([ecosystem.md](ecosystem.md)):
  1. **Kicks to a glow radius** (audio, image): kick Events with a decay envelope, read at 30 per second.
  2. **A ramp for a progress parameter** (image): "evaporate from 0 to 90 % over 5 s": keyframes with easing.
  3. **Bass energy to a size** (audio, vector): a Series, slewed, clipped.
  4. **Tempo-synced LFO** (standalone, a MIDI controller or a light): a Grid and an LFO in cycles per beat.
  5. **The wake at a drop** (audio, video): a latch on one Event.
  Each expected number comes from a hand computation or an analytic formula, never from running the code. A test runs every
  example and compares; an example whose source does not exist yet is the to-do list.

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
