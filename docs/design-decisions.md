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

- **numpy is the one runtime dependency, added with the first code that imports it** (deptry fails on a dependency nothing
  imports, so none is declared in the scaffold). *Reason:* control must install in a second on a machine with no media
  libraries, and `bullterrier-audio` carries the heavy ones (librosa, torch). pydantic was the second until R4a: it
  declared parameters the way bullterrier-image does, parameters became Rust's in R2, and the inputs followed in R4a
  ("Inputs in Rust").

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
- **Parameters are declared once, with unit, range and description.** Pydantic models until R2; Rust tables since ("Parameters in Rust").
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
- **Stateful shapers render on a grid, keep what they rendered, and a read holds the latest sample at or before the time** (`Rendered`): the shaper's `run`
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
  what the built-ins use. The one exception is the compiled `_native` package, the lowest layer (see "The Rust core").
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
  (and pydantic, until R4a) only; the filters stay ours (a causal recurrence each, tested against a hand-computed response).
- **LFOs, envelopes on events, tempo sync, the Grid (C2).** No standalone Python library found that reads a value at any time in
  seconds; synthesis libraries (SignalFlow, Pyo) are real-time engines with their own graphs. New code.
- **JAMS** is a JSON annotation format for music (beats, onsets, segments, chords) with a schema, validation and a mir_eval bridge.
  It overlaps Event and Grid. Keep the strict Bundle (its Series, one file for analysis and curves, and the rules in
  docs/formats.md are what audio and video need); a JAMS import and export adapter is possible later. JAMS's field layout was
  not read in this check: read it before claiming how close the two are.

## Re-evaluated before C2 (batch C1.5)

Every decision above was looked at again before more sources were built on it. Kept: one bundle file, Grid as beat times,
no `NaN` in files; total signals; causal `Rendered` read by holding; closed, strict parameter declarations; entry-point
registry with `plugin` as the one door; numpy and pydantic only (pydantic left in R4a); offline only; easing written here; keyframes instead of a
separate ramp. Changed:

- **`lib` is how sources and shapers are used** (`lib.slew(signal, rise=0.2)`), not `registry.get("slew")`. *Reason:* the
  common call should be the short one; `dir(lib)` and `help(lib.slew)` (built from the parameter declarations) make what is
  installed discoverable. The registry stays for queries.
- **The built-in package is `builtin`**, not `sources`: it holds shapers too. The entry-point group keeps its published
  name, `bullterrier_control.sources`.
- **Errors by kind.** A bad parameter value is a `ParameterError`, a bad file or format value a `FormatError`; both are
  `InvalidError`s with the same `path: rule (got value)` wording. *Reason:* `FormatError` for `rise=-1` told a caller to look
  at a file that does not exist.
- **The model base and the strict value types live in `validated`**, below both `formats` and `params` (until R4a removed them). *Reason:* parameters
  depended on the formats package for something that is not a format.
- **A source's inputs are a closed set (`events`, `grid`, `points`, `values`), passed by keyword.** *Reason:* parameters stay
  named scalars (drivable, listable); data (events, keyframes, a step list) is not a parameter.
- **An envelope on a moment is attack then decay to 0; sustain and release apply to an event with a duration.** *Reason:* the
  first draft closed a moment's gate at once, so a kick with no release vanished instead of decaying.
- **Small batches (C2a to C2d), each closing worked examples.** The examples and `poe new-source` came first, so every source
  batch starts from a scaffold and ends with numbers checked by hand.
- **Committed files stand on their own.** `docs/ecosystem.md` carries the ecosystem's rules; nothing committed points at a
  folder outside the repository.

## Decided in C2a

- **Only the input mechanism and `points` now; `events` comes with C2b, `grid` and `values` with C2c.** *Reason:* each thing
  arrives with its first consumer. A new kind is one `INPUTS` entry and a row in each contract table.
- **`Keyframe` lives with the inputs and raises `ParameterError`; its ease is checked when it is built.** *Reason:* it is
  not a bundle format, and a bad call argument is a parameter error. An error at `points[1].ease` shows at the call, not at
  the first read. Dicts are accepted, which gives patch files (C4) their parsing.
- **Nine eases with exact formulas, plus CSS `cubic-bezier(x1, y1, x2, y2)`; no Material names.** *Reason:* each name has a
  citable formula (Penner; CSS Easing Level 1); the Material names are one `cubic-bezier` each and have no standard to cite.
  `hold` is the animation-tool name for a step.
- **`keyframes` has no before/after parameters:** it holds the first value before the first point and the last value from the
  last. *Reason:* fewer knobs; zero outside the points is one more point.
- **No wall-clock performance test; the read uses `searchsorted`.** *Reason:* timing assertions are flaky; the algorithm is
  fixed instead.

## Decided in C2b

- **A moment is an event that never ends and has sustain 0.** *Reason:* one formula serves a kick and a held note;
  there is no second code path for events without a duration.
- **An exponential tail is dropped once it is below 1e-9 times its strength** (ln(1e9), about 20.7 time constants into
  the fall). *Reason:* it bounds the reads each event touches, so memory is O(times + events), not their product. Each
  event is within that of the uncut formula; under `sum` the errors of many events add up. A linear fall ends exactly,
  so it is not cut.
- **The input check returns `events` sorted by `t`, stably** (equal times keep the order given). *Reason:* the result
  does not depend on the order the caller gave, apart from events at the same time, which keep the given order (a
  float `sum` is not associative), and callers need not sort.
- **The `path: rule` message join lives once in `inputs.py`**, in the helper both kinds use. *Reason:* one place
  decides how an input error reads (`events[1].strength: ...`), and the registry still names no kind.
- **`latch` ignores strength and duration.** *Reason:* it answers "has the first matching event happened?"; a level
  that follows strength is what `envelope` is for.
- **A moment with `attack` 0 and `decay` 0 is invisible: the value is 0 everywhere.** *Reason:* it rises and falls in
  zero time, and a read at one time cannot see a spike of no width. It is the formula's own limit, so there is no
  special case; the parameter docs say that 0 falls at once.
- **The `label` field and its rule `params.label in ("", event.label)` are written in both `envelope` and `latch`.**
  *Reason:* two short copies; they are extracted when a third consumer (the C3 event operations) needs them.

## Decided in C2c

- **`lfo` and `steps` share one module, `cadence`, for the position of a read.** *Reason:* "rate times seconds, or times
  beats of a grid" and the rule that `per="beat"` needs a grid are one decision; two copies would drift. It sits in the
  layer of `signal`, `params` and `easing`, below `plugin`, which re-exports `counter`.
- **A time out of reach is a `SignalError`, whichever way it is out.** *Reason:* a `Grid` refuses a position it cannot
  compute with a `FormatError`, but the contract says a signal gives a finite value or a `SignalError` at any time, so
  `cadence` re-raises it as one; an overflowing `rate * t` is refused the same way, so no source sees a non-finite position.
- **`per` defaults to `"second"` in both sources, and `grid` is optional.** *Reason:* the contract tests build every source
  from its required inputs alone, and a source must work with its defaults. `per="beat"` without a grid is a
  `ParameterError` naming `per`; a grid given per second is ignored, so one grid can be passed to every source of a patch.
- **An `lfo` stays inside `low` and `high` exactly, by clipping.** *Reason:* `low + (high - low) * s` lands an ulp beyond
  `high` in about one case in twenty at `s = 1`, and a consumer doing `round(127 * value)` would leave its range. The range
  is documented as a promise, so it is enforced.
- **A cycle position of exactly 1.0 is read one float step below 1.** *Reason:* `np.mod` of a tiny negative number (a time
  just before a first beat that is not at 0) rounds to 1.0, though the true position is a hair before the end of a cycle;
  reading it as 0.0 would send a `square` with `duty` 1 low and flip a saw from the top of its ramp to the bottom.
- **Before the first beat the position is negative and `steps` counts back from the end of its list.** *Reason:* the Grid
  continues the first beat length backwards, so a cycle or list runs on without a seam at the first beat.

## Decided in C2d

- **The random number is SplitMix64's output number `index`, not a hash of `seed * step + index`.** *Reason:* integers
  only, so the same bits on every platform, and it matches the published generator, so a test can check it against the
  reference outputs rather than against itself. Distinct seeds are distinct starting states of one stream; the finaliser
  scrambles neighbouring states, so seeds below 2^32 give unrelated values in practice.
- **A position beyond what an integer holds is reduced mod 2^64 in floats before it becomes an index.** *Reason:* at a rate
  of 1000 a time of 1e300 is a position of 1e303, which no integer type holds; `np.mod` on floats is exact, so the read
  is defined, deterministic and never casts out of range.
- **`noise` has no `per="beat"`.** *Reason:* a random value per beat is a `steps` of random `values`; tempo-synced noise
  waits for a consumer that needs it.
- **`lfo` and `noise` put 0..1 on `low`..`high` through one function, `scaling.between`.** *Reason:* the exact-range
  promise (see C2c) is one decision, extracted when `noise` became the second consumer.
- **A step edge sits where `rate * t` reaches a whole number, to within float rounding.** *Reason:* `rate * (k / rate)`
  can round just below `k` (rate 7, t = 61/7 reads step 60), so a read at a "frame time" may land on the step before. No
  float formula avoids it for every rate, and a read inside a step is exact, so the docs say so rather than hide it behind
  a tolerance. It applies to `steps` and `noise` `hold`; `lfo`, `linear` and `smooth` are continuous there.

## Decided before C3 (agreed with the owner, 2026-10-26)

- **A shaper stays causal; whatever needs the whole data or its future works on the data.** Percentile normalising needs
  every value, a lead reads ahead, snapping an event to the nearest beat may move it earlier. These are operations on
  events and series, done before the data becomes a signal; the shapers (follow, gate, hold, remap, delay) read only the
  past. *Reason:* the signal contract (causal, the same value however it is read) is what lets a consumer read any time
  in any order; one shaper that looks ahead would break that for every signal built on it, and the contract tests would
  refuse it anyway. *Consequence:* a lead exists for events only; a signal can only lag (`delay`).
- **C3 is two batches: causal shapers (C3a), then operations on data (C3b).** *Reason:* they are two kinds of thing with
  different contracts (signal to signal; data to data), and small batches each end with worked examples checked by hand.
- **The scenes example comes first.** Sections are events with a duration, and `envelope` turns them into a scene
  intensity with no new code. *Reason:* it checks long events through `envelope` before the new shapers build on it, and
  shows a consumer the pattern.

## Decided in C3a

- **`follow` replaces "one-pole smoothing".** Symmetric smoothing is `follow` with `attack == release`. *Reason:* one
  shaper, one formula; a `smooth` alias would be a second name for one decision.
- **`remap`'s response curve is an ease** (the names keyframes use, and `cubic-bezier(...)` for any custom curve).
  *Reason:* one vocabulary of curves. Gamma and exponential curves wait for a consumer that needs them.
- **`remap` does not clip its input itself.** *Reason:* the ease already reads its progress clipped to 0..1 (an input
  below 0 gives `low`, above 1 gives `high`); a second clip would be a second copy of that rule. The test pins it.
- **`hold` has no `label` parameter.** *Reason:* C3b's `select` picks events for every consumer; `envelope` and `latch`
  keep theirs.
- **`hold` reads the input at time 0 before the first event, and everywhere when there are no events.** *Reason:* the
  signal stays total and causal, with no invented 0. Events at the same time are one event; strength, duration, label
  and pitch are ignored.
- **`delay` only lags** (`lag` 0 or more). *Reason:* a lead would read the input's future; it exists for events only
  (C3b `shift`). Before `lag` seconds have passed it shows the input at time 0.
- **The formulas of `follow` and `gate`.** `follow` writes `level = d * level + (1 - d) * x`,
  `d = e^(-1 / (rate * time))`, with `d = 0` for a time of 0 (no division, and the output is the input exactly, which
  `level + (x - level) * 1` is not in floats). *Reason for the mix:* it cannot overflow however far apart `level` and
  `x` are (`x + (level - x) * d` took 1e308 to -1e308 across a gap no float holds), so a pass-through is exact at the
  edge of the floats too. It stays between the lowest and highest input so far to within float rounding. `gate` opens
  at `x >= open` and closes at `x < close`, so equality opens and does not close; `close` above `open` is refused, and
  `close == open` is a plain threshold.
- **The causality contract test builds each shaper from the sample inputs** (it built them from a signal alone, which
  a shaper with a required input, `hold`, cannot be), and the sample events gain one after the time the two inputs
  differ at. *Reason:* a shaper that reads the input only at events would otherwise show no difference after the
  change, and the test's own "it can fail" check would refuse it.

## Decided in C3b

- **Operations are registered, a third kind beside sources and shapers.** An operation is data in, data out
  (`def name(params, *, events) -> tuple[Event, ...]`, or `-> Series`), registered with `@operation` and called through
  `lib` with keyword inputs. *Reason:* parameters are declared once (CLAUDE.md, rule 3), which holds only through the
  registry; plain functions would declare each default twice, and `help()`, `dir(lib)` and C4's patches and `list --json`
  get operations for nothing. The return annotation is the declaration of what comes back and is checked at registration;
  an operation takes no signal. *Cost:* the contract tests split: the signal rules run over sources and shapers, the
  parameter rules over all three, and `test_operation_contract.py` holds what is specific to data.
- **No `merge`, no `downbeats`, no signal `quantize`, no Series lead or lag.** Merging is `events=[*kicks, *snares]`, since the
  `events` input sorts; downbeats are `select(events=beats(grid=g), label="downbeat")`; a signal on the grid is
  `hold(signal, events=beats(grid=g))`; a lead for a series waits for a consumer (build the Series with another `start`).
  *Reason:* each would be a second name for something that composes.
- **`matching(events, label)` is the one rule for a `label` parameter**, in `inputs`, used by `envelope`, `latch` and
  `select`. *Reason:* C2b planned this extraction for the third consumer.
- **`quantize` rounds half up, to the later position, and never moves an event below time 0; `shift` trims.** The position is
  `floor(beat * division + 0.5) / division` in beats, so it follows tempo changes; an event whose nearest position is before
  0 goes one position up. `shift` drops a moment before 0 and a span ending at or before 0, and starts a span that crosses 0
  at 0 with the rest of its duration. *Reason:* an Event has `t >= 0`, and a cue must not be lost to rounding that the grid
  alone caused. The snap never decreases, so sorted events stay sorted and `quantize` does not sort again; events that land
  on one time keep their order.
- **A time too far from the beats to snap is a `ParameterError` naming `events`.** *Reason:* the Grid reports it as a
  `FormatError` about no input in particular; for an operation it is a value of its `events` input.
- **`normalise` with equal percentiles maps a spike to 0.** A stretch of equal values holding both percentiles gives zeros,
  an outlier included (documented in its docstring). *Reason:* with no span there is no scale to put the outlier on.
- **`thin` and `every` count in the order of the sorted input.** `thin` keeps the first of events at one time (a gap above
  0), `every` counts them one by one. *Reason:* the `events` input sorts stably, so the result never depends on the order
  given.
- **`normalise`: `lower` runs 0 to 0.99 and `upper` 0.01 to 1.** *Reason:* `upper` must exceed `lower`, so a range of 0 to 1
  for both has no valid corner (both at 0, or both at 1), and the contract test sets every number at one end of its range.
  The values are first scaled by a power of two (exact) into -1 to 1, so a series reaching the largest floats does not
  overflow, and a value far above a tiny span divides to beyond the floats and is clipped to 1 without a warning.
  Percentiles are numpy's `linear` (Hyndman and Fan, type 7). Equal percentiles give zeros, an outlier included.
- **`beats` marks a downbeat by `(beat - first_downbeat) % meter == 0`.** *Reason:* `first_downbeat` is below `meter`
  (the Grid checks it), so no beat before it can satisfy the rule and a second test for it would be dead code.
- **A time that is a bound to within a nanosecond is the bound, decided once in `rounding`.** `at_least(value, bound)` and
  `more_than(value, bound)` (`TOLERANCE = 1e-9` seconds, re-exported by `plugin`) decide `thin`'s gap, `shift`'s landing at
  and ending at 0, and `quantize`'s "before 0". *Reason:* with decimal inputs the arithmetic lands a rounding error either
  side of the bound (`0.3 - 0.1` is `0.19999999999999998`, a span of `0.1 + 0.2 - 0.3` is 3e-17 s long, a position
  3 beats before beats at 0.6, 0.8, 1.0 is at -2e-16 s), and the first versions dropped an event or bumped one a whole
  position for it. The tests had used only multiples of 1/8 and 1/64, which floats hold exactly. A nanosecond is far
  above that error for times under a day and far below any time that means something; a bound missed by a millionth
  is still missed. A position at 0 within the tolerance is set to 0 exactly. The sample-relative `_ROUNDING` of `signal`
  and `rendered` (shares of a sample) is another quantity and stays.

## Decided in C4

- **A patch names signals only, and is built against a bundle given later.** A patch is a look, a bundle one song's
  measurements, so one patch serves every song (`Patch.build(bundle)`, `--bundle FILE`). *Reason:* control shapes and the
  consumer routes (ecosystem rule); a patch that said what a signal drives, or which song it was for, would be a project
  file, which is video's. A top-level key that is not a node, and a node key that is no input or parameter, are refused,
  so a mapping cannot creep in. A video project embeds a patch as a table (`Patch.parse`) and refers to its nodes by name.
- **Inputs are inline data or references: `events.<name>`, `series.<name>`, `grid` (paths into the bundle) or a node
  above.** *Reason:* the paths are the ones error messages already use, and a new series in audio's output can never clash
  with a node name, because `events`, `series` and `grid` (and `format`, `version`) are not node names. A node uses only
  the nodes above it: no cycles, and a patch reads top to bottom like a recipe. A reference must fit the key (events for
  `events`, a series for `series`, the grid for `grid`, a signal or a series for `signal`); `points` and `values` are
  inline only, since no bundle holds them.
- **A series where a signal is wanted is read linearly.** *Reason:* the common case (a measured curve into `slew`); `hold`
  interpolation waits for a consumer, and a patch can reach it by a source of `steps`.
- **A node's keys are `use`, `signal`, inputs and parameters; a parameter cannot be named `use` or `signal`.** *Reason:*
  one flat table reads best in TOML, and the registry refuses the clash once, as it does for input names.
- **The header is `format = "bullterrier-control/patch"` and `version = 1`, like the bundle.** *Reason:* a reader tells the
  two files apart by `format`, and the versioning rules are the bundle's.
- **Every problem of a patch is reported together, as `<node>.<key>: rule`; what the patch and the registry decide is
  checked when it is read.** *Reason:* a patch is written by hand and checked before a song exists; one error at a time
  makes a long patch a long conversation. Two things wait for `build`: the names a bundle must hold, and a rule a thing
  applies when it is called (`lfo` with `per = "beat"` needs a `grid`), which is the thing's own and cannot be asked of
  it without data. So `validate` can say ok to a patch whose one node fails when built; its message names the node.
  `build(only=...)` builds just the nodes asked for and what they use, so a patch with a node that needs a bundle
  still samples the others without one.
- **The patch schema is written by hand from the reader's own constants, not generated from a model.** *Reason:* the keys
  of a patch are the node names, which no model declares; the node-name rule and the header live once in `patch.py`. The
  schema is structural; what depends on the registry is `bt-control list --json`'s.
- **`list --json` describes a thing with the JSON Schema of its parameter model, in the shape bullterrier-image gives its
  parameters** (`minimum`, `maximum`, `enum`, `default`, `description`, and the extras `unit`, `advanced`, `cyclic`).
  *Reason:* video reads the parameter description of every package to map a signal onto a parameter; one shape means one
  reader. The range is stated once (the duplicate `min` and `max` extras went), and `returns` is lower case everywhere
  (`signal`, `events`, `series`). No wrapper or version key, as in image. The unit names are stable: a consumer maps a
  signal onto its own units (pixels, degrees) from them.
- **`cyclic` is image's idea, in cycles:** a number whose value one cycle further is the same position wraps (image's is an
  angle in degrees; the flag is described the same way in both). Here it is a
  decimal in `cycles` whose range is exactly one cycle (`max == min + 1`); any finite number (a whole number or a
  decimal) is wrapped into the range, and a value inside it, either end included, is kept as given. `lfo.phase` is the first. The schema keeps the
  one-cycle `minimum` and `maximum`, so a generic validator refuses a value the model wraps: a consumer wraps first.
  *Reason:* a rotating phase needs no wrap of its own, and one flag, described the same way, serves every package.
- **`sample` includes the last time when it falls on a sample time to within a nanosecond** (`rounding.TOLERANCE`), and
  refuses more than ten million samples. *Reason:* `0.29 * 100` is `28.999999999999996`, and a range written in decimals
  must not lose its last sample to it; a typo (`--rate 1e9`) must not fill the memory first. Its JSON output is a bundle
  (a series of unit `ratio` per node, starting at `--from`), so what `sample` writes is what the rest of the ecosystem reads.
- **matplotlib is an optional extra (`plot`), imported when a picture is drawn.** *Reason:* control installs with numpy
  (and pydantic, until R4a) only; a picture is a convenience of the command line and the gallery. Without it `plot` is a
  `BackendUnavailableError` (as in image) naming the extra. A PNG carries no version or date, so a rerun writes the same
  bytes. matplotlib writes its font cache with the locale's encoding when first imported, which the warning gate reports,
  and before 3.10.7 it calls pyparsing names that pyparsing 3.3 deprecates (the floor, 3.8, fails the lowest-dependency
  run at import). Both warnings are exempt in `filterwarnings`, only from matplotlib's own modules: the floor stays at
  what the code needs, not at what keeps a third party quiet.
- **The gallery is one patch per built-in source and shaper, a picture of each, and one generated page.** The patch is
  `examples/gallery/<name>.toml` (its node `<name>` is the thing, its other nodes feed it), drawn over four seconds into
  `docs/gallery/<name>.png`; `poe new-source` writes the patch. No picture is compared (it depends on the drawing library);
  tests check that every built-in has a patch that builds and moves, a picture exists, and the page's text is current.
  *Cost:* `docs/gallery.md` lists every source, so adding one modifies it (one of the two exceptions to rule 4: the generated API files are the other, see "Code-first API (R0.5)"), and two
  branches that add sources conflict in it; regenerating it resolves that. Operations have no picture: they give no signal.

## The Rust core (batch R0)

- **The numeric work moves to Rust; Python stays the interface** (owner, 2026-10-28). Two crates in a Cargo workspace
  beside `src/`: `crates/core` (pure Rust, no PyO3 or numpy, `#![forbid(unsafe_code)]`, builds for
  `wasm32-unknown-unknown`) and `crates/python` (PyO3 bindings that build `bullterrier_control._native._core`, built by
  maturin). A built-in is ported once and its Python body deleted; there is no second implementation. The core crate has
  its public name, `bullterrier-control`, from the start; nothing is published but the Python package. *Reason:* a
  video renderer reads signals every frame without Python; the stateful shapers are Python loops that hold the GIL
  (ten of them over a four-minute song take about 2 s: fine offline, too slow for interactive editing); a core that builds
  for WebAssembly can edit curves in a browser. *Consequence:* contributors need rustup (`rust-toolchain.toml` installs the
  rest), wheels are per platform (abi3: one per platform for every Python from 3.12), runtime Python dependencies stay
  numpy and pydantic (pydantic left in R4a).
- **One generic boundary, `_core.render(name, inputs, params)`, and it was transitional.** It took the shaper's input
  sampled at its rate from time 0 and returned a series of the same length, with the GIL released; the bindings named no
  shaper, so porting one never changed them. R3 removed it, when `Signal` and `Rendered` moved to Rust (below).
- **`Rendered` promises its `run` a fresh 1-D, C-contiguous float64 array.** *Reason:* the core reads the input in place,
  without the GIL; if each recurrence had to cope with a float32, strided or misaligned array, or the core copied it, the
  cost and the bug would be repeated per shaper. Fresh (always a copy, owned and aligned) because a plugin's `compute` may
  return a view it keeps and changes from another thread. `Rendered` makes the copy once, in the one place a recurrence is
  called.
- **A NaN in a shaper's input comes through the kernel.** A Rust kernel never panics on it; `sample` reports it as a
  `SignalError`, as for a Python recurrence. *Reason:* the signal contract is kept in one place (`sample`), not in each
  kernel.
- **`_native` is the lowest layer; built-ins may import it directly.** It is a package of its own at the bottom, beside
  `errors`, and imports only `errors` (below, R1): the import linter cannot read a compiled module but sees the
  package. Any layer may therefore call it. The contract "Built-in sources use only the author API" leaves it out of its
  forbidden list, an explicit exception to the author-API door (a built-in would otherwise reach it only through `plugin`).
  *Reason:* a built-in runs its body in Rust, and `plugin` stays what a third-party author needs. Today only `slew` and
  `follow` call it.
- **Until R2, pydantic was the one declaration of parameters.** The Rust `Params` repeated the fields and the core
  trusted the values; vulture could not see the core read a field by name, so `poe dead-code` wrote a whitelist. R2
  removes all three (below).
- **The Rust gates match the Python ones**, each in `poe check` with a known-bad test (`tests/tools/test_rust_gates.py`):
  rustfmt; clippy (pedantic, no `unwrap`, `expect`, `panic` or printing; dead code, missing docs and unreachable `pub` as
  errors); rustdoc; cargo-shear; cargo-deny (bans, licences, sources; advisories in `poe audit`); a `wasm32` build of the
  core; stubtest of `_native/_core.pyi`; cargo test (proptest for laws such as slew's rate bound). Rust built-ins never
  refer to each other (an architecture test, as for the Python ones). Warnings are errors only in the gate, so a newer
  compiler never breaks a user's build. No `.cargo/config.toml` and no `target-cpu=native`: a wheel must run on every CPU
  of its platform.
- **Platforms: wheels for five, the sdist for the rest.** One abi3 wheel each (every Python from 3.12) for Linux x86_64
  and aarch64 with glibc 2.28 or newer (manylinux_2_28), macOS arm64 and x86_64, and Windows x86_64, built by
  maturin-action on GitHub's runners (Linux natively in the manylinux container of each architecture; macOS x86_64
  cross-compiled), with no build cache. Windows on Arm, musl Linux (Alpine) and glibc older than 2.28 install from the
  sdist, which needs Rust (rustup; the sdist carries `rust-toolchain.toml`, so it builds with the compiler CI tests);
  only a glibc Linux build of the sdist is tried before release. Every wheel is installed from the artifact alone on its
  own platform, and the sdist built, each running `bt-control` and a patch that uses `slew` (the Rust extension),
  before anything is published. *Reason:* a user on a common platform never needs a compiler, and nothing published
  goes untried. manylinux2014 (glibc 2.17) is end-of-life and no dependency here
  forces a floor, so it is 2_28. A musl or Windows arm64 wheel waits for someone to ask.

## API first (owner, 2026-10-28)

- **The programmatic API is the product, in Python now and in Rust once ported; patch files are a serialisation of the
  same graph, built only through that API.** A patch is a nice extra, never a feature the API lacks. *Reason:* the API
  should be extremely clear, typed and discoverable in an IDE, and a file format that can do more than the API says the
  API is missing something. *Consequence:* batch R0.5 (`docs/roadmap.md`) specifies the API in Python and Rust side by
  side and improves the Python one; every batch from R1 on meets it; R4 enforces "the patch uses only the public API" with
  an import contract in Python and module privacy in Rust.

## Parameters in Rust (batch R2)

Owner decisions (2026-10-31):
- **A Python plugin keeps `class P(Params)` with `param()`; Rust does the checking.** The class builds a `ParamTable` of the
  core when it is made; `P(**values)` reads values through it. *Reason:* what an author writes does not change, and every
  rule (declaration and values) has one definition that Rust, the web app and the CLI share. *Cost:* pydantic's wording
  of a refusal is gone (`ParameterError` lists `path: rule` for every broken rule), and so is inheriting fields from
  another `Params` class.
- **The units a parameter may have are a Rust enum of their own, `ParamUnit`, not the format `Unit`.** *Reason:* the two
  lists differ (a series is measured in `midi`, which no parameter is; a parameter is in `cycles`, `semitones` or `count`,
  which no series is) and one type for both would let a file or a parameter name a unit it cannot have. Each is defined
  once, in Rust; Python holds neither list.
- **All 21 built-ins declare their parameters in Rust now; only `slew` and `follow` have a Rust body.** *Reason:* the
  declaration is the part everything else (help, `list --json`, `lib.pyi`, the check) reads, so it cannot wait for a body.
  `build.rs` lists every built-in in `PARAMS` and those with a Rust body in `SHAPERS`; R5 deletes the difference (see R3).

Rulings:
- **`Error` and `FormatError` stay two types, both `#[non_exhaustive]`, both carrying a list of problems.** `docs/api.md`
  now says one error type per module family. *Reason:* a render error (unknown shaper, parameters) and a format error (a
  file) are handled differently; one enum would force a caller to match variants that cannot occur. `ParamError` is
  the third, for declarations and values of a table alone.
- **A rule between fields is a function named on the struct (`#[param(rule = path)]`), run after the field rules** (clip's
  `high` not below `low`, gate, every, normalise, remap). *Reason:* these rules were pydantic validators; in Rust they
  live beside the fields they compare, and a field that is already broken is not compared. A Python plugin has no such hook.
- **`remap.ease` is text, and its curve names are listed in Rust and in Python (`easing.py`).** A drift test fails when
  they differ. *Reason:* easing moves to Rust in R3; a second copy until then costs less than a Rust easing now. The
  test is one-directional (a name only Rust knows is not caught). R3 removed both: `Ease::from_str` is the one list.
- **At the Python boundary** an integer beyond 64 bits and a non-finite float are refused (also for a cyclic field, which
  pydantic wrapped); a value nested deeper than 128 containers is refused, in the one conversion shared by parameters
  and formats. *Reason:* JSON cannot hold the first two, and a self-containing or very deep value crashed the
  interpreter. A decimal's values are always `float` (a default written `1` reads `1.0`).
- **At most 100 problems are kept and the total is exact** (`ParamError::omitted()`; the Python message shows the first 20
  and `... and N more`). *Reason:* the cap of the formats applied to parameters: a hostile value must not cost memory
  per broken item.
- **`describe()` gives the properties pydantic's JSON Schema gave** (`type` or `enum`, `default`, `description`, `title`,
  `minimum`, `maximum`) plus `unit`, `advanced`, `cyclic`, as an object with sorted keys (`params()` keeps the
  declaration order; the Python layer lists basic fields before advanced ones). The Python registry adds the model-level
  keys (`additionalProperties`, `title`, `type`), so `list --json` is unchanged; `lib.pyi` and the generated chain
  are unchanged too. Python's `P.describe()` adds `range`, the range text `help()` prints.
- **`help()` prints a whole number's range in full** (`0 to 4294967295` for `noise.seed`, was `4.29497e+09`); `lib.pyi`
  still prints `:g`.
- **A parameter left out of `render` takes its default** (it was an error); Python always sends all of them.
- **`UNITS`, `DISCOURAGED`, `_check`, `fields_doc` and the vulture whitelist are gone.** Declaration rules are
  `ParamTable::new`'s, and with no Python fields left plain vulture sees no field the core reads by name.

Breaking for plugin authors: a broken declaration is `ParameterError` (was `ValueError`); `fields_doc()` and `model_*`
are replaced by `describe()` and `to_dict()`; fields are not inherited from a parent `Params` class; an annotation that
is none of float, int, bool, str or `Literal` is refused.

## Code-first API (R0.5)

Taught in `docs/api.md`, Python and Rust side by side; every Python example there runs as a test.

- **Shapers are taught as methods** (`energy.slew(rise=0.2).clip(high=0.8)`), not as nested functions. *Reason:* a
  signal flows left to right through its shapers, so the code reads as the pipeline, and a method completes its keywords
  in an editor. `signal.pipe(shaper, **params)` puts any shaper, a plugin's included, into the same chain.
- **`lib` stays as the registry's uniform entry** (`lib.slew(signal, rise=0.2)`: tooling, plugins, `help()`), typed by a
  generated stub (`lib.pyi`), and is not taught as a second style. Metadata (description, parameters, summary) comes from
  `registry.get(name)`; `lib.<name>` is typed as a plain function for calling.
- **A generated stub plus a connected lookup, not `__getattr__` alone and not a layering exception.** A module-level
  `__getattr__` types nothing (mypy sees `Any`, no completion of `rise=`). The methods need to call the registered shaper,
  but `signal` is below `registry` and may not import it, and import-linter forbids sibling imports. So `Chain[S]`, the
  methods, sits in its own generated layer under `signal` (`rounding > _chain > formats`), generic in the signal type so
  that it imports nothing of `signal`; `Signal` inherits `Chain["Signal"]`; and the registry hands `signal` its by-name
  lookup when it is imported (`signal.connect_shapers`). No contract has an exception. `uv run poe api`
  (`tools/gen_api.py`) writes `lib.pyi` and `_chain.py` from what each built-in publishes, never from its code, and a
  test fails while either is stale: the second generated exception to open/closed after the gallery. A plugin's shaper
  has no method; it is reached as `lib.<name>` and through `pipe`. The generator refuses text that does not compile
  (summaries and parameter docs go into docstrings as written).
- **`from_values` and `events`.** A measured curve took three concepts (`from_series(Series(rate=..., values=...,
  unit=Unit.RATIO))`); the common case is one call, and `from_values` pairs with `from_series`, which stays for data from
  files. `events(times, label=...)` does the same for `Event(...)`, which still works. The names are the plain ones;
  `curve` and `signal` say less.
- **The Rust column is the target.** `docs/api.md` shows the Rust API R1 to R5 build to (parameters as a struct with
  `Default`, validated when the signal is built and returned as `Result`), so the Python API is not shaped by Python
  alone; the page says it is not implemented, and its Rust blocks become doctests as the crates land.
- **The Rust API rules** (`docs/api.md`) follow the Rust API Guidelines: a cheap-clone `Signal`, parameter structs with
  `Default` (the cost: a struct literal without `..Default::default()` stops compiling when a field is added, which is why
  the page always teaches it; `#[non_exhaustive]` with setters is decided before 1.0), one `#[non_exhaustive]` error enum,
  and a sampling API with an allocation-free form for a live loop. *Concurrency:* atomics are used only through `Arc`; the
  one mutable shared thing, the history cache of a stateful signal, is a `Mutex` over an `Arc` snapshot (a reader locks
  briefly, grows the cache if needed, clones the `Arc`, and reads without the lock). Miri checks `unsafe` code, and the
  core forbids it (the borrow checker and `Send`/`Sync` already exclude data races in safe code), so it is not run;
  `loom` only if lock-free code is ever hand-written, which it should not be. *The real hazard is a deadlock between a
  Rust lock and the Python GIL* (a signal written in Python is called from Rust): a Rust lock is never held while calling
  into Python; compute outside the lock, install the result under it, and let the loser of a race drop its copy. R3 tests
  this with threads and Python-written signals (below).
- **Patch files come last, and only through the public API.** A patch is a serialisation of the same graph; saving a
  code-built graph as a patch is R4's.
- **Costs of the method style.** (1) A built-in shaper's name is a reserved attribute name on every `Signal` subclass
  (`energy.clip` is the shaper): a plugin author keeps their own attributes private (`_name`); the generator refuses a
  built-in shaper named like an attribute `Signal` has already. (2) The chain methods take keyword parameters only. A
  future built-in shaper that takes an input such as `points` would need `_chain` to import the input's type from a layer
  above it, which the contract forbids; it must be typed differently (for example by taking its data through `pipe`).

## The formats in Rust (batch R1)

- **`serde_path_to_error` names the field of a wrongly typed value.** *Reason:* serde stops at the first value it cannot
  read and says only "invalid type"; the crate gives the path (`values[2]`) so the one problem such a value is can be
  listed like every other rule (`path: rule`). It has no dependencies of its own beyond serde and `itoa`, both already
  permissive and tiny.
- **Formats are read from JSON objects only, numbers strictly, and `serde_json` round-trips floats.** serde's derived
  `Deserialize` also reads a struct from a JSON array, field by field; the formats refuse that (`[0.5, null, ...]` is not an
  Event). A JSON integer beyond 2^53 and within 64 bits is refused, since an `f64` would round it to a neighbour; one beyond
  64 bits reaches `serde_json` as an already rounded `f64` and is accepted like a decimal (`arbitrary_precision` would close
  that gap, at a cost in every number read). `serde_json`'s `float_roundtrip`
  feature is on (workspace dependency): without it a decimal can read back one unit in the last place
  away from the `f64` that was written, found by the round-trip property test. *Cost:* slightly slower float parsing.
- **`schemars` generates the bundle schema from the structs that read the file.** *Reason:* the reader (`RawBundle`,
  `RawEvent`, `RawSeries`, `RawGrid`) is the one description of the file's shape, so field names, required fields,
  refused unknown fields and the name maps cannot drift from the schema, and no Python model is needed to generate it. The ranges a schema can state (`minimum`, `exclusiveMinimum`, `minItems`) are attributes on those fields, beside
  the checks that enforce them: two statements of one rule, which a test running both over the same documents keeps in
  agreement. Default features only (`derive`, `std`), no new transitive licence class (`deny.toml` passes). *Cost:* a
  proc-macro crate in the core's build.
- **The schema is stricter than its predecessor where that one was wrong.** Pydantic wrote `ge` (not a JSON Schema
  keyword, so ignored by validators) for `meter` and `first_downbeat`; the generated schema states `minimum`. A schema
  is generated for the file as it is written, so an optional field may be `null` and the name maps are objects only.
- **A rule across the document is listed with the item it is about, and `meter` is a `u32`.** Events sorted by `t`, `t`
  within the `duration` and a series' `start` within it are listed, all of them, after that item's own rules (the
  item's own rules still apply when it is broken: its `t` is read even if its `strength` is not). `Grid` holds `meter` and
  `first_downbeat` as `u32`; a larger `meter` in a file is a problem of its own (no real bar has four billion beats).
- **`Bundle::to_json` spaces JSON as Python's `json.dumps`** (`", "`, `": "`) and with `ascii_only` escapes as
  `ensure_ascii=True` does (UTF-16 units, DEL too), so a file written by either side is read by the other and the
  output of the old and the new writer can be compared. Floats are written by `serde_json` (shortest round-trip form,
  `1e22` where Python writes `1e+22`).
- **The formats are Rust-backed classes, not Pydantic models or wrappers** (owner, 2026-10-29). `Series`, `Event`, `Grid`
  and `Bundle` are classes of the extension, re-exported by `bullterrier_control.formats`. *Reason:* every rule is
  defined once, in Rust; the core loads, checks and saves a bundle with no Python (video's render worker, the web app); a
  Pydantic model, or a wrapper over a Rust type, would keep a second statement of each rule that can drift. *Cost:* no
  `model_*` methods, and the Python side reads Python objects with its own deserializer (below).
- **Every broken rule is listed, not the first** (owner, 2026-10-29). A `FormatError` carries a `Problem` (path and rule) per
  rule. *Reason:* a producer fixes a file in one pass. *Cost:* a reader cannot stop at the first problem; the rules
  that depend on a value that was not read (a wrong JSON type, an unknown field or unit, a non-whole number, an integer
  beyond 2^53) stop at that value.
- **The schema is generated from the Rust types** (owner, 2026-10-29), as the `schemars` entry says; no Python model is
  involved.
- **`_native` imports `errors`, lazily.** The extension raises `FormatError`, a Python class in `errors`, and imports it the
  first time it raises one. *Reason:* `FormatError` stays in the one hierarchy callers catch (`BullterrierControlError`).
  *Cost:* `_native` is no longer an import-free leaf; the layers read `formats > validated > _native > errors` (`validated` left in R4a), and the
  extension must not import anything else of the package.
- **`Unit` is defined once, in Rust, and the Python members are created from it.** `Unit.FRACTION`, ... are made from
  `Unit::ALL` when the extension loads; a member copies, pickles and hashes as itself. *Reason:* the list of units cannot
  differ between the schema, the core and Python. *Cost:* `Unit` is no longer a `str`, so `Unit.RATIO == "ratio"` is false:
  compare `.value`, or use the member. A constructor still accepts the text (`unit="ratio"`).
- **No `pythonize`: a deserializer over Python objects of about 300 lines in the bindings (`data.rs`).** The release of
  `pythonize` that fits the current `pyo3` is not out (it needs the previous one), and `deny.toml` still bans it. Going
  through `serde_json::Value` instead would not do: it cannot hold NaN, which must be listed as a problem beside the
  others, not refused alone first. *Cost:* the bindings carry the deserializer and its tests; it can be replaced by
  `pythonize` once a release fits.
- **What Python can hold that JSON cannot is refused or turned into JSON's form at the boundary.** A `bool` is not a number
  (also in a numpy bool array); a `str` with a lone surrogate is refused; an `int` beyond 2^53 is refused in a float field
  (the core refuses the same JSON integer, so two integers never read as one value), and one beyond 64 bits anywhere.
  numpy scalars are accepted as numbers. Any array is copied once at the boundary (`tolist()`), never read through
  a reference into the caller's memory: the lesson of R0, where a plugin kept and changed a view. *Cost:* one copy.
- **`Series.values` is a read-only float64 array over an immutable copy** (a tuple before), made once. *Reason:* a
  consumer reads the values as an array, and a Series is a value (`==`, `hash`) that must not change under its holder.
  `Bundle.series` and `Bundle.events` are read-only mappings in name order (events as tuples). *Cost:* an array view cannot be
  made writeable by the caller, only copied.
- **A refused format keeps at most 100 problems and counts the rest; the Python message shows the first 20 and
  `... and N more`.** `FormatError.problems` holds the first 100 (Rust: `FormatError::omitted()` counts the others), and N is
  the total minus 20. *Reason:* a list of millions of broken values must not cost millions of `Problem`s (gigabytes) to
  refuse; a message stays readable when a whole list of events is wrong. *Cost:* past 100 broken rules, the rest are known
  only as a count; fixing the first ones and trying again shows the next.
- **Wording and API changed from the Pydantic version** (a break, accepted before the first release):
  - Messages: a value is shown as JSON (`got -1.0`, `"a"`), type errors read in serde's words (`invalid type: string "a",
    expected a number`), a missing field reads `is required`, an event after `duration` is reported at
    `events.<name>[i].t`, and a grid conversion error at the argument (`positions[2]: must be finite`).
  - `model_dump`, `model_copy`, `model_validate` and `model_json_schema` are replaced by `to_dict()`, `Event.replace()`,
    `parse()` (a decoded mapping) and `bundle_schema()`.
  - `Grid.seconds_at(positions)` is renamed from `beats`, since a position is a fraction of a beat.
  - `Bundle(...)` takes `Grid`, `Series` and `Event` objects; a decoded mapping goes through `Bundle.parse`.

## The signal graph in Rust (batch R3)

Owner decisions (2026-10-31):
- **One batch.** *Reason:* the graph, `Rendered`, the numeric helpers and the Python switch only make sense together; a
  half-moved `Signal` would need a boundary of its own.
- **Pydantic leaves in R4, with the inputs.** R3 moves only easing, so `Keyframe`'s ease check calls Rust
  (`Ease::from_str`) and `Keyframe`, the inputs' models and `validated` stay pydantic. (Done in R4a, below.)
- **A polish task came first** (boundary minors carried from R1 and R2).

Rulings:
- **The `PARAMS`/`SHAPERS` split of `build.rs` stays until R5.** *Reason:* 19 built-ins still run in Python. A built-in
  file that defines `pub(crate) fn shape(input: &Signal, params: &Params) -> Result<Signal, Error>` is a shaper the core
  runs; `shape` replaces `run`. `slew` and `follow` define `shape` over `Signal::rendered` with a private `run`.
  `bullterrier_control::shape(name, &Signal, &Value)` replaces `render`.
- **`Params.__getattr__` waits for R5** (its readers are the Python bodies R5 deletes); **`gen_api`'s own range text,
  `inputs.py:127` and the O(n) problem strings in `formats.rs` wait for R4** (they go with `lib` and the inputs). Python
  that a later batch rewrites is not polished.
- **`Node` is sealed; `Signal::from_fn` and `Signal::rendered` are the extension points** (the Python binding, Rust
  plugins). A callback's error is kept as the `source()` of `Error::Node`, and the binding returns the original Python
  exception object unchanged. *Reason:* two constructors cover every outside node; a public trait would freeze the
  read signature before R4's `Source` and `Shaper` traits.
- **`_core.render` and `bullterrier_control::shapers()` are gone.** No shaper's input passes through Python sampling when
  the graph is Rust. A shaper is called through `bullterrier_control::shape(name, &Signal, &Value)`; Python calls one through the registry.

Design:
- **`MAX_DEPTH = 1000`.** A graph deeper than this is `Error::Depth` (an operator past it gives a signal that refuses
  every read). *Reason:* a bound on the memory of one read, not on stack ("The graph walks by loop, not recursion
  (batch R4c)"). A longer sum is a balanced tree. A callback node counts as depth 1.
- **Nested callbacks are capped at 48 per thread** (`SignalError` in Python), whatever kind ("The graph walks by loop, not
  recursion (batch R4c)"). Python-owned reference cycles that pass through a Rust graph (`self.x = self + 1`) are not
  collected.
- **GIL and lock rule.** `sample` releases the GIL, so a pure-Rust read lets other Python threads run. No Rust lock is
  held while calling Python: a `Rendered` locks briefly to clone or install its history, and renders (which may call
  Python) outside the lock. Two reads that need more history both render; the longer result is kept. *Cost:* duplicate
  work under contention instead of a deadlock; a shorter render finishing later never replaces the longer one.
- **A Python `compute` receives 1-D float64 times** (it used to get the caller's shape); `sample` restores the shape. Its
  call runs under `numpy.errstate(all="ignore")`, as the old `sample` did, so an overflow is a `SignalError` naming the
  time, not a warning.
- **`Signal` is not abstract.** A subclass without `compute` fails at its first read (`TypeError` naming the class), not
  when defined. *Reason:* the extension builds results (operators, `from_values`, `shape`) as instances of `Signal`, and
  Rust cannot honour an abstract-method check.
- **`_core.make_signals_as(Signal)`** (once per process) makes every signal the extension builds an instance of the
  package's `Signal`, so results carry `pipe` and the chain methods. *Reason:* a pyclass cannot inherit a Python mixin.
  The native base is listed first: `class Rendered(_core.Rendered, Signal)`, `class Constant(_core.Constant, Signal)`.
- **Numbers.** Easing, cadence, hashing, scaling and rounding are Rust (`crates/core/src/{easing,cadence,hashing,scaling,
  rounding}.rs`); the Python modules keep their docstring contracts and re-export. Hashing and the curve ends are
  bit-exact with the numpy they replace; cubic, overshoot and bezier agree within about 1e-12. The curves use explicit
  multiplication, no `powi` or `powf`, so every target gives the same bits.
- **A differential over 2000 seeded graphs** (old numpy graph against the new, report-only) found no regression. Leaves
  of constants, held series, hashing and the plain eases are bit-identical. A linear series read differs by one rounding
  of the operand scale in about 8 % of reads on arm64, where numpy's `interp` fuses the multiply-add and this core
  rounds twice. A bezier with a handle at exactly 0 or 1 drifts up to 1e-8 within 1e-6 of progress 0 or 1 (the
  bisection is ill-conditioned there). A `Rendered` read in the last allowed sample's span is now accepted (the old
  check was a sample stricter than the documented limit), and a plugin `compute` returning a wrong length is a
  `ValueError` where the old code returned a wrong-length array or an `IndexError`.
- **Messages differ a little from Python's** (`got NaN` against a list of values; `1e+300` written `1e300`); tests assert
  the error type and a short substring.
- **Timings** (report only, `bench_baseline.py` of R0, this machine, dev build): ten slews with an lfo mix took
  2267-4124 ms before R3 and 541-1064 ms after; a cached slew read per frame 15-28 us before, 1.4-1.7 us after. The
  envelope and lfo inputs are still Python and are sampled through the callback.

Breaking for plugin authors: a signal's `compute` gets 1-D times; a `Rendered` subclass with its own `__init__` signature is
not supported; native signals (`Constant`, `Rendered`, results of operators) cannot be pickled or
copied; `at_least`/`more_than` take a number bound; a missing `compute` is found at the first read.

## Inputs in Rust (batch R4a)

Owner decisions (2026-10-31):
- **The inputs (`points`, `events`, `grid`, `values`, `series`) are checked in the core, in a module `inputs`
  (`crates/core/src/inputs.rs`).** `InputKind` names a kind and checks data as it (`check`), `Input` is the checked form,
  `Keyframe` is a Rust type, and `matching` picks events by label. *Reason:* what an input is, and every rule that checks
  it, has one definition that Rust, the web app and the CLI share, as the formats and parameters have since R1 and R2.
  Python's `INPUTS` only names the kinds (docs, the stub annotation text, `check`).
- **Pydantic is removed**: the dependency, `validated.py` and its layer. A test fails if a source file imports it or
  `pyproject.toml` asks for it (`tests/test_no_pydantic.py`). It remains in `uv.lock` only as a development dependency of
  a packaging check. *Reason:* with parameters (R2), formats (R1) and inputs in Rust, nothing used it; the wheel's one
  Python dependency is numpy.
- **`Keyframe` is a Rust class of the extension** (frozen, keyword-only init, `parse`, `==`, `hash`, `repr`, pickles),
  like `Event`. *Reason:* the rule for one keyframe (finite numbers, `t` from 0, a known ease) is the core's.

Rulings:
- **A refused input is `Error::Input`, a list of problems (path and rule): the first 100 kept, the rest counted and
  shown as `... and N more`; Python raises it as a `ParameterError`.** *Reason:* the cap of the formats and parameters, so
  one rule everywhere. It also fixes the loss of "N more" that the Python re-wrap in `inputs.py` caused.
- **The items of a list are checked as they are read, not collected first.** A bad item is recorded, or discarded once the
  problems are full, and the next is read. *Reason:* memory stays bounded for a hostile input. Measured on 1M items
  (release build): `[true] * 1M` as `values` held 283 MB more than the input before, 0.5 MB after. *Cost:* `check` takes
  a value held in memory; a reader that streams JSON text cannot go on after a broken item (documented on `check`).
- **`check` is generic over a serde `Deserializer`, not `&serde_json::Value`.** *Reason:* a `Value` cannot hold NaN, which
  the Python reader delivers and which must be listed as a problem beside the others (as in "The formats in Rust").
  `check(&json!(..))` works as it would on a `Value`.
- **One set of strict number rules**: the formats' (`Number`, `field::{check, finite, required}`), also used by
  parameters: no `bool`, text or `null` for a number, NaN and the infinities refused, a whole number beyond 2^53
  refused. *Reason:* a duplicate list of rules (the Python aliases `Finite`, `Whole`, `Switch` and `Text`) can drift.
- **The Python reader (`data.rs`) reads the package's own objects as their file data**: an `Event`, `Keyframe`, `Grid` or
  `Series` in a list or field is read as the mapping of its fields. *Reason:* callers mix `Event` objects and mappings in
  one `events` list, and the same object must pass or fail the same rules as its file form. *Cost:* every reader of
  `data.rs` now accepts them: `Event.parse(Event(...))` and `Bundle.parse` with `Event` objects inside are accepted
  (they were refused).
- **An exception a user's object raises passes through unchanged** (a mapping whose `__getitem__` raises, an `__iter__` or
  `tolist` that raises): the first one is kept, nothing more is read, and it reaches the caller. *Reason:* it is the
  caller's own bug, not a bad value, as for a Python `compute` (R3). The formats and parameters still turn such an
  exception into a refusal message; the same pass-through there is a follow-up. An overridden `__iter__`,
  `__getitem__` or `items` of a list or dict subclass is not called: its contents are read.
- **Unchecked construction stays possible in Rust**: `Keyframe::at` and an `Input` variant built by hand skip the rules
  (documented). *Reason:* newtypes with private fields would add types for no consumer yet; R4b revisits it if sources
  take `Input` and rely on the promise.

Accepted differences from the Python inputs (a break, accepted before the first release; every path and verdict of
`tests/test_inputs.py` is unchanged):
- `ease: null` (`ease=None`) is linear, as a `null` event field is absent; it was refused. A `null` `t` reads as missing.
- A whole number beyond 2^53 (`values=[2**60]`, `t=2**60`) is refused; it was accepted, rounded.
- `np.bool_` in `values` is refused (it read as 1.0); a numpy array is accepted for `values` (it was refused). numpy
  scalars of every width are numbers; `longdouble` and numpy NaN are refused.
- A number must be a Python `int`/`float` or a numpy scalar of those kinds: `Decimal` and `Fraction` are refused (they
  were accepted through `float()`); `bytes` is not text for an ease; numpy arrays are read like their lists for
  `points` and `events` too; a mapping that raises while it is read passes its exception through in every input; the
  order check of `points` is reported beside other field problems (it was skipped when any field was invalid).
- A key that is not text is a `ParameterError` at its item; it was `TypeError: keywords must be strings`.
- A bezier ease is written back tidied (`" cubic-bezier( 0.25, 0.1 ,0.25, 1.0 ) "` reads `cubic-bezier(0.25, 0.1, 0.25,
  1)`); it was kept as written.
- Messages use the Rust wording (serde's type errors, `is not after the previous t = ..`). Every out-of-order pair is
  listed (not the first); a type error or an unknown field stops the reading of that keyframe or event, so fewer of its
  other problems are listed at once.
- A raising user mapping in `events` reaches the caller unchanged (it was wrapped as `ParameterError: events[0]: ...`;
  `points` already behaved so).
- `Event.parse(Event)` and `Bundle.parse` with `Event` objects are accepted.

## The registry and patches in Rust (batch R4b)

Owner decisions (2026-10-31):
- **R4 is two batches**: R4a the inputs (above), R4b the registry, `lib` and patches. *Reason:* the inputs are the first thing
  a registered source or a patch node holds, and each half is reviewable on its own.
- **Rust is the registry's authority.** One Rust `Registry` holds `Thing`s. Rust built-ins register themselves; Python
  built-ins and plugins register as callback things when their package is imported. The package's registry is a
  `_core.Registry` that starts from what Rust runs. *Reason:* a patch must be read, built and saved with no Python
  (`Patch::from_toml(text)?.build(&bundle)?`), and the name, clash, taken-name and call rules must have one definition that
  Rust, the web app and Python share, as the formats, parameters and inputs have. Python keeps the entry-point loading with
  its lock and the checks of a Python function's shape (its signature, its inputs, an operation's return annotation).
- **Two pilots**: `constant` (a source) and `select` (an operation with an `events` input) run in Rust. Their Python bodies
  are deleted. *Reason:* they prove the source and operation conventions and the inputs end to end; the other 17 are R5.
- **Save-as-patch covers graphs made through the registry.** `Patch::from_signals` (Python `Patch.from_signals`) saves
  signals made by a registered thing, each recording how it was made (below), and `to_toml` / `save` write them. A graph
  with an operator (`+ - *`, negation, `minimum`, `maximum`), `from_values`, `from_series`, a `Constant` made in Python, a
  `compute` of its own or a `Rendered(run=...)` is refused, naming the path (`glow.signal`) and why, up to 100 listed and
  a count. An operation's result is plain data, so a saved patch writes it inline where a later node uses it (its recipe
  is not kept). A graph built from a bundle's `series.<name>` by a patch cannot be saved: keep the patch that read it.
  *Reason:* "a signal is what its things made" is exact and has no operator nodes, which would be a patch-format change
  (R5 territory). A bundle series has no source that reads it, so the refusal says to keep the patch.

Rulings:
- **The registry is a value, not a global.** `Registry::builtin()` is an immutable `&'static Registry` of what Rust runs;
  `Registry::new()` and `Clone` give one to register in. The package owns one (`registry.table()` gives a copy, so the
  decorators stay the one way in) and `isolated()` swaps an empty one in. *Reason:* no global mutable state, safe on wasm32, tests need no teardown. The binding's registry
  changes by copy and swap under a short lock; no lock is held while a callback runs.
- **One door, and `const NAME`.** Every way of making a signal from a thing (`Thing::call`, `shape`, `Signal::pipe`, a typed
  method such as `Signal::slew`, a `lib::` function) goes through one private call that checks the call, reads the values
  through the table, runs the body, checks the kind of the output, names errors with the thing's name and records the
  recipe. A Rust thing has one name, `const NAME` of its trait; a built-in's file name is proven equal to it at compile
  time. *Reason:* before, the same slew through each door gave a different recipe and a different name in its error, and a
  graph built with `pipe` or a typed method could not be saved.
- **The recipe lives in a wrapper node.** `Recorded { recipe, inner }` passes depth and values through; `Signal::recipe()`
  reads it. *Reason:* a field on every node would touch every constructor. Its depth counts through the signals a call
  holds (a body that gives its input back, or ignores it, adds no node of its own), so a chain of calls is bounded by
  `MAX_DEPTH`; unbounded, a 100k chain overflowed the stack on read and on drop. Its `Debug` is the thing's name and the
  depth, not the signals, which doubled per level ("Resource gates"). Cost: about 3 ns a read per level, within noise on a batch read.
- **Traits make the params struct the thing.** `Source`, `Shaper` and `Operation` are implemented for `Params`, so
  `glow.pipe(Fold { depth: 0.5 })?` is the call. `Thing::source::<T>`, `shaper::<T>`, `operation::<T>` and `Thing::custom`
  (a callback) make a `Thing`; `Registry::register(Thing)` takes it. *Reason:* Rust cannot bound `T` by "one of three
  traits" for a `register::<T>(name)`, and a thing's doc and parameter title are not on the trait. Inputs on all three
  kinds: "Built-ins with data in Rust (batch R5a)".
- **`build.rs` finds the impl in the code, not the text.** A file with `impl Source|Shaper|Operation for Params` is registered
  (a source is also a `lib::` function); a token scan skips comments, docs, strings, characters and `macro_rules!` bodies
  and accepts the trait by any path. *Reason:* a plain text search registered a marker in a comment and missed
  `impl crate::Shaper`. This replaces R3's `pub(crate) fn shape` convention and its `SHAPERS` list: `PARAMS` still lists all
  21 and `register` the ones the core runs, until R5.
- **Patches are read in Rust, in order.** `Patch::parse` takes the entries as an ordered slice, not a `serde_json::Value`
  (the core's serde_json sorts keys, and "a node above" depends on the order); `from_toml` is `parse` after the TOML
  reader. The thing a node names is looked up once, when the patch is read, so there is no `build_with`: a second registry at
  build would be a second source of truth. A node is where the text first names it, even in a sub-table header
  (`[b.x]` above `[a]` names `b` first, as Python's `tomllib` ordered it).
- **`toml` is the TOML dependency**, `parse` feature only (no serde, no writer, no indexmap). *Reason:* a patch is TOML, the
  parser is pure Rust and builds for `wasm32-unknown-unknown`, `cargo deny` and `shear` pass, its errors carry line and
  column, and its key spans keep the file order. The writer is ours (`patch/text.rs`), so the output is stable and each
  decimal is written as the same bits.
- **A TOML syntax error's detail is the Rust parser's**: `not TOML (line 1, column 10: string values must be quoted, ...)`;
  Python's reader added the path (`{path}: not TOML (...)`). Other patch messages keep their words (`<node>.<key>: rule`),
  except that a value, reference or list of nodes above longer than 80 characters is cut with `...`, and the "N more" count survives:
  a patch that breaks more than 100 rules says how many were cut, including those an input or a parameter table cut
  (R4a left this lost).
- **Events a call gives are in time order, checked by the core's door.** An unsorted result from any thing, Rust or Python, is
  `Error::Output` (a `ValueError`, naming the thing and the event); it is refused, not sorted. *Reason:* `Output::Events`
  promises time order, and a patch built on a body that broke it gave the wrong events silently.
- **`register` and `from_signals` take each fact once.** The binding's `register(function, *, kind, params, inputs,
  returns)` reads the table from the `Params` class (a non-`Params` class is a `TypeError`); `from_signals(items, *, registry)`
  requires its registry, and Python's `Patch.from_signals(signals)` passes the package's. *Reason:* a `params` and a `table`
  that disagree registered silently, and a default registry names the wrong one for every plugin.
- **The privacy rule.** The `patch` module names only public items of the other modules, checked by a scan of its source
  (`patch/privacy.rs`, with known-bad rows), and Python's `patch` imports only `registry`, `formats`, `errors` and `_native`
  (an import contract). *Reason:* API first: a patch that needed an internal would show the public API is not enough. Cost:
  two small helpers are repeated in the patch module (quoting a value, writing the problems), and the scan sees paths, not
  methods.

Accepted differences from the Python registry and patch (a break, accepted before the first release):
- `Registered` is the core's `Thing`: not generic (`Registered[P, R]` is gone), and a Rust built-in has `function` and
  `params_model` `None`. `registry.NAME_PATTERN` and `registry.NODE_KEYS` are gone (the core has them); `registry.table()`
  is new. `description()["inputs"]` stays in declared order.
- A call returns the core's recorded `Signal`, never the body's own object: `isinstance(level(), Constant)` is false for a
  registry result. A Python body gets a new handle over the caller's signal (the same values, not the caller's subclass or
  object), and an operation's events arrive as new `Event`s. A body that gives the wrong kind of result is a `TypeError`;
  Python passed anything through. An exception a body raises reaches the caller unchanged.
- A `Signal` subclass that defines a built-in shaper's name (`slew`, `clip`, ...) is a `TypeError` at class creation.
- A top-level patch key that is not text is read as its `str`, a key inside a node must be text; a NaN or infinity reads
  `must be finite (got nan)`; problems within a node list unknown keys first (by name), then the parameters in table
  order.
- `bt-control list --json` is byte-identical to before. `gen_api` takes each range from the core's table (the noise seed now
  reads `0 to 4294967295`, not `4.29497e+09`).
- The shaper scaffold writes no Python module: the Rust file registers itself.

Cost: ten independent slews through the registry take the same time as before R4 within noise (about 20 ms a run, 24 ms
the first run of a process); a `lib.slew` call costs 2.2 µs (it was 5.0 µs), a cached read 2.0 µs, parsing a 100-node
patch 0.4 ms (it was 2.1 ms). A Python-bodied built-in call costs about 3 µs more than before (it goes through the Rust
door and back); the 17 that remain in Python are ported in R5.

## The graph walks by loop, not recursion (batch R4c)

- **The problem.** A graph is read and dropped by recursion, and `MAX_DEPTH`, a limit in nodes, was a promise about bytes of
  stack: the cost of a level was about 80 B for a sum in a release build, 380 B in a debug build.
  A chain of 999 slews crashed a 256 KiB thread, and a debug build overflowed a 2 MiB one on 1000. The same limit meant a
  different number of bytes on every target and build.
- **A node reads in two steps, one loop reads the inputs.** A node has `start(times, out)`, which writes its values or names
  the inputs it needs (`Next`), and `finish(times, inputs, out)`, which combines them. `walk::read`
  (`crates/core/src/signal/walk.rs`) is the only code that reads an input: a depth-first loop over three stacks on the heap
  (frames, grids, buffers), reading, rendering and allocating in the order the recursive read did. The values are bit for bit
  the old ones (a differential over 2000 seeded graphs, every node kind, compared the bits of every value). A scan test
  (`signal/walked.rs`) fails if any other file under `signal/` names a read of a signal, or an `impl Node` sits in a file it
  does not scan, because exposing only `start` and `finish` does not stop a node holding a `Signal` and calling it.
- **A drop is a loop too.** Every last handle of a node goes to a per-thread queue: the first drop on a thread runs the loop,
  and a drop inside it only queues. The queue is a `const` thread local in `ManuallyDrop` with no destructor, so it is never
  torn down: a graph another thread local holds is still dropped by the loop when the thread ends, whichever goes first (the
  first cut recursed there: 999 sums on a 64 KiB thread died in a thread-local destructor). A second thread local frees the
  queue's buffer at thread end. No node declares what it holds, so none can declare it wrongly.
- **Not `stacker`** (grow the stack on demand). *Reason:* it needs a C compiler and assembler at build time, brings 7 crates,
  was unverified on Windows and Linux, drops CPython 3.14's stack guard on the segments it grows, and cannot grow on
  wasm32 (a graph trapped at about 4,400 deep in node). Its cost, measured in the same harness: a read 2.7 ns a node more
  (as the loop's), a drop 26 to 80% more (the loop's: "Cost" below).
- **Not a per-node stack budget** (count each node by its weight). *Reason:* it is a table of measured weights per target,
  right until a node or a compiler changes.
- **The one cap that remains: callbacks.** A `from_fn`, a recurrence's `run`, a thing's body, and so a Python `compute`,
  `run` or body, is user code that may read a signal or call a thing: a new walk on the thread's stack, with user frames
  between two levels, which no loop can take over. Callbacks nested more than 48 deep on a thread, of every kind together,
  are refused with `Error::Depth` (`SignalError` in Python). The count stands for bytes: measured in a release build on
  macOS arm64, a level takes about 815 B for a Rust `from_fn`, 3,150 B for a Python `compute` and 4,000 B for a Python
  body, and a thread needs about 8 KiB before the first, so 8 KiB + 48 x 4,000 B = 196 KiB fits 256 KiB with about 60 KiB for
  the caller. 64 levels left 6 KiB. Not measured on Linux or Windows; a callback that recurses through C (a deep `repr`,
  numpy internals) takes more a level and is not covered.
- **The smallest supported thread stack is 256 KiB.** A Python thread on macOS gets 512 KiB by default. musl's 128 KiB is not
  supported: nested Python callbacks do not fit 128 KiB or 64 KiB (64 nested crashed both). Graphs of Rust nodes (a sum, a
  slew) walk on 32 KiB.
- **`MAX_DEPTH` stays at 1000 as a bound on the nodes a read holds, `MOST_HELD` on the samples.** A read holds a frame (64 B
  for a sum) for each node on the way down, and a recurrence a grid and a buffer of at least 1024 samples each; a
  100,000-deep chain of slews takes 1.7 GB (16.9 KB a level). A recurrence holds two samples per sample of its time, so
  at a far time depth alone bounds nothing: one slew read at 18,000 s peaked at 320 MB, 30 stacked ones past 4 GB. The
  walk refuses a read that holds more than 2^26 samples (512 MiB) across its graph with `Error::Signal`
  (`walk::MOST_HELD`), checked before the buffer is allocated. *Reason:* the `limit` of a `Rendering` is per node, so a
  patch of stacked shapers multiplied it by their number. A nested callback's read has its own budget (48 deep at most). The limit was raised to 100,000 for the experiment only, on a 64 KiB thread: sums, recurrences and
  slews read and dropped. On wasm32 a graph of Rust nodes needs no depth budget (99,999 sums read and dropped in node);
  only the callback cap applies, the same on every target. It does not bound time: a read visits a node once for each
  path to it, so a node shared at every level (`t = t + t`) doubles the read a level (22 levels: 0.1 s), as before R4c.
- **Cost against the recursive read** (release, macOS arm64, median of 15 interleaved runs at a load near 6 to 8; runs at
  higher load discarded): a read costs about 2.7 ns a node more (100 sums at one time: 615 to 884 ns; at 1000 times,
  +3%); a Python compute read, noise to +5%; on wasm a read of 10 to 100 nodes about twice. A drop, at a load under 4
  (median of 9, two interleaved rounds): +15 to 40% for one or 10 sums, -10 to -20% for 100 or 999, +10 to 55% for a
  balanced tree of 1023 nodes; 8 threads dropping at once as fast as the recursive drop (0.8 µs for 10 sums). The node a dropped handle holds
  while its node drops is one a thread: one for all made that 7 times slower.
- The wheel check of the deepest chains on a 256 KiB thread is in "Resource gates (R4c, owner, 2026-11-01)".

## Built-ins with data in Rust (batch R5a)

- **`type Inputs` on all three kinds.** `Source` and `Shaper` take it as `Operation` does: one mechanism, and the door
  records what `Inputs::into_inputs` gives, so a recipe cannot miss an input.
- **`pipe` and `pipe_with`.** `pipe(params)` is bounded to `Inputs = ()`; a shaper with inputs is `pipe_with(params,
  inputs)`. *Reason:* a shaper that needs data cannot be piped without it, at compile time.
- **`lib::` argument order** is api.md rule 2: borrowed required inputs first, then the parameters, optional inputs last;
  a fieldless source (`struct Params;` or `{}`) takes no struct. *Reason:* the call reads like the Python one, and a
  struct with nothing to set is noise.
- **`Signal::retimed(map)`** reads its input at the times `map` gives. `delay` and `hold` are built on it. *Reason:* a
  `from_fn` reading its input is a nested walk, capped at 48; `retimed` hands the times to the one loop, so its `map` is a
  callback that reads no signal and a chain of 999 delays reads.
- **`Signal::mapped(f)`** replaces each value by `f(value)` at the same times. `remap` is built on it, its ease parsed
  once when the shaper is made. *Reason:* the walk reads the input in place and `f` runs over it, so a chain of 999
  reads on a small stack. `f` is still counted as a callback (once a read): it may read a signal it holds, a walk of
  its own; uncounted, 100 000 such nested reads overflowed a 256 KiB stack.
- **libm for exp.** `follow` calls `libm::exp` (the `libm` crate, MIT, pure Rust, pinned exact), not std's. *Reason:*
  std's `exp` calls the platform's math library, so the same signal could differ by an ulp between targets (rule 6
  allows float tolerance across platforms, but the core's easing and hashing already give the same bits everywhere, and a
  patch's hash and a rendered file should too). Over 10 000 seeded arguments the two differ by at most 1 ulp. Later
  built-ins that need `sin`, `exp` or `powf` use `libm` the same way.
- **Ported against the Python bodies.** A differential harness (not committed) compared each port with the body it
  replaced; all bit-identical: `delay` 351 cases, `hold` 370, `latch` 398, `every` 408, `thin` 385, `shift` 385, `beats`
  376, `quantize` 570, `normalise` 924.
- **`latch` switches at plain `>=`**, as the Python body did, not `rounding::at_least`.
- **`normalise` leaves the sign of an output zero unspecified.** From 5 values it may differ from the old body: numpy's
  partition order of equal zeros is not reproduced; values never differ. Its percentile follows the installed numpy
  (2.5.4), whose `_lerp` has no `a == b` snap.
- **A retiming adds 2 samples per read time to `MOST_HELD`** (2^26 across stacked shapers), which the Python `compute`
  bodies never counted. 3 600 000 times through 9 delays read; through 10, `SignalError`. *Reason:* the R4c rule applied
  to memory always allocated, now counted. Intended.

## Resource gates (R4c, owner, 2026-11-01)

Unbounded memory, unbounded output and stack overflow are impossible by design where the design can make them so, and caught
where it cannot by hard gates in the repository: each runs on a bare `cargo test` or `pytest`, in `poe check` and in CI, and
has a known-good and a known-bad test. *Reason:* runaways froze the machine (one test binary reached 43 GB); a personal
script or a note does not run for the next contributor.
- **`Debug` is a label.** `Node` does not require `Debug`; each node gives `label()` (its kind, or the name of the thing that
  made it), and the one `Debug for Signal` prints that and the depth: `Signal { node: "slew", depth: 3 }`. No node can print
  its inputs, so a derive over any type holding a signal (`Recipe`, `Output`) is bounded per level. Before, each node wrote
  its own, and one printing its signals doubled per level. Test: the deepest chain of every node kind formats in at most 48
  bytes a level.
- **The core's test binary caps its memory at 4 GiB.** Its global allocator (`#[cfg(test)]`) is the `cap` crate's, a
  dev-dependency (MIT OR Apache-2.0, no dependencies, one file), so the core keeps `forbid(unsafe_code)`; an allocation past
  the cap is refused, which aborts the binary ("memory allocation of N bytes failed"). A whole run peaks near 140 MB. Test:
  the binary runs itself with the cap lowered to 64 MiB in the child, which asks for twice that.
- **The installed wheel walks the deepest chains on a 256 KiB thread.** `tools/check_installed.py`, run by CI's `build` on
  every platform's wheel and on the sdist, grows each chain of `CHAINS` to the deepest the package accepts and walks it on a
  thread of 256 KiB, each in its own Python, so a crash is a failure naming the chain; `tests/test_depth_stack.py` runs the
  same on the development install.
- **A gate that passed is not repeated on the same tree** (2026-11-01). `poe check` notes the tree of `HEAD` when it
  starts and, when every step passed, records it in the git folder if the working tree was clean at both ends and `HEAD`
  was unchanged; the pre-push hook skips the gate for a tip with that tree. *Reason:* the hook repeated a minutes-long
  gate on contents just checked, and a slow loop gets worked around. *Consequence:* nothing is skipped that was not
  checked; an edit, a commit during the run or any uncommitted file means no record, and CI checks everything again.
  *Not seen by the record:* state outside the tree (installed cargo tools, the advisory database, the built extension).
- **Not guarded:** doctests (binaries of their own) and the test binaries of the bindings and macros crates (uncapped), and
  memory on the Python side (no incident yet: a watchdog comes the day one happens).
