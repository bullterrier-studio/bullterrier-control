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
- **matplotlib is an optional extra (`plot`), imported when a picture is drawn.** *Reason:* control installs with numpy and
  pydantic only; a picture is a convenience of the command line and the gallery. Without it `plot` is a
  `BackendUnavailableError` (as in image) naming the extra. A PNG carries no version or date, so a rerun writes the same
  bytes. matplotlib writes its font cache with the locale's encoding when first imported, which the warning gate reports,
  and before 3.10.7 it calls pyparsing names that pyparsing 3.3 deprecates (the floor, 3.8, fails the lowest-dependency
  run at import). Both warnings are exempt in `filterwarnings`, only from matplotlib's own modules: the floor stays at
  what the code needs, not at what keeps a third party quiet.
- **The gallery is one patch per built-in source and shaper, a picture of each, and one generated page.** The patch is
  `examples/gallery/<name>.toml` (its node `<name>` is the thing, its other nodes feed it), drawn over four seconds into
  `docs/gallery/<name>.png`; `poe new-source` writes the patch. No picture is compared (it depends on the drawing library);
  tests check that every built-in has a patch that builds and moves, a picture exists, and the page's text is current.
  *Cost:* `docs/gallery.md` lists every source, so adding one modifies it (the exception to rule 4), and two branches that
  add sources conflict in it; regenerating it resolves that. Operations have no picture: they give no signal.

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
  numpy and pydantic.
- **One generic boundary, `_core.render(name, inputs, params)`, and it is transitional.** It takes the shaper's input
  sampled at its rate from time 0 and returns a series of the same length; it releases the GIL while it runs and the
  bindings name no shaper, so porting one never changes them. It serves only shapers that need history, which is all R0
  ports; it goes away in R3, when `Signal` and `Rendered` themselves live in Rust. A wrong name, parameter set, dtype or
  shape is a programming error (`ValueError`): a user's values are validated before they get here.
- **`Rendered` promises its `run` a fresh 1-D, C-contiguous float64 array.** *Reason:* the core reads the input in place,
  without the GIL; if each recurrence had to cope with a float32, strided or misaligned array, or the core copied it, the
  cost and the bug would be repeated per shaper. Fresh (always a copy, owned and aligned) because a plugin's `compute` may
  return a view it keeps and changes from another thread. `Rendered` makes the copy once, in the one place a recurrence is
  called.
- **A NaN in a shaper's input comes through the kernel.** A Rust kernel never panics on it; `sample` reports it as a
  `SignalError`, as for a Python recurrence. *Reason:* the signal contract is kept in one place (`sample`), not in each
  kernel.
- **`_native` is the lowest layer; built-ins may import it directly.** It is a package of its own at the bottom, beside
  `errors`, and imports nothing from `bullterrier_control`: the import linter cannot read a compiled module but sees the
  package. Any layer may therefore call it. The contract "Built-in sources use only the author API" leaves it out of its
  forbidden list, an explicit exception to the author-API door (a built-in would otherwise reach it only through `plugin`).
  *Reason:* a built-in runs its body in Rust, and `plugin` stays what a third-party author needs. Today only `slew` and
  `follow` call it.
- **Pydantic stays the one declaration of parameters until R2.** The Python shaper keeps its model, docstring and
  `@shaper`; its body hands `params.model_dump()` to the core. The Rust `Params` has the same fields, no defaults, and
  refuses unknown ones, so a struct that drifts from the model fails the first call naming the field. *Reason:* units,
  ranges, `help()` and `list --json` already come from the model, and the core gets values already checked and trusts
  them. Declaring parameters in Rust is R2. Vulture cannot see that the core reads a field by name, so `poe dead-code`
  writes a whitelist of the fields of the shapers `_core.shapers()` names when it runs (porting edits no shared file),
  and rustc's dead-code lint checks the Rust side.
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
  forces a floor, so it is 2_28, the one bullterrier-image's wheels have; a musl or Windows arm64 wheel waits for
  someone to ask.

## API first (owner, 2026-10-28)

- **The programmatic API is the product, in Python now and in Rust once ported; patch files are a serialisation of the
  same graph, built only through that API.** A patch is a nice extra, never a feature the API lacks. *Reason:* the API
  should be extremely clear, typed and discoverable in an IDE, and a file format that can do more than the API says the
  API is missing something. *Consequence:* batch R0.5 (`docs/roadmap.md`) specifies the API in Python and Rust side by
  side and improves the Python one; every batch from R1 on meets it; R4 enforces "the patch uses only the public API" with
  an import contract in Python and module privacy in Rust.
