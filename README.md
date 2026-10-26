# bullterrier-control

Control signals for Python: numbers that change over time, read at any moment in seconds. Keyframes with easing, LFOs,
envelopes and shaping (slew, gates, sample and hold, range mapping), the same result for the same inputs and seed.

It never touches pixels or sound. It answers "what is this value at 12.4 s?", and what the value drives is up to you: an
image effect, a synthesizer, a light, a MIDI controller.

```python
from bullterrier_control import Series, Unit, from_series, lib

energy = from_series(Series(rate=10, values=(0.0, 1.0, 0.2, 0.0), unit=Unit.RATIO))  # a measured curve
smooth = lib.slew(energy, rise=0.2, fall=0.5)  # climbs and falls no faster than this
clipped = lib.clip(smooth, low=0.1, high=0.8)  # and stays between 0.1 and 0.8
clipped.sample([0.0, 0.1, 0.2, 0.3, 1.0])  # values at those times, in seconds
# array([0.1  , 0.5  , 0.566, 0.366, 0.1  ])
```

Keyframes: values at chosen times, eased between ([docs/examples.md](docs/examples.md) works this one out by hand):

```python
from bullterrier_control import lib

ramp = lib.keyframes(points=[{"t": 0, "value": 0, "ease": "in-out"}, {"t": 5, "value": 0.9}])
ramp.sample([1.0, 2.5, 7.0])  # the ease belongs to the segment that starts at its point; the last value holds
# array([0.0936, 0.45  , 0.9   ])
```

Envelopes: a flare on each event, here a kick at 0.5 s that fades with a time constant of 0.25 s:

```python
from bullterrier_control import lib

flare = lib.envelope(events=[{"t": 0.5}], decay=0.25)
flare.sample([0.25, 0.5, 0.75, 1.0])  # 0 before the kick, 1 at it, then e^-1 and e^-2
# array([0.    , 1.    , 0.3679, 0.1353])
```

## Concepts

- **Signal**: a value at every time. `signal.sample(times)` takes a number or an array of times in seconds and returns the
  values in the same shape. Signals combine with `+`, `-`, `*`, `minimum` and `maximum`.
- **Sources** make a signal from their parameters (and, for some, from data such as keyframes, events or a beat grid);
  **shapers** make a signal from a signal. Both are in `lib`: `dir(lib)` lists them, `help(lib.slew)` shows every
  parameter with its unit and range.
- **Formats**: measurements come in, and curves go out, as one JSON file, a *bundle* ([docs/formats.md](docs/formats.md)):

| Format | Meaning |
|---|---|
| Series | Values at a fixed rate, from a start time, with a unit |
| Event | A moment in seconds with a strength, and optionally a duration, label and pitch |
| Grid | Beat times, meter and first downbeat; converts beat positions and seconds |

```bash
bt-control validate analysis.json    # is this file a valid bundle?
bt-control schema                    # the JSON Schema of a bundle, for tools in other languages
```

- **Extending**: a package of your own sources and shapers imports `bullterrier_control.plugin` and names its module as an
  entry point of the group `bullterrier_control.sources`; its sources then appear in `lib`.

## Status

Pre-alpha, not yet on PyPI. The formats and the core exist, with the sources `constant`, `keyframes`,
`envelope` and `latch` and the shapers `clip` and `slew`; LFOs, step sequences and noise come next
([docs/roadmap.md](docs/roadmap.md)).

## Development

```bash
uv sync
uv run poe check    # every gate: format, lint, dead code, types, tests
```

See [CONTRIBUTING.md](CONTRIBUTING.md). Every rule is in [CLAUDE.md](CLAUDE.md); the reasons are in
[docs/design-decisions.md](docs/design-decisions.md).

## Works with

bullterrier-control is one of five independent packages of [Bullterrier Studio](https://github.com/bullterrier-studio)
([docs/ecosystem.md](docs/ecosystem.md)). It depends on none of them. `bullterrier-audio` writes its measurements in this
package's formats without importing it; `bullterrier-video` reads signals from here and maps them onto the parameters of
`bullterrier-image` and `bullterrier-vector`.

## Licence

MIT, see [LICENSE](LICENSE).
