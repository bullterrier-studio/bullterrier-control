<p align="center">
  <img src="https://raw.githubusercontent.com/bullterrier-studio/bullterrier-control/main/docs/images/logo.svg" alt="bullterrier-control logo: a bull terrier head drawn in sine lines with one orange sine wave across it" height="160">
</p>

<h1 align="center">bullterrier-control</h1>

<p align="center">
  <b>Control signals for Python.</b><br>
  Keyframes with easing, LFOs, envelopes, step sequences and seeded noise, shaped by slew, clip, follow, gate and more,<br>
  read at any moment in seconds. A look is a patch file; the same inputs and seed always give the same values.
</p>

<p align="center">
  <a href="https://github.com/bullterrier-studio/bullterrier-control/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/bullterrier-studio/bullterrier-control/actions/workflows/ci.yml/badge.svg?branch=main"></a>
  <img alt="Python 3.12+" src="https://img.shields.io/badge/python-3.12%2B-3776ab?logo=python&logoColor=white">
  <img alt="uv" src="https://img.shields.io/badge/built%20with-uv-de5fe9">
  <img alt="Tests" src="https://img.shields.io/badge/tests-pytest-2ea44f">
  <img alt="Code style" src="https://img.shields.io/badge/lint-ruff-261230">
  <img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue">
  <img alt="Platforms" src="https://img.shields.io/badge/platforms-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey">
</p>

<p align="center">
  <img src="https://raw.githubusercontent.com/bullterrier-studio/bullterrier-control/main/docs/images/banner.svg" alt="Six signals drawn by bullterrier-control: keyframes, lfo, envelope, steps, noise and slew, each with the value read at one moment">
</p>

**Things that happened at certain moments in, a value for every moment out.** You tell it when things happen (an
explosion at 4.2 s, a drum hit, a scene change) and how you want the world to react (shake, then settle; drift slowly;
change once the boss appears). It answers "what is the value at 12.4 s?" for any time you ask, in any order. It never
touches pixels or sound: what the value drives (an image effect, a camera, a light, a synthesizer, a MIDI controller)
is up to you. Music software calls this automation or modulation, animation tools keyframes.

<table>
  <tr>
    <td width="33%" valign="top"><b>Read at any moment</b><br>A signal has a value at every time. <code>sample(times)</code> takes a number or an array of seconds, in any order, and returns values in the same shape. No clock, no frames.</td>
    <td width="33%" valign="top"><b>Sources, shapers and operations</b><br>Keyframes, LFOs, envelopes, steps, noise and latches make signals; slew, follow, gate, hold, remap, delay and clip shape them; select, thin, quantize and more prepare events and series. Signals combine with <code>+</code>, <code>-</code>, <code>*</code>, <code>minimum</code> and <code>maximum</code>.</td>
    <td width="33%" valign="top"><b>Deterministic</b><br>The same inputs and seed give the same values, whatever the order or number of earlier reads. Noise is the same on every machine.</td>
  </tr>
  <tr>
    <td width="33%" valign="top"><b>Parameters declared once</b><br>Every parameter has a unit, a range and a description. <code>help(btc.lib.slew)</code> lists them, and a bad value fails with the field and the value.</td>
    <td width="33%" valign="top"><b>Plain data in and out</b><br>Measurements and curves travel as one JSON file, a bundle, and a look as a TOML patch, each with a JSON Schema for tools in other languages. <code>bt-control validate</code> checks both.</td>
    <td width="33%" valign="top"><b>Small and extensible</b><br>One Python dependency, numpy. Offline. A package of your own sources, shapers and operations plugs in as an entry point.</td>
  </tr>
</table>

## Contents

[What is it for?](#what-is-it-for) | [What gap does it fill?](#what-gap-does-it-fill) | [Install](#install) |
[Quick start](#quick-start) | [Sources, shapers and operations](#sources-shapers-and-operations) |
[Patch files](#patch-files) | [Concepts](#concepts) | [Formats](#formats) | [Status](#status) | [Development](#development) |
[Works with](#works-with) | [License](#license)

## What is it for?

Say you are cutting a trailer for a game. Three explosions happen at 4.2 s, 9.8 s and 31.0 s, and the boss appears at
60 s. You want the screen to shake on every explosion and settle within half a second, the camera to wander slowly the
whole time, and the corners of the screen to darken once the boss is on screen.

Your video tool needs a number for each of these at every moment it draws. You describe the behaviour once, as a patch
(a small text file), and bullterrier-control produces the numbers:

```toml
# examples/trailer.toml
format = "bullterrier-control/patch"
version = 1

# a jolt on each explosion that fades within about half a second
[shake]
use = "envelope"
events = "events.explosion"   # the moments come from a file of the song's or footage's events
decay = 0.15

# a slow wander, always on
[drift]
use = "noise"
shape = "smooth"
rate = 0.5
low = -0.3
high = 0.3
seed = 3

# darker corners from the moment the boss appears
[vignette]
use = "latch"
events = "events.boss"
before = 0.1
after = 0.7
```

```python
import bullterrier_control as btc
from pathlib import Path

look = btc.load_patch(Path("examples/trailer.toml")).build(btc.load(Path("examples/trailer.json")))
shake = look["shake"]
shake.sample([4.2, 4.35, 10.1, 20.0])  # full jolt on the explosion, then e^-1 and e^-2 of it, then calm
# array([1.    , 0.3679, 0.1353, 0.    ])
```

`look["vignette"]` is 0.1 until 60 s and 0.7 from then on, and `look["drift"]` wanders between -0.3 and 0.3, the same way
every time. The same from a shell, 30 values a second:
`bt-control sample examples/trailer.toml --bundle examples/trailer.json --to 62 --rate 30 --csv`.

What the numbers do (move the camera, darken the corners) is your video tool's business. The library only answers
"how hard is the shake at 4.35 s?". The same patch serves another trailer: give it that trailer's events. The same idea
drives music visuals: swap the explosions for drum hits.

## What gap does it fill?

The pieces exist separately; we found no single Python library that puts them together for offline use. A search in
October 2026 turned up:

- **Easing and keyframe libraries**, such as [easing-lab](https://pypi.org/project/easing-lab/),
  [easings](https://pypi.org/project/easings/) and [keyframed](https://pypi.org/project/keyframed/): curves between
  values, read at a time. They are about the curve; we did not find events, oscillators, noise or tempo in them.
- **Synthesizer, DAW and 3D-app tools** (LFOs, envelopes, automation; Blender's keyframes, Unreal's sound modulation):
  the same ideas, but running live inside that application.
- **Audio-reactive visual tools**, such as [Animusic](https://github.com/wardzin/animusic): they connect an audio
  analysis straight to an effect, so the shaping in between is fixed inside the tool.

bullterrier-control is the part in the middle, on its own, for anything that happens at moments in time. It takes
moments and measurements from any source (music analysis, a game log, a script) and gives back values to drive anything.
What it offers, each backed by tests:

- **Any time, any order, the same answer.** Read 12.4 s first or last, once or in one call for the whole clip: the same
  values. Noise is seeded and is the same on every machine.
- **Events become shapes.** A flare on each event, a gate, a held value, with defined rules where they overlap.
- **Tempo that changes.** A rate "per beat" follows a beat grid that speeds up or slows down.
- **A look is a file.** A patch is plain text a person can read and a program can check before it runs
  (`bt-control list --json` describes every parameter: unit, range, whether it wraps).
- **Small.** One Python dependency (numpy), offline, nothing to run in the background.

If you know a library that already does all of this, we would like to hear of it.

## Install

Requires Python 3.12+. Pre-alpha, not yet on PyPI. The shapers are written in Rust, so installing from GitHub compiles
the extension and needs a Rust toolchain from [rustup](https://rustup.rs/) (a distribution's `cargo` is too old).
Releases will ship ready-built wheels, one per platform for every Python from 3.12: Linux x86_64 and aarch64 (glibc
2.28+), macOS Apple silicon and Intel, Windows x86_64. Any other platform installs the source package and compiles
the extension with rustup ([why these](https://github.com/bullterrier-studio/bullterrier-control/blob/main/docs/design-decisions.md#the-rust-core-batch-r0)). From GitHub:

```bash
pip install git+https://github.com/bullterrier-studio/bullterrier-control.git
```

`bt-control plot` draws with matplotlib, an optional extra; everything else needs only numpy:

```bash
pip install "bullterrier-control[plot] @ git+https://github.com/bullterrier-studio/bullterrier-control.git"
```

From a checkout, with [uv](https://docs.astral.sh/uv/) and rustup (this installs matplotlib too):

```bash
git clone https://github.com/bullterrier-studio/bullterrier-control.git
cd bullterrier-control
uv sync
uv run bt-control --help
```

## Quick start

The docs import the package as `btc`. Measurements in, shaped by a chain of methods and read at times of your choosing:

```python
import bullterrier_control as btc

energy = btc.from_values([0.0, 1.0, 0.2, 0.0], rate=10)  # a measured curve: four values, ten a second
glow = energy.slew(rise=0.2, fall=0.5).clip(low=0.1, high=0.8)  # climbs and falls no faster than this, stays in a range
glow.sample([0.0, 0.1, 0.2, 0.3, 1.0])  # values at those times, in seconds
# array([0.1 , 0.5 , 0.57, 0.37, 0.1 ])
```

A consumer can read a whole timeline in one call, since `sample` takes an array; reading one time per call gives the
same values. [docs/api.md](https://github.com/bullterrier-studio/bullterrier-control/blob/main/docs/api.md) has the full tour, in Python and in the Rust API it is built toward.

Keyframes: values at chosen times, eased between ([docs/examples.md](https://github.com/bullterrier-studio/bullterrier-control/blob/main/docs/examples.md) works this one out by hand):

```python
import bullterrier_control as btc

ramp = btc.lib.keyframes(points=[{"t": 0, "value": 0, "ease": "in-out"}, {"t": 5, "value": 0.9}])
ramp.sample([1.0, 2.5, 7.0])  # the ease belongs to the segment that starts at its point; the last value holds
# array([0.0936, 0.45  , 0.9   ])
```

Envelopes: a flare on each event, here a kick at 0.5 s that fades with a time constant of 0.25 s:

```python
import bullterrier_control as btc

kicks = btc.events([0.5], label="kick")
flare = btc.lib.envelope(events=kicks, decay=0.25)
flare.sample([0.25, 0.5, 0.75, 1.0])  # 0 before the kick, 1 at it, then e^-1 and e^-2
# array([0.    , 1.    , 0.3679, 0.1353])
```

## Sources, shapers and operations

**Sources** make a signal from their parameters and, for some, from data such as keyframes, events or a beat grid.
**Shapers** make a signal from a signal, and are methods of one: `energy.slew(rise=0.2)`. All three kinds are in `btc.lib`
too (`btc.lib.slew(energy, rise=0.2)`): `dir(btc.lib)` lists them, `help(btc.lib.slew)` shows every parameter with its
unit and range. A shaper of your own goes into a chain with `signal.pipe(btc.lib.fold, depth=0.5)`.

| Name | Kind | What it makes |
|---|---|---|
| `keyframes` | source | Passes through values at chosen times, easing from each point to the next |
| `lfo` | source | A sine, triangle, square or saw wave between `low` and `high`, per second or per beat |
| `envelope` | source | Attack, decay, sustain and release on each event |
| `steps` | source | A list of values held one step each, repeating, per second or per beat |
| `noise` | source | Seeded random values, held, joined by lines or eased |
| `latch` | source | One value until the first event with a label, another from then on |
| `constant` | source | One value at every time |
| `slew` | shaper | Follows its input but climbs and falls no faster than allowed |
| `follow` | shaper | Chases its input with its own attack and release times (equal times smooth) |
| `gate` | shaper | Turns its input into 0 and 1, opening and closing at two levels |
| `hold` | shaper | Takes the input's value at each event and keeps it until the next |
| `remap` | shaper | Puts a 0 to 1 input onto a range, bent by an ease |
| `delay` | shaper | Shows its input a fixed time late |
| `clip` | shaper | Follows its input but never leaves a range |

**Operations** make data from data: they prepare events and series before they become signals. They are in `btc.lib` too,
called with keyword inputs (`btc.lib.thin(events=kicks, gap=0.25)`), and give events or a series back.

| Name | Takes | What it gives |
|---|---|---|
| `select` | events | The events with a label and at least a strength |
| `every` | events | Every Nth event |
| `thin` | events | The events that come at least a gap after the last one kept |
| `shift` | events | The events moved earlier or later, cut off at time 0 |
| `beats` | a grid | An event on each beat (downbeats marked), and on subdivisions |
| `quantize` | events, a grid | The events snapped to the nearest beat or subdivision |
| `normalise` | a series | The series scaled between two of its percentiles, to 0 to 1 |

A package of your own imports `bullterrier_control.plugin` and names its module as an entry point of the group
`bullterrier_control.sources`; its sources, shapers and operations then appear in `btc.lib`. [docs/gallery.md](https://github.com/bullterrier-studio/bullterrier-control/blob/main/docs/gallery.md)
draws every source and shaper.

## Patch files

A look is data: a patch names sources, shapers and operations and their values, one table per node. An input is written
inline, or refers to a node above it, or to what the song's bundle holds (`events.kick`, `series.bass`, `grid`), so one
patch serves every song. A patch names signals only: what a signal drives is up to the consumer.

```toml
# examples/glow.toml
format = "bullterrier-control/patch"
version = 1

[strong]
use = "select"
events = "events.kick"   # the bundle's events "kick"
threshold = 0.7

[glow]
use = "envelope"
events = "strong"        # the node above
decay = 0.25
```

```python
import bullterrier_control as btc
from pathlib import Path

glow = btc.load_patch(Path("examples/glow.toml")).build(btc.load(Path("examples/kicks.json")))["glow"]
glow.sample([0.5, 0.75, 1.0, 1.5])  # a flare on each strong kick, then e^-1 and e^-2; the kick at 1.0 s is too weak
# array([1.    , 0.3679, 0.1353, 1.    ])
```

```bash
bt-control sample examples/glow.toml --bundle examples/kicks.json --to 2 --rate 4   # a bundle of series (--csv: a table)
bt-control plot examples/glow.toml --bundle examples/kicks.json --to 2 --out glow.png
bt-control list                      # every source, shaper and operation; `list slew` shows one in full
bt-control list --json               # the same as data for another tool: inputs, and every parameter with its unit and range
```

`bt-control list --json` describes each parameter as JSON Schema (`minimum`, `maximum`, `unit`, `cyclic` for a phase that
wraps), the shape `bullterrier-image` gives its parameters, so a tool can check a pipeline before it runs. The patch format
is in [docs/formats.md](https://github.com/bullterrier-studio/bullterrier-control/blob/main/docs/formats.md), its schema in [schemas/patch-1.schema.json](https://github.com/bullterrier-studio/bullterrier-control/blob/main/schemas/patch-1.schema.json).

## Concepts

- **Signal**: a value at every time. `signal.sample(times)` takes a number or an array of times in seconds and returns the
  values in the same shape.
- **Event**: a moment in seconds with a strength, and optionally a duration, label and pitch: a beat, an onset, a cue.
- **Operation**: data in, data out (events or a series): select, thin or snap events to a grid before they drive a signal.
- **Time is in seconds** everywhere. A value in beats is turned into seconds through a grid.
- **Parameters** are declared once, with a unit, a range and a description, and drive validation, defaults and `help()`.

## Formats

Measurements come in, and curves go out, as one JSON file, a *bundle* ([docs/formats.md](https://github.com/bullterrier-studio/bullterrier-control/blob/main/docs/formats.md)):

| Format | Meaning |
|---|---|
| Series | Values at a fixed rate, from a start time, with a unit |
| Event | A moment in seconds with a strength, and optionally a duration, label and pitch |
| Grid | Beat times, meter and first downbeat; converts beat positions and seconds |

```bash
bt-control validate analysis.json    # is this file a valid bundle? (a .toml file is checked as a patch)
bt-control schema                    # the JSON Schema of a bundle, for tools in other languages (`schema patch`: a patch's)
```

## Status

Pre-alpha, not yet on PyPI. The formats, patch files, the command line (`validate`, `schema`, `list`, `sample`, `plot`)
and the core exist, with the sources `constant`, `keyframes`, `envelope`, `latch`,
`lfo`, `steps` and `noise`, the shapers `clip`, `delay`, `follow`, `gate`, `hold`, `remap` and `slew`, and the operations
`beats`, `every`, `normalise`, `quantize`, `select`, `shift` and `thin`.

## Development

```bash
uv sync             # builds the Rust extension: needs rustup
uv run poe setup    # once per clone: git hooks, commit template, cargo-shear and cargo-deny
uv run poe check    # every gate: format, lint, dead code, types, tests
```

See [CONTRIBUTING.md](https://github.com/bullterrier-studio/bullterrier-control/blob/main/CONTRIBUTING.md). Every rule is in [CLAUDE.md](https://github.com/bullterrier-studio/bullterrier-control/blob/main/CLAUDE.md); the reasons are in
[docs/design-decisions.md](https://github.com/bullterrier-studio/bullterrier-control/blob/main/docs/design-decisions.md). The banner is drawn from the library itself:
`uv run python tools/make_banner.py`.

## Works with

bullterrier-control is one of five independent packages of [Bullterrier Studio](https://github.com/bullterrier-studio)
([docs/ecosystem.md](https://github.com/bullterrier-studio/bullterrier-control/blob/main/docs/ecosystem.md)). It depends on none of them. `bullterrier-audio` writes its measurements in this
package's formats without importing it; `bullterrier-video` reads signals from here and maps them onto the parameters of
`bullterrier-image` and `bullterrier-vector`.

## License

MIT, see [LICENSE](https://github.com/bullterrier-studio/bullterrier-control/blob/main/LICENSE).
