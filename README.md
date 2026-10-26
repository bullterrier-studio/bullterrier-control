# bullterrier-control

Control signals for Python: numbers that change over time, read at any moment in seconds. Keyframes with easing, LFOs,
envelopes and shaping (slew, gates, sample and hold, range mapping), the same result for the same inputs and seed.

It never touches pixels or sound; it answers "what is this value at 12.4 s?". What the value drives is the caller's
business: an image effect, a synthesizer, a light.

## Status

Pre-alpha. The project is scaffolded and gated; no signal exists yet. The work is planned in batches
([docs/roadmap.md](docs/roadmap.md)); the first is the specification of the three plain-data formats this package owns:

| Format | Meaning |
|---|---|
| Series | Values at a fixed rate, from a start time, with a unit |
| Event | A moment in seconds with a strength, and optionally a duration, label and pitch |
| Grid | Tempo map, beat and downbeat times; converts beats and seconds |

```bash
uv sync
uv run bt-control --version
uv run poe check    # every gate: format, lint, dead code, types, tests
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
