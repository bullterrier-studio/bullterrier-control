# CLAUDE.md: bullterrier-control

Python library and CLI that produces **control signals**: numbers that change over time and drive the parameters of anything. It is
what DAWs call automation, animation tools keyframes and synthesizers modulation. `README.md` (usage), `docs/ecosystem.md` (the
five packages and their rules), `docs/design-decisions.md` (why), `docs/roadmap.md` (what is next), `docs/formats.md` (the files).

**Write docs tersely.** This file, the README, `docs/`, docstrings, reports and briefs: state the rule or fact, drop preamble,
repetition and justification a reader does not need to act. Reasons go in `docs/design-decisions.md`, once. Committed files are
public: no absolute paths, no references to folders outside the repository.

## Start here

```bash
uv sync && uv run poe setup          # once per clone: environment, git hooks, commit template
uv run pytest tests/builtin -n 0     # the tests near your change, while you work
uv run poe check                     # the whole gate, before you report or merge (then `uv run lint-imports`)
```

| To ... | Look at |
|---|---|
| use a source or shaper | `lib` (`lib.slew(signal, rise=0.2)`; `dir(lib)`, `help(lib.slew)`) |
| add a source or shaper | "Adding a source or shaper" below; one module in `src/bullterrier_control/builtin/`, one test file in `tests/builtin/` |
| see what every source and shaper promises | "The signal contract" below; `tests/builtin/test_contract.py` checks it over all of them |
| declare parameters | `params.py` (`Params`, `param`); the rules in "Parameters" below |
| write a signal that needs history | `rendered.py` (`Rendered`) |
| read or write a file | `formats/` (`load`, `save`, `Bundle`); `docs/formats.md` |
| see a worked example with numbers | `docs/examples.md` and `examples/` |
| find what is planned | `docs/roadmap.md`: work goes in small batches, each planned before it starts |

**State:** the formats, the core (signals, `Rendered`, parameters, registry, `lib`) and the sources `constant`, `clip` and `slew`
exist. Next: batch C2a (`docs/roadmap.md`). Maintainers may keep machine-specific notes in a git-ignored `.local/` folder (for
example `.local/prior-work.md`, earlier experiments worth reusing) and batch plans in the git-ignored `.superpowers/plans/`: read
them when present; never commit or cite them.

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
| The same result for the same inputs and seed | Drawing or rendering anything → image, vector, video |

**Measures vs shapes:** audio *measures* ("a kick at 1.20 s"); control *shapes* ("on each kick, jump to 1 and release over 0.3 s").
Test for where a number belongs: would it change if the look changed? If yes, it is control's (smoothing times, ranges, gates).

**No sound, no pixels, no frames:** time is in seconds everywhere. No fps, frame number, sample rate of audio, or file format of a
picture or sound in code, parameters or docs. A reader asks for a value at a time; the consumer chooses the times. Musical time is
a parameter unit, not a second clock: a value in `beats` is turned into seconds through a Grid.

**Formats this package owns** (plain data, documented and versioned here; changing one is a breaking release): **Series** (values at
a fixed rate from a start time, with a unit and a documented scaling), **Event** (a moment in seconds, a strength, an optional
duration, label and pitch: one type for onsets, beats, notes and sections), **Grid** (beat times, meter and first downbeat;
converts beat positions and seconds), carried in one file, the **Bundle**; the rules are in `docs/formats.md`, the machine-readable form is
`schemas/bundle-1.schema.json`. `bullterrier-audio` writes them **without importing this package**:
the schema is a file here, audio checks its output against a copy. A format is never shared as code.

## Names

`bullterrier-control` (PyPI, repository), `bullterrier_control` (import; docs alias `btc`), `bt-control` (command, `bt-control.toml`,
cache folder), `BT_CONTROL_*` (env vars), `BullterrierControlError` (base exception), `bullterrier_control.sources` (entry-point
group for packages of sources and shapers). Full name for identity, `bt-` for what a person types. Names inside the package
(`Series`, `lib`, sources, shapers) carry no prefix.

## Architecture

Layers, imports flow **downward only** (import-linter: contracts in `pyproject.toml`, run by `tests/test_layering.py` and
`uv run lint-imports`). A forbidden import means the code is in the wrong layer: move it.

```
app        __main__, cli
builtin    the built-in sources and shapers: one module each, never importing each other, using only `plugin`
lib        every registered source and shaper by name (`lib.slew`), with `help()` built from its parameters
plugin     the author API: everything a source needs (built-ins use it too); the one door, enforced by a contract
registry   the table of sources and shapers; `@source`, `@shaper`; entry-point loading
rendered   Rendered: a signal that needs history, rendered once and read by holding the latest sample
signal     Signal, Constant, from_series, arithmetic  |  params   Params, param, UNITS
formats    Bundle > Series | Event | Grid > the format base and Unit
validated  ValidatedModel and the strict value types (Finite, Whole, Switch, Text)
errors     BullterrierControlError > InvalidError > FormatError | ParameterError; SignalError; UnknownNameError
```

A source is `def name(params) -> Signal`, a shaper `def name(signal, params) -> Signal`, each registered with `@source` or
`@shaper` and its `Params` model; packages are entry points of the group `bullterrier_control.sources`. Pipes-and-filters: a
shaper maps a signal to a signal. Each thing arrives with its first consumer (`vulture` fails on unused code).

## The signal contract

Every signal, and so every source and shaper, promises this (`tests/builtin/test_contract.py` checks every registered one):
- **Total:** a value at every time. A time before 0 reads as time 0; a Series-backed signal holds its first and last value.
- **`sample(times)`** (final: never overridden) takes a number or an array of any shape, refuses what is not a finite number
  (text, a bool, NaN, an infinity) with `SignalError`, and returns float64 in the same shape. A signal implements
  `compute(times)`, which only ever sees finite times from 0 up.
- **Finite:** a value that is not finite (an overflow) is a `SignalError`, never returned. At any time a signal gives a finite
  value or raises `SignalError` (a stateful shaper refuses a time that needs more samples than its limit).
- **Deterministic and read-independent:** the same times give the same values, whatever the order, size or number of earlier
  reads. A stateful shaper is a causal recurrence run through `Rendered` (output *i* depends on inputs 0 to *i*): never a
  field that remembers the last read.
- **Causal:** a read at time t depends on the input only up to t (a float rounding of a billionth of a sample aside). What
  comes later in a shaper's input never changes an earlier output.
- **Parameters declared** with `param` (unit, range, description): every number works at both ends of its range and is refused
  beyond it, `true` or a string is not a number, an unknown name is refused, and the refusal names the field.
A new source or shaper meets all of this by registering: the contract tests find it and no test names it.

## The rules

1. **Never import audio, image, vector or video.** Nor optionally, nor lazily. A format shared with a sibling is a plain data file
   owned by one package, not code.
2. **Sources and shapers never import each other.** Shared numeric code goes to a module named for its concept (`interpolation`,
   `smoothing`, `noise`), never `utils`/`common`/`helpers`. Extract when a second consumer needs it.
3. **Parameters are declared once** as a pydantic model (as in bullterrier-image), with unit, range and description, and drive
   validation, defaults and generated docs. Units include `seconds`, `beats` (through a Grid), `hertz`, `cycles`, `fraction`.
4. **Open/closed:** adding a source or shaper creates files named for it and modifies none.
5. **No hidden tunables.** Every value that shapes a result is a named, documented parameter in the field's own terms (attack,
   release, rate, depth, phase, slew). Exempt: mathematical and standard constants, each cited in a comment.
6. **Determinism:** same inputs and `seed` give the same signal on the same machine and library versions; across platforms equal
   within float tolerance. `numpy.random.default_rng(seed)`; no global random state. Reading a signal at a time never depends
   on what was read before.
7. **Layering and the contract are tests**, run over every source and shaper automatically.
8. **Errors:** callers catch `BullterrierControlError` (`bullterrier_control.errors`). Library code raises a subclass, never a bare
   `ValueError`, for anything a caller or an input file can get wrong; each also inherits the matching builtin. A bug in the
   calling code (a wrong registration, a wrong argument to `Rendered`, a wrong call shape) is a plain `ValueError`/`TypeError`.

## Standards

- `src/` holds only code and the data it runs on. Tests, examples and docs live outside it.
- Public API has `__all__`: `bullterrier_control`, `bullterrier_control.plugin` (for authors) and `bullterrier_control.registry`
  (queries). Everything else is internal.
- Runtime dependencies: numpy and pydantic. Adding another needs a reason in `docs/design-decisions.md`: control must
  install without librosa, torch, Pillow or any media library.
- Offline only in v1: no live clock, MIDI or OSC output.
- No backwards-compatibility shims, aliases or re-exports before 1.0 unless the owner asks: change the callers.
- The CLI catches `BullterrierControlError` and never prints a traceback for a user's mistake (added with the first command that can
  raise it: no `try` for an error nothing raises).
- The library never prints and never configures logging.
- Text is UTF-8 (`encoding="utf-8"` on every open); paths are `pathlib.Path`.
- Security: ruff `S`, `PGH`, `LOG` run in `poe check` (a subprocess call carries `# noqa: S603  <why its argv is safe>`).
  `uv run poe audit` (pip-audit on `uv.lock`, needs network) runs before a release and in CI, which uses `uv run --locked poe check`.
- Versions are SemVer (a 0.x minor may break the API), changed only by `uv run poe release VERSION` (the owner picks it: checks,
  shows the plan, asks, bumps, writes CHANGELOG.md with git-cliff, commits, tags, pushes main and the tag; `docs/releasing.md`).
  Tests read the version from `pyproject.toml`, never write it.
- CI: one workflow per concern in `.github/workflows/`, joined by `ci.yml`; checks run in parallel, then `build`, then
  `release.yml` publishes only what that build made. `ci-ok` is the only required check: a new job goes into its `needs` (a new
  check also into `build`'s). Actions are pinned by commit SHA, jobs hold minimal permissions; `uv run poe lint-workflows`
  (in `poe check`) enforces it. The workflows are linted offline; they have not run on GitHub until the repository exists.
- The gates are the review: reviewers check behaviour and these rules only. A defect a review finds twice becomes a gate.
- Design decisions and reasons go in `docs/design-decisions.md`, not here.

## Parameters

A pydantic model per source or shaper, a subclass of `Params` (as `EffectParams` is in bullterrier-image: frozen, `extra="forbid"`,
strict whatever the annotation), each field `param(default, doc=...)`. Every field has a default and a description (what it does,
unit and range in words, what raising it does). A field is a number, a `bool`, a `str` or a `Literal` choice of strings, nothing
else; the declaration is checked when the class is made.
- **Numeric** (`int`, `float`): also a `unit` (`seconds`, `beats`, `hertz`, `cycles`, `fraction`, `decibels`, `ratio`, `semitones`,
  `count`) and inclusive `min`/`max` that hold the default. A parameter in `beats` is turned into seconds through a Grid; the model
  declares the unit, the reader converts.
- **Other** fields take no unit/min/max: `bool`, `Literal[...]` (the doc mentions every choice), `str`, lists of those.
- **Basic and advanced:** `advanced=True` marks a field that tunes the technique rather than chooses the shape; listed apart, still
  settable and validated. Aim at six or fewer basic fields.
- **Seeded where random:** a source that uses randomness has a `seed` field; none reads global state.

Use the field's own terms (attack, decay, sustain, release, rate, depth, phase, slew, threshold, hysteresis, hold, ease, overshoot,
swing, seed, offset), never `strength`, `intensity`, `factor`, `val`, `k`, or the unit's name. A bad value fails at construction or
load, naming source, field, constraint and value. Not parameters: the fields of a format (`Event.strength`, `Series.rate`), which
`docs/formats.md` defines.

## Adding a source or shaper

1. **Scaffold:** `uv run poe new-source <name> --kind source|shaper` writes `src/bullterrier_control/builtin/<name>.py` and
   `tests/builtin/test_<name>.py`, working as written (the package imports every module it holds: no list to edit).
2. **Fill in:** the `Params` model (the field's own terms, unit, range, description: "Parameters"), the body (analytic where it
   can be; a causal recurrence through `Rendered` where it needs history), the docstring's first sentence (what
   `help(lib.<name>)` shows). A source that takes events, a grid or other data declares them as keyword-only inputs.
3. **Test:** what it promises, each expected number worked out by hand; the contract tests already cover shapes, ranges,
   determinism, read order and causality for every registered one. If `docs/examples.md` names it under **Needs**, that
   example must now pass.
4. **Gate:** `uv run poe check`, then `uv run lint-imports`. Only the two new files (and docs naming it) appear in `git status`.

## Fixing defects: structurally, not defensively

A fix removes the cause, not the symptom.
1. **Reproduce first, at the public interface** (failing test, red evidence), then **find where the invalid state is created**,
   not where it is noticed. Fix it there.
2. **Make the bad state impossible:** a stricter type, range or `Literal`, one source of truth, a function whose result cannot be
   wrong, or remove the code path. Not a branch that copes afterwards.
3. **Check the class, not the instance:** `grep` for the same shape elsewhere; fix it the same way or say why not.
4. **Trust contracts inside the library; validate at the boundary.** Outside input (a file, a parameter, a signal from another
   package) is validated once, where it enters, with a `BullterrierControlError` naming the value. Internal code does not re-check
   what the layer above guarantees; a broken guarantee fails loudly and its cause is fixed.
5. **No defensive code:** no fallback or default hiding a missing or invalid value; no `try/except` that continues (`except
   Exception`, bare `except`, `contextlib.suppress`); no `isinstance`, `None` or range re-checks of internal arguments; no
   clamping, `np.nan_to_num` or retry "just in case"; no `getattr`/`.get()` with a default for something that must exist; no
   widening a type to silence an error; no skipped, xfailed or loosened tests (the one skip allowed: a worked example in
   `docs/examples.md` waiting for the source it names under **Needs**).
6. **A fix that needs a design change says so:** propose it under "Needs from others" instead of patching around the design.
7. **One decision, one place.** A table, constant, name or rule that exists twice is a defect. Keep one definition in the owning
   module. For tests: look for existing coverage first; add a row to the existing table; setup two files need is a fixture in the
   nearest `conftest.py`.
8. **KISS outranks these rules.** If applying one makes the code longer, more indirect or harder to read than the problem, don't:
   note the finding and move on.
9. Where a skill (`systematic-debugging`'s defence-in-depth, "add validation at every layer") disagrees with this section, this
   section wins.

## Lint, types and gates

`uv run poe check` runs every gate, cheapest first, stopping at the first failure: `ruff format --check`; `ruff check` (plus
`PLW1514` explicit encodings, `BLE` no blind `except`, `PTH` pathlib); `poe lint-workflows` (actionlint, zizmor); `vulture` (dead
code in `src`; exemptions in `[tool.vulture]`); `deptry`; `poe build-check` (twine, check-wheel-contents); `mypy`; the tests, in
random order (import contracts and the tooling's own tests). Each gate has a test proving it fails on a known-bad input.

`ruff` wants type hints and a docstring on every public function, class, method and module, and no `print` outside the CLI.
`mypy` checks `src` and `tests`. Fix findings properly; for a false positive use a targeted `# noqa: CODE  (reason)` or
`# type: ignore[code]  # reason` (a bare or unused one fails). Tests may not reach private names (`SLF001`, `PLC2701`), except
architecture tests, with a reason.

## Testing

Follow the `testing-best-practices` skill. A test coupled to implementation or otherwise poor (weak sole assertion, tautology,
golden file, sleep, mock of our own code) is worse than none: delete it. Reviewers treat such tests as blocking.

- Behaviour through the public contract; **property tests, not golden files**. E.g. an envelope never leaves 0..1; a gate's output
  is two-valued; a slew-limited signal never moves faster than its limit; reading at time t twice gives the same value.
- Expected values come from an independent oracle (standard, hand computation, brute-force reference), never the code's formula.
- Every checker/validator gets a known-good and a known-bad input. Seeds pin determinism. No sleeps, live network or wall-clock
  assertions.
- Tests run in random order (pytest-randomly; rerun with `-p randomly --randomly-seed=N`, disable with `-p no:randomly`); none
  may depend on another.
- **Mutation-check your tests:** break the code on purpose (a sign, a boundary, an ignored argument) and see a test fail.
  Surviving mutants on core behaviour are review findings. Use `PYTHONDONTWRITEBYTECODE=1` and clear `__pycache__` between mutants.
- Refactors prove behaviour is preserved by comparing outputs before and after, in the report, not committed.

**Tests describe behaviour, not implementation:** they must survive any behaviour-preserving refactor.
- Never import underscore names, read production constants or private attributes, or call private functions. Worth testing means
  worth a public name; otherwise test it through the behaviour it serves.
- Never monkeypatch or mock our own code; never assert call counts or order on it. Substitute only true external boundaries
  (clock, network, filesystem via `tmp_path`), preferably by injecting through a public parameter.
- No source or AST inspection in behaviour tests (architecture tests are the exception; their docstring says so).
- Assert observable outputs only: returned values, files written, exceptions raised. For errors, the type plus a short substring
  naming the argument or value, never the whole sentence.
- Hard to test through the public interface is a design signal: change the interface, don't reach inside.

Writing a test file:
- **Parametrise by default:** cases differing by literals are one table (`pytest.param(..., id=...)` or a dict) with good and
  bad rows together; the id states the rule.
- **A new test file only for a new module**, at the mirror path.
- **Shared helpers are fixtures in the nearest `conftest.py`**: `tests/conftest.py`, `tests/tools/conftest.py` (`project_copy`,
  `uv_run_env`, `git_repo`). A helper one file uses stays there.
- **DAMP, not DRY:** each case and its expectation are visible; helpers build input or name an assertion, never hide what is
  checked; a table replaces loops and branches.
- **No change-detectors:** test a rule with a known-bad input; never restate configuration.
- **Slow tests in groups:** a module whose module-scoped fixture spawns tools (ruff, mypy, poe, git) carries
  `pytestmark = pytest.mark.xdist_group("<module>")`.
- **Running:** one file or folder: `uv run pytest tests/test_cli.py -n 0` (`-p no:randomly` for fixed order); everything:
  `uv run poe test`.
- **Fast feedback (agents):** while iterating run only the tests covering your change (mirror file or folder, or `-k <name>`).
  Run the full gate (`uv run poe check`, then `uv run lint-imports`) once when the feature work is finished, before you report.

## Skills

Installed globally: `superpowers:writing-plans`, `superpowers:test-driven-development`,
`superpowers:verification-before-completion` (process); `testing-best-practices` (tests); `software-design-philosophy` (deep
modules, no pass-throughs, no information leakage; interface docstrings state promises).
`superpowers:*` skills are process aids, not authority: they tend to verbose, over-engineered, over-defensive output. Where one
asks for more validation, layers, documents or steps than this file, follow this file.

## Working style

- **Read narrowly:** your brief's files, `CLAUDE.md`, one reference module. If you need more, say why in the report.
- **Propose before refactoring** outside your brief's file scope; inside it, execute (plans are pre-approved). Put needed changes
  elsewhere under "Needs from others".
- Report honestly: red/green evidence, exact commands run, anything skipped or unverified.
- Scratch space for plans, briefs, reports: `.superpowers/` (git-ignored; never commit it).

## Git conventions

Once per clone: `uv run poe setup` (hooks and commit template). Before every push or merge: `uv run poe check` (`uv run poe fix`
repairs what it can). Commit hooks run ruff only; the pre-push hook runs the whole gate only for a push to `main` or a tag that
changes more than documentation.
- **Trunk-based:** everything integrates on `main`. Short-lived branch `<type>/<short-description>` (`feat/envelope-release`),
  merged into `main` and deleted when done; no worktrees unless the owner asks; rebase on `main` before reporting. Never push.
- **Conventional Commits 1.0:** `type(scope)!: subject`. Types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`,
  `build`, `ci`, `chore`, `revert`. Scope optional, lowercase: a module name, `core`, `cli`, `docs`, `deps`. `!` or a
  `BREAKING CHANGE:` footer marks a breaking change.
- **Subject:** imperative present tense, lowercase start (acronyms and identifiers excepted), no final period, header at most 72
  characters.
- **Body** (blank line, wrapped at 100): why, not what. **One logical change per commit.**
- **Agents** end the message with `Co-Authored-By: <your model name> <noreply@anthropic.com>`.
- The `commit-msg` hook (`tools/check_commit_message.py`, limits in `[tool.commit-message]`) rejects a malformed message and says
  which rule failed. Merge, revert and fixup commits git writes pass unchanged.
- **Never `--no-verify`, skip, silence or loosen a hook:** fix the cause. If a rule is wrong, change it in `pyproject.toml` in its
  own commit and say why.
- All Python tooling is configured in `pyproject.toml` (ruff, mypy, pytest, import-linter, poe, commit-message). Do not add
  `ruff.toml`, `mypy.ini`, `setup.cfg`, `pytest.ini`, `.coveragerc` or similar (a test fails). Other config files:
  `.pre-commit-config.yaml` (hook wiring only), `.editorconfig`, `.gitmessage`.
