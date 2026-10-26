# Formats

The plain-data formats bullterrier-control owns and every package in the ecosystem reads or writes: **Series**, **Event**,
**Grid**, carried in one file, the **Bundle**. `bullterrier-audio` writes them without importing this package; this page
and `schemas/bundle-1.schema.json` are the contract. Changing a rule is a new `version` (see Versioning).

Conventions: JSON, UTF-8, one object per file. Times are **seconds** from the start of the material, never frames. Every
number is finite (`NaN`, `Infinity`, `-Infinity` are rejected). A number is a JSON number: a whole number is fine where a
decimal is expected, `true` and a string are not. An optional field is absent when unset; a reader also accepts it
written as `null`.

## Bundle

| Field | Type | Rule |
|---|---|---|
| `format` | string | exactly `"bullterrier-control/bundle"` |
| `version` | int | exactly `1`; a reader rejects any other |
| `source` | string, optional | free text: what produced the file (tool, version, input name) |
| `duration` | number, optional | length of the material in seconds, `> 0` |
| `grid` | Grid, optional | at most one |
| `series` | object, name → Series, default `{}` | |
| `events` | object, name → array of Event, default `{}` | each array sorted by `t`, non-decreasing |

Names (the keys of `series` and `events`) match `^[a-z0-9][a-z0-9_.-]*$` (`bass`, `stem.drums`, `kick`, `sections`). A repeated
key is an error. With `duration` set, every event has `t <= duration` (equal is valid) and every series has
`start <= duration`.

## Series

Values at a fixed rate: sample *i* is at `start + i / rate`.

| Field | Type | Rule |
|---|---|---|
| `start` | number | `>= 0`, default `0` |
| `rate` | number | `> 0`, samples per second |
| `values` | array of numbers | at least one |
| `unit` | string | one of `fraction`, `decibels`, `hertz`, `midi`, `ratio`, `seconds`, `beats` |
| `scaling` | string, optional | required and non-empty when `unit` is `fraction`: how 0..1 was reached (e.g. "5th to 98th percentile of the song"); every value is then in `[0, 1]` |

An undefined stretch (unvoiced pitch) is not `NaN`: the producer reports it as Events, or leaves the series out.

## Event

A moment, or a span when `duration` is set.

| Field | Type | Rule |
|---|---|---|
| `t` | number | `>= 0`, seconds |
| `strength` | number | in `[0, 1]`, default `1` |
| `duration` | number, optional | `> 0`; absent means a moment |
| `label` | string, optional | non-empty (`"A"`, `"drop"`, `"kick"`) |
| `pitch` | number, optional | MIDI note number in `[0, 127]`, fractions allowed |

One type serves onsets, beats, notes, sections and risers; what a name's events mean is the producer's to document.

## Grid

The musical clock, stored as the beat times themselves.

| Field | Type | Rule |
|---|---|---|
| `beats` | array of numbers | at least 2, strictly increasing, each `>= 0`: the time of every beat |
| `meter` | int | beats per bar, `>= 1` |
| `first_downbeat` | int | index into `beats` of the first downbeat, `0 <= first_downbeat < meter` |

Downbeats are `beats[first_downbeat::meter]`. Tempo is not stored: it is `60 / (beats[i+1] - beats[i])`, so a tempo and its
beats cannot disagree. A beat position *p* (fractional; 0 is `beats[0]`) maps to seconds by linear interpolation between
neighbouring beats, and beyond either end by extrapolating the first or last interval; seconds map back by the inverse. The
conversion itself ships with the code that reads signals (roadmap C1); the meaning is fixed here.

## Versioning

`version` is `1`. A new optional field, a loosened or tightened rule, or a new unit is a new version; a reader rejects a
version it does not know and says which it supports. Producers write the lowest version that carries their data.

## Schema

`schemas/bundle-1.schema.json` is generated from the models (`uv run bt-control schema`) for tools in other languages. It
states the shapes, types and ranges; the rules a schema cannot state (sorted events, `duration` bounds, strictly increasing
beats, names, repeated keys) are enforced by `bt-control validate FILE` and by `bullterrier_control.load`.
