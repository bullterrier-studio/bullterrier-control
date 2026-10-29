# Formats

The plain-data formats bullterrier-control owns and every package in the ecosystem reads or writes: **Series**, **Event**,
**Grid**, carried in one file, the **Bundle**; and the **Patch**, a look of signals as data (the last section).
`bullterrier-audio` writes the first three without importing this package; this page, `schemas/bundle-1.schema.json` and
`schemas/patch-1.schema.json` are the contract. Changing a rule is a new `version` (see Versioning).

Conventions: JSON, UTF-8, one object per file. Times are **seconds** from the start of the material, never frames. Every
number is finite (`NaN`, `Infinity`, `-Infinity` are rejected). A number is a JSON number: a whole number is fine where a
decimal is expected, `true` and a string are not (an integer-valued field such as `version` or `meter` also refuses
`1.0`). A whole number beyond 2^53 is refused where a decimal is expected, since a decimal cannot hold it exactly. The one gap:
a JSON integer beyond 64 bits is read like a decimal. Python refuses any `int` beyond 2^53. Strings must be writable as UTF-8 (no lone surrogates). An optional field is absent when unset; a reader also accepts
it written as `null`. `series` and `events` are objects, never `null`.

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
`start <= duration`. An event's span and a Grid's beats may run past `duration`. A name may be used in both `series` and
`events`.

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

In Python, `Series.values` is a read-only float64 array, not a tuple.

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
| `first_downbeat` | int | index into `beats` of the first downbeat: `0 <= first_downbeat < meter` and `< len(beats)` |

Downbeats are `beats[first_downbeat::meter]`. Tempo is not stored: it is `60 / (beats[i+1] - beats[i])`, so a tempo and its
beats cannot disagree. A beat position *p* (fractional; 0 is `beats[0]`) maps to seconds by linear interpolation between
neighbouring beats, and beyond either end by extrapolating the first or last interval; seconds map back by the inverse. The
conversion is `Grid.seconds_at` and `Grid.beat_at`.

## Versioning

`version` is `1`. A reader reports a file it cannot read as JSON (too deeply nested, an integer of thousands of digits) as
invalid. A new optional field, a loosened or tightened rule, or a new unit is a new version; a reader rejects a
version it does not know and says which it supports. Producers write the lowest version that carries their data.

## Schema

`schemas/bundle-1.schema.json` is generated from the Rust types of the core (`uv run bt-control schema`) for tools in other
languages. It states the shapes, types, ranges and names. A schema cannot state, and `bt-control validate FILE` and
`bullterrier_control.load` alone enforce: sorted events, `duration` bounds, strictly increasing beats, `first_downbeat` below
`meter` and `len(beats)`, `scaling` and the 0..1 range of a `fraction` series, repeated keys, integers written as `1.0`
(JSON Schema counts `1.0` as an integer), non-finite numbers, and UTF-8 text.

A refusal lists every broken rule. A value of the wrong JSON type, an unknown field or unit, a non-whole
number for an integer, or an integer beyond 2^53 stops the reading at that value, with one problem.

## Patch

A look of signals as data: sources, shapers and operations with their values, as a TOML file or as a table inside another
file (a video project holds its patches that way; `Patch.parse` reads the table). It names **signals only**: what a signal
drives is the consumer's. The bundle it is built against is given when it is built, never named in it, so one patch serves
every song. Conventions as above (seconds, finite numbers; TOML's `inf` and `nan` are refused), with a TOML integer
accepted where a decimal is expected.

| Key | Type | Rule |
|---|---|---|
| `format` | string | exactly `"bullterrier-control/patch"` |
| `version` | int | exactly `1`; a reader rejects any other |
| *node name* | table | one per node, in the order that matters: a node uses only the nodes above it |

A node name matches `^[a-z][a-z0-9_]*$` and is none of `format`, `version`, `events`, `series`, `grid`. Any other top-level
key is an error, so a mapping of a signal onto something else cannot be written here.

A node:

| Key | Type | Rule |
|---|---|---|
| `use` | string | required: the name of a registered source, shaper or operation (`bt-control list`) |
| `signal` | reference | a shaper's input, required for a shaper and refused for the others |
| *an input* | data or reference | each input the thing takes (`bt-control list --json`): `events`, `grid`, `series`, `points`, `values`; the required ones must be present |
| *a parameter* | number, switch or text | each parameter the thing declares; its unit, range and default are in `bt-control list --json` |

Anything else is an error. An input is written inline, as in a bundle (a list of events or keyframes, a grid, a series), or
as a **reference** where it takes one (`events`, `series`, `grid` and `signal`; `points` and `values` are inline only):

| Reference | Is | Fits |
|---|---|---|
| `events.<name>`, `series.<name>` | an entry of the bundle (`<name>` as in a bundle, dots included: `series.stem.drums`) | `events`, or `series` and `signal` |
| `grid` | the bundle's grid | `grid` |
| a node name | a node above this one | what the node gives: events (an operation on events) for `events`, a series (`normalise`) for `series`, a signal (a source or shaper) for `signal` |

A series where a signal is wanted is read linearly between its samples and holds its first and last value.
Every problem of a patch is reported together as `<node>.<key>: rule`. What a JSON Schema cannot state, and `Patch.parse`
and `bt-control validate` enforce: that `use` names a registered thing and each key an input or parameter of it, every
value's range and type, that a reference fits and names a node above, and (when the patch is built) that the bundle holds
the entries it names. `schemas/patch-1.schema.json` states the header, the shape of a node and a node's name
(`uv run bt-control schema patch`); a version written as `1.0` passes it and not the reader. Versioning is as for the bundle.
