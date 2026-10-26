# Design decisions

The decisions that shape bullterrier-control and still hold, each with its reason and what it costs or commits us to. Rules for
contributors are in [CLAUDE.md](../CLAUDE.md). The ecosystem-wide decisions (the five packages, names, shared formats) are in
`bullterrier-design`.

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
  and each side tests against the schema. Not written yet: `docs/formats.md` is the first piece of work.

## Dependencies

- **numpy is the only runtime dependency, added with the first signal** (deptry fails on a dependency nothing imports, so
  none is declared in the scaffold). *Reason:* control must install in a second on a machine with no media libraries, and
  `bullterrier-audio` carries the heavy ones (librosa, torch).

## Project

- **The tooling mirrors bullterrier-image** (one gate, `uv run poe check`; ruff rules, mypy, vulture, deptry, import-linter,
  pre-commit hooks, Conventional Commits, CI workflows, release command). *Reason:* five repositories that are gated the same way
  need no relearning. *Cost:* each repository owns its copy of the tooling scripts and tests; a fix to one is made in the
  others by hand. Image-specific parts (effects, gallery, subject finding) were left out.
- **The entry-point group `bullterrier_control.sources` is not declared yet.** It arrives with the first source and its registry;
  declaring it now would be configuration nothing reads.
- **The version is 0.0.0 until the first release.** `poe release` accepts only a version above the one in `pyproject.toml`, so
  the first release (0.1.0rc1, then 0.1.0) needs a lower starting point.
