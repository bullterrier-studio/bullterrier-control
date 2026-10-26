# Worked examples

What bullterrier-control is for, shown with numbers. Each example takes what another package would hand over (a bundle
file in [`examples/`](../examples), the kind `bullterrier-audio` writes, or values written by hand), builds a signal, and
reads it at the times a consumer would ask. The output is plain numbers: what they drive is the consumer's business, shown
as one line of arithmetic that is **not** part of this package.

Every number in the tables is worked out by hand from the definition of the source, not copied from a run. A test
(`tests/test_examples.py`) runs each code block and compares; an example whose source is not built yet says so in its
**Needs** line and is skipped until then.

## 1. Kicks to a glow radius

Audio found three kicks; on each, a glow should flare and fade.

```python
from pathlib import Path

from bullterrier_control import lib, load

bundle = load(Path("examples/kicks.json"))  # kicks at 0.5 s (strength 1.0), 1.0 s (0.6) and 1.5 s (1.0)
signal = lib.envelope(events=bundle.events["kick"], attack=0, decay=0.25)
```

| t (s) | value | by hand |
|---|---|---|
| 0.0 | 0 | before the first kick |
| 0.5 | 1.0 | the first kick, at full strength |
| 0.75 | 0.36787944117144233 | one decay time later: e⁻¹ |
| 1.0 | 0.6 | the second kick (0.6) beats the first one's e⁻² = 0.135 (overlaps take the larger) |
| 1.25 | 0.22072766470286540 | 0.6 · e⁻¹, still larger than the first kick's e⁻³ |
| 1.5 | 1.0 | the third kick |

The consumer (video): `glow_radius = 2 + 18 * value` px. Ducking a sound on the kick instead is `1 - 0.8 * value`.

## 2. A ramp for a progress parameter

An effect should evaporate from 0 to 90 % over five seconds, easing in and out.

```python
from bullterrier_control import Keyframe, lib

signal = lib.keyframes(points=[Keyframe(t=0, value=0, ease="in-out"), Keyframe(t=5, value=0.9)])
```

| t (s) | value | by hand (in-out is smoothstep, p²(3 − 2p), at progress p = t / 5) |
|---|---|---|
| 0.0 | 0 | the first point |
| 1.0 | 0.0936 | 0.9 · 0.2²(3 − 0.4) = 0.9 · 0.104 |
| 2.5 | 0.45 | halfway: 0.9 · 0.5 |
| 4.0 | 0.8064 | 0.9 · 0.8²(3 − 1.6) = 0.9 · 0.896 |
| 5.0 | 0.9 | the last point |
| 7.0 | 0.9 | after the last point the value holds |

The consumer (video): the effect's `progress` parameter is `value` at each frame.

## 3. Bass energy to a size

Audio measured the bass, which comes in at 1 s; a star should grow with it, but smoothly and within limits.

```python
from pathlib import Path

from bullterrier_control import from_series, lib, load

bundle = load(Path("examples/bass.json"))  # 0 until 1.0 s, then 1, at 10 values a second
bass = from_series(bundle.series["bass"], interpolation="hold")
signal = lib.clip(lib.slew(bass, rise=0.5, fall=0.5), low=0.1, high=0.9)
```

`slew` climbs by 1 in `rise` = 0.5 s, so by 0.002 each 1 ms internal step: the step read at time t ≥ 1.0 s is number
k = 1000·t and holds 0.002 · (k − 999).

| t (s) | value | by hand |
|---|---|---|
| 0.0 | 0.1 | no bass yet, raised to `low` |
| 1.0 | 0.1 | the first step up, 0.002, is still below `low` |
| 1.25 | 0.502 | 0.002 · 251 |
| 1.4 | 0.802 | 0.002 · 401 |
| 1.6 | 0.9 | arrived at 1, lowered to `high` |
| 3.0 | 0.9 | |

The consumer (video): `star_radius = 40 + 80 * value` px.

## 4. A tempo-synced LFO

A light should pulse once per beat, following the beat as the song slows down. **Needs:** `lfo` (batch C2c).

```python
from pathlib import Path

from bullterrier_control import lib, load

bundle = load(Path("examples/grid.json"))  # beats at 0.0, 0.5, 1.1 and 1.8 s: it slows down
signal = lib.lfo(shape="sine", rate=1, per="beat", grid=bundle.grid)
```

| t (s) | value | by hand: 0.5 + 0.5 · sin(2π · beat) |
|---|---|---|
| 0.0 | 0.5 | beat 0 |
| 0.125 | 1.0 | beat 0.25 (a quarter of the first beat, 0.5 s long) |
| 0.375 | 0.0 | beat 0.75 |
| 0.5 | 0.5 | beat 1 |
| 0.65 | 1.0 | beat 1.25 (a quarter of the second beat, now 0.6 s long) |
| 1.275 | 1.0 | beat 2.25 (a quarter of the third beat, 0.7 s long) |

The consumer (a light, or a MIDI controller): `cc = round(127 * value)`.

## 5. The wake at a drop

Everything should switch to its "after the drop" state when the drop arrives.

```python
from pathlib import Path

from bullterrier_control import lib, load

bundle = load(Path("examples/drop.json"))  # the drop at 12.0 s
signal = lib.latch(events=bundle.events["drop"])
```

| t (s) | value | by hand |
|---|---|---|
| 0.0 | 0 | before the drop |
| 11.999 | 0 | |
| 12.0 | 1 | at the drop |
| 30.0 | 1 | it stays |

The consumer (video): `saturation = 0.3 + 0.7 * value`.

## 6. Scenes: build, drop, outro

Audio found three sections, each an event with a duration. A scene should be as strong as the section it is in, rising into
it over a second and fading out over half a second; where one section fades as the next rises, the larger value wins.

```python
from pathlib import Path

from bullterrier_control import lib, load

bundle = load(Path("examples/sections.json"))  # build 0-6.4 s, drop 6.4-12 s, outro 12-16 s
signal = lib.envelope(events=bundle.events["sections"], attack=1.0, decay=0, sustain=1, release=0.5, curve="linear")
```

Sustain 1 holds a section's strength; each section rises linearly over 1 s and falls linearly over 0.5 s after it ends.

| t (s) | value | by hand |
|---|---|---|
| 0.0 | 0 | the build starts rising |
| 0.5 | 0.2 | the build, halfway up: 0.4 · 0.5 |
| 3.0 | 0.4 | the build, held |
| 6.65 | 0.25 | the drop rising, 1 · 0.25, beats the build releasing, 0.4 · 0.25 / 0.5 = 0.2 |
| 7.4 | 1.0 | the drop, risen and held |
| 12.25 | 0.5 | the drop releasing, 1 · 0.25 / 0.5, beats the outro rising, 0.3 · 0.25 = 0.075 |
| 12.5 | 0.15 | the outro, halfway up: 0.3 · 0.5 |
| 13.0 | 0.3 | the outro, held |
| 16.25 | 0.15 | the outro releasing: 0.3 · 0.25 / 0.5 |
| 17.0 | 0 | after the last release |

The consumer (video): `look_opacity = value`.

## 7. Strong kicks only

Audio found kicks of every strength. Only the strong ones should flare, and none within a quarter second of the last one
kept. **Needs:** `select`, `thin` (batch C3b).

```python
from bullterrier_control import lib

kicks = [
    {"t": 0.5, "strength": 0.9},
    {"t": 0.7, "strength": 0.45},
    {"t": 1.0, "strength": 0.8},
    {"t": 1.05, "strength": 0.95},
    {"t": 2.0, "strength": 0.7},
]
strong = lib.thin(events=lib.select(events=kicks, threshold=0.5), gap=0.25)
signal = lib.envelope(events=strong, decay=0.25)
```

`select` drops the kick of strength 0.45; `thin` drops the one at 1.05, only 0.05 s after the kick at 1.0. What is left:
0.5 s (0.9), 1.0 s (0.8) and 2.0 s (0.7), each decaying with a time constant of 0.25 s.

| t (s) | value | by hand |
|---|---|---|
| 0.5 | 0.9 | the first kick |
| 0.75 | 0.33109149705429813 | 0.9 · e⁻¹; the dropped 0.45 kick would have made it 0.45 · e⁻⁰·² = 0.3684 |
| 1.0 | 0.8 | the second kick, above the first one's 0.9 · e⁻² = 0.12 |
| 1.1 | 0.5362560368285115 | 0.8 · e⁻⁰·⁴; the thinned 0.95 kick would have made it 0.95 · e⁻⁰·² = 0.7778 |
| 2.0 | 0.7 | the last kick, above the second one's 0.8 · e⁻⁴ = 0.015 |

The consumer (video): `flash_gain = 1 + 2 * value`.
