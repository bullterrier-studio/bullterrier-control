# bullterrier-control

Control signals for Python: numbers that change over time, read at any moment in seconds. Keyframes with easing, LFOs,
envelopes and shaping (slew, gates, sample and hold, range mapping), the same result for the same inputs and seed.

It never touches pixels or sound; it answers "what is this value at 12.4 s?". What the value drives is the caller's
business: an image effect, a synthesizer, a light.

## Status

Pre-alpha. The three plain-data formats and the core exist: a signal read at any time in seconds, shapers that limit and
clip it, and a registry other packages add to. More sources and shapers follow in batches ([docs/roadmap.md](docs/roadmap.md)).

```python
from bullterrier_control import Series, Unit, from_series, registry

energy = from_series(Series(rate=10, values=(0.0, 1.0, 0.2, 0.0), unit=Unit.RATIO))  # a measured curve
smooth = registry.get("slew")(energy, rise=0.2, fall=0.5)  # climbs and falls no faster than this
clipped = registry.get("clip")(smooth, low=0.1, high=0.8)  # and stays between 0.1 and 0.8
clipped.sample([0.0, 0.1, 0.2, 0.3, 1.0])  # values at those times, in seconds
# array([0.1  , 0.5  , 0.566, 0.366, 0.1  ])
```

The formats this package owns:

| Format | Meaning |
|---|---|
| Series | Values at a fixed rate, from a start time, with a unit |
| Event | A moment in seconds with a strength, and optionally a duration, label and pitch |
| Grid | Beat times, meter and first downbeat; converts beat positions and seconds |

```bash
uv sync
uv run bt-control validate analysis.json    # is this file a valid bundle? (docs/formats.md)
uv run bt-control schema                    # the JSON Schema of a bundle, for other languages
uv run poe check                            # every gate: format, lint, dead code, types, tests
```

A bundle is one JSON file; the smallest valid one:

```json
{"format": "bullterrier-control/bundle", "version": 1,
 "series": {"bass": {"rate": 100, "values": [0.0, 0.4, 0.9], "unit": "fraction", "scaling": "peak of the song"}},
 "events": {"kick": [{"t": 0.5}, {"t": 1.0, "strength": 0.8}]}}
```

## Development

See [CONTRIBUTING.md](CONTRIBUTING.md). Every rule is in [CLAUDE.md](CLAUDE.md); the reasons are in
[docs/design-decisions.md](docs/design-decisions.md).

## Works with

bullterrier-control is one of five independent packages under the Bullterrier Studio organisation. It depends on none of
them. `bullterrier-audio` writes its measurements in this package's formats without importing it; `bullterrier-video`
reads signals from here and maps them onto the parameters of `bullterrier-image` and `bullterrier-vector`.

## Licence

MIT, see [LICENSE](LICENSE).
