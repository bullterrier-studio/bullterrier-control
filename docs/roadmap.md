# Roadmap

Batches of work, in order. Each batch gets its own plan before work starts. Sizes: **S** about one working session,
**M** two or three, **L** four or more. Decisions behind the batches: [design-decisions.md](design-decisions.md).
Prior experiments with reusable code: `.local/prior-work.md` (git-ignored; machine-specific paths).

| # | Batch | Size | State |
|---|---|---|---|
| — | Scaffold: gate, hooks, CI, release tooling, CLAUDE.md | S | done |
| C0 | Formats: Series, Event, Grid | M | next |
| C1 | Core: Signal, evaluation, parameters, registry | M | |
| C2 | Sources | M–L | |
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
- **Grid**: tempo map (piecewise constant BPM at first), beat times, downbeat times, meter; converts beats ↔ seconds.
- JSON Schema files in the package (data, not code), plus loaders and writers that validate at the boundary.
- A file holds a set of named Series, Events lists and one Grid (the shape audio's `analyse()` writes).

Done when: schemas validate known-good and reject known-bad files; round-trip write/read is exact; the schema is
copyable by audio for its conformance test.

## C1 Core

- `Signal`: `sample(times) -> values` (vectorised, seconds), a domain (start, end), a unit.
- Evaluation: analytic signals are exact at any time; stateful shapers (slew, followers) render on an internal grid at a
  declared rate and are read by interpolation. Reading at t never depends on earlier reads.
- Signal from a Series (interpolation: linear, hold), constant, combine (add, multiply, min, max, crossfade).
- Parameters declared once with pydantic (as bullterrier-image): unit, range, description, default. Unit `beats` is
  resolved through a Grid.
- Registry of sources and shapers by name, entry-point group `bullterrier_control.sources`.

Done when: contract tests run over every registered source and shaper (range, determinism, reading order independence).

## C2 Sources

- Keyframes with easing: linear, smooth, ease in/out, cubic bezier, step/hold, overshoot (the title "slam").
- LFO: sine, triangle, square, saw (rising, falling), phase, free-running (Hz) or tempo-synced (cycles per beat).
- Envelopes started by events: AR / ADSR / exponential decay; retrigger rules; the inverted one is sidechain ducking.
- Step sequencer: a repeating list of values, one per grid step.
- Seeded noise (value noise, smooth noise), sample-and-hold random.
- Latch: 0 before an event, 1 after (the "wake" at a drop).

## C3 Shapers and event operations

- Signal shapers: slew (separate rise/fall), attack/release follower, one-pole smoothing, gate with hysteresis, sample and
  hold, quantize to the grid, range map with response curve (linear, exponential, gamma), normalise (percentiles),
  time shift (lead/lag), clip.
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
