# Design decisions

The decisions that shape bullterrier-control and still hold, each with its reason and what it costs or commits us to. Rules for
contributors are in [CLAUDE.md](../CLAUDE.md). The ecosystem-wide decisions (the five packages, names, shared formats) are in
[ecosystem.md](ecosystem.md).

## Scope

- **Time is in seconds, and only seconds.** A signal is read at a time given in seconds; there are no frames, sample indices or
  rates of a medium in the interface. *Reason:* frames exist only in video, and a signal tied to one clock cannot serve a
  second consumer. *Consequence:* a Series carries its own rate and start time, and a reader resamples at read time.
- **Control shapes; audio measures.** Anything that would change if the look changed (smoothing time, range, gate threshold) lives
  here; what is true of the sound whatever the look (tempo, onset time, band energy) lives in audio. *Reason:* analysis is
  expensive and cacheable, shaping is cheap and tuned by hand; mixing them makes every tweak re-run the analysis.

## Formats

- **Series, Event and Grid are plain data owned here.** `bullterrier-audio` writes them without importing this package; it
  validates its output against a copy of the schema published here. *Reason:* the ecosystem rule is that packages connect through
  data formats, not code, so audio stays installable without control. *Cost:* the format is known in two places; it is kept tiny
  and each side tests against the schema. Specified in `docs/formats.md` (batch C0).
- **One file type, the Bundle.** Series, Event lists and a Grid travel together in one JSON file with a `format` and a
  `version`. *Reason:* one thing to load, validate and version; audio's `analyse()` result is one file. *Cost:* a consumer that
  wants one signal reads the whole file; acceptable at the sizes involved, and a binary version can follow as `version` 2.
- **Grid stores beat times, not a tempo.** Tempo is derived from the intervals. *Reason:* a tempo map plus beats can disagree;
  beat times are what beat trackers produce and what a renderer needs, and tempo changes then need no extra field.
- **Non-finite numbers are rejected in files.** *Reason:* Python's JSON accepts `NaN` and `Infinity`, other languages' parsers
  do not; an undefined stretch is an Event or an absent series, never `NaN`.

## Dependencies

- **numpy and pydantic are the runtime dependencies, each added with the first code that imports it** (deptry fails on a
  dependency nothing imports, so none is declared in the scaffold). *Reason:* control must install in a second on a machine
  with no media libraries, and `bullterrier-audio` carries the heavy ones (librosa, torch). pydantic declares parameters the
  way bullterrier-image does, so one parameter description has one shape across the ecosystem.

## Agreed with the owner (2026-10-26)

- **Musical time is a unit, resolved through a Grid.** Time stays in seconds; a parameter may be in `beats` (a release of
  0.3 beats, an LFO at 1 cycle per beat). *Reason:* music is written in beats (the Sonic Pi experiment works only in beats),
  and one clock in seconds keeps signals readable by anyone. *Consequence:* Grid is a format of this package.
- **One Event type with an optional duration.** Onsets, beats, notes, sections and risers are all Events; a moment has no
  duration. *Reason:* one type is a smaller format than Event plus Span, and every consumer handles both the same way.
- **Analytic where possible, rendered where stateful.** Keyframes, LFOs and envelopes on events are exact at any time; slew,
  followers and other stateful shapers render on an internal grid at a declared rate and are read by holding the latest
  sample at or before the time (C1: interpolating would let a read depend on input after the time read).
  *Reason:* random access in seconds with no hidden history, without forbidding the shapers every DAW has.
- **Parameters are pydantic models, as in bullterrier-image.**
- **Offline only in v1.** Real-time output (a live clock to MIDI or OSC) is later, as an optional extra.
- **Order:** formats first (C0), then control's core and audio's analysis in parallel (`docs/roadmap.md`).

## Project

- **The tooling mirrors bullterrier-image** (one gate, `uv run poe check`; ruff rules, mypy, vulture, deptry, import-linter,
  pre-commit hooks, Conventional Commits, CI workflows, release command). *Reason:* five repositories that are gated the same way
  need no relearning. *Cost:* each repository owns its copy of the tooling scripts and tests; a fix to one is made in the
  others by hand. Image-specific parts (effects, gallery, subject finding) were left out.
- **The version is 0.0.0 until the first release.** `poe release` accepts only a version above the one in `pyproject.toml`, so
  the first release (0.1.0rc1, then 0.1.0) needs a lower starting point.

## Core (batch C1)

- **A signal is total.** `Signal.sample(times)` is defined for every time: time before 0 reads as time 0, and a signal made
  from a Series holds its first and last value beyond its ends. *Reason:* a consumer asks for any time without checking a
  domain first, and a signal that returns "undefined" for some times pushes that case onto every caller.
- **One validated entry, one hook.** `sample` is final in the base class (float64, the input's shape, NaN and infinity refused
  with `SignalError`); a signal implements `compute`. *Reason:* the check at the boundary is written once, not in every signal.
- **Combining is arithmetic, not a registered shaper.** `+`, `-`, `*`, negation, `minimum` and `maximum` on signals (floats
  allowed). *Reason:* they have no parameters to declare, and a crossfade is `a * (1 - m) + b * m`.
- **Stateful shapers render once and a read holds the latest sample at or before the time** (`Rendered`): the shaper's `run`
  is a causal function from the input sampled on a grid at a declared `rate` (an advanced parameter) to an output of equal
  length. *Reason:* causality makes a read independent of what was read before, whatever the order or chunking; holding
  rather than interpolating keeps a read at time t from depending on input after t (interpolation did leak up to one
  internal sample: a review caught it); a limit on the number of samples keeps a read at an absurd time from filling
  memory. *Cost:* the output is a staircase of one internal step (1 ms by default).
- **A signal never returns a value that is not finite, and `sample` cannot be overridden.** `sample` is `@final`; an
  overflow or a bug in `compute` is a `SignalError` naming the time, not an infinity that travels on. Times that are text,
  a bool or too large for a float are refused too.
- **Parameter declarations are closed.** A field is a number, a `bool`, a `str` or a `Literal` of strings; the model is
  strict whatever the annotation; defaults are validated; no aliases, no names that are discouraged or are units; ranges are
  finite. *Reason:* each hole a review found (an `Optional` number with no range, a plain `float` that accepted `true`, a
  default nothing checked) would have reached every patch.
- **Parameter units say `hertz`, not `hz`**, as a Series' `Unit` does; the two vocabularies differ on purpose (what a
  measured series holds, what a parameter's number is in) and share names where they meet.
- **Sources reach the library only through `plugin`.** A forbidden-import contract enforces it, so what an author sees is
  what the built-ins use.
- **Test hooks are `registry.isolated()` and `registry.unregister`.** A registration is re-registerable only if it is a
  top-level function of a module (reloading); two functions made by one factory are different things.
- **Programming errors are plain `ValueError` and `TypeError`.** A wrong registration, a wrong argument to `Rendered` or a
  wrong call shape is a bug in the calling code, not something a caller, a file or a patch can get wrong; those are
  `BullterrierControlError`s.
- **Parameters are declared with `param(default, doc=, unit=, min=, max=)`** on a `Params` model; the rules are checked when the
  class is made. *Reason:* a broken declaration fails at import, naming the field, not in some patch much later.
- **One entry-point group, `bullterrier_control.sources`, for sources and shapers.** *Reason:* naming.md names the group; a shaper
  is a source of signals made from a signal, and one group means one loading rule.
- **`slew` and `clip` ship in C1** (they were C3 items): the contract tests need a stateless and a stateful shaper to run over.
- **Errors of declared rules are `FormatError`** (a format's fields, a source's parameters): one wording, one place.

## Build or adopt

Checked once, with a quick search (four queries; a first pass, not a survey):
- **Easing curves (C2).** Python libraries exist (`pytweening`, `easing-functions` with the Penner set including elastic and
  back-overshoot, `rpeasings`, `easings` with Bezier and Catmull-Rom). Adopt the formulas with the source cited, or depend on
  one if it is small, typed and maintained; check cubic-bezier easing and the overshoot ("slam") curve against `easings` and
  `easing-functions`. Do not port photo-fx's old `timing.py` curves without the same check.
- **Filters and slew (C3).** `scipy.signal` would do some of it, but scipy is not a dependency and control installs with numpy
  and pydantic only; the filters stay ours (a causal recurrence each, tested against a hand-computed response).
- **LFOs, envelopes on events, tempo sync, the Grid (C2).** No standalone Python library found that reads a value at any time in
  seconds; synthesis libraries (SignalFlow, Pyo) are real-time engines with their own graphs. New code.
- **JAMS** is a JSON annotation format for music (beats, onsets, segments, chords) with a schema, validation and a mir_eval bridge.
  It overlaps Event and Grid. Keep the strict Bundle (its Series, one file for analysis and curves, and the rules in
  docs/formats.md are what audio and video need); a JAMS import and export adapter is possible later. JAMS's field layout was
  not read in this check: read it before claiming how close the two are.

## Re-evaluated before C2 (batch C1.5)

Every decision above was looked at again before more sources were built on it. Kept: one bundle file, Grid as beat times,
no `NaN` in files; total signals; causal `Rendered` read by holding; closed, strict parameter declarations; entry-point
registry with `plugin` as the one door; numpy and pydantic only; offline only; easing written here; keyframes instead of a
separate ramp. Changed:

- **`lib` is how sources and shapers are used** (`lib.slew(signal, rise=0.2)`), not `registry.get("slew")`. *Reason:* the
  common call should be the short one; `dir(lib)` and `help(lib.slew)` (built from the parameter declarations) make what is
  installed discoverable. The registry stays for queries.
- **The built-in package is `builtin`**, not `sources`: it holds shapers too. The entry-point group keeps its published
  name, `bullterrier_control.sources`.
- **Errors by kind.** A bad parameter value is a `ParameterError`, a bad file or format value a `FormatError`; both are
  `InvalidError`s with the same `path: rule (got value)` wording. *Reason:* `FormatError` for `rise=-1` told a caller to look
  at a file that does not exist.
- **The model base and the strict value types live in `validated`**, below both `formats` and `params`. *Reason:* parameters
  depended on the formats package for something that is not a format.
- **A source's inputs are a closed set (`events`, `grid`, `points`, `values`), passed by keyword.** *Reason:* parameters stay
  named scalars (drivable, listable); data (events, keyframes, a step list) is not a parameter.
- **An envelope on a moment is attack then decay to 0; sustain and release apply to an event with a duration.** *Reason:* the
  first draft closed a moment's gate at once, so a kick with no release vanished instead of decaying.
- **Small batches (C2a to C2d), each closing worked examples.** The examples and `poe new-source` came first, so every source
  batch starts from a scaffold and ends with numbers checked by hand.
- **Committed files stand on their own.** `docs/ecosystem.md` carries the ecosystem's rules; nothing committed points at a
  folder outside the repository.
