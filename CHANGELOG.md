# Changelog

All notable changes to bullterrier-control are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the version numbers follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Before 1.0, a minor release
may change the public API.

`uv run poe release` writes each release's section from the Conventional Commits since the previous release (see
"Releasing" in [CONTRIBUTING.md](CONTRIBUTING.md)); a section already written by hand for that version is kept, and
its `Unreleased` becomes the release date.

## [Unreleased]

## [0.1.0] - Unreleased

The first release.

### Added

- Control signals read at any time in seconds: `Signal`, with arithmetic, `minimum` and `maximum`; a value at every
  time, deterministic, independent of the order of reads.
- Sources `constant`, `keyframes` (with named and Bezier eases), `envelope`, `latch`, `lfo`, `steps` and `noise`.
- Shapers `clip`, `delay`, `follow`, `gate`, `hold`, `remap` and `slew`.
- Operations on events and series: `beats`, `every`, `normalise`, `quantize`, `select`, `shift` and `thin`.
- Plain-data formats, documented and versioned: Series, Event, Grid, carried in a Bundle (`schemas/bundle-1.schema.json`),
  with `load` and `save`; and the patch, a look as a TOML file of sources, shapers and operations
  (`schemas/patch-1.schema.json`), built against any song's bundle.
- Parameters declared once, with unit, range and description, and described as JSON Schema (`bt-control list --json`).
  `lfo.phase` wraps (`cyclic`).
- Packages can add sources, shapers and operations through the entry-point group `bullterrier_control.sources`.
- The command `bt-control`: `validate`, `schema`, `list`, `sample` (CSV or a bundle) and `plot` (a PNG; needs the `plot` extra,
  `pip install "bullterrier-control[plot]"`).
- A gallery with a picture of every source and shaper (`docs/gallery.md`) and worked examples with the numbers
  (`docs/examples.md`).
