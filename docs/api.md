# The code-first API

The programmatic API is the product. A patch file is a serialisation of the same graph, built only through this API
(task 10, last, as the extra it is). Each task below is shown in Python, which exists today, and in Rust, the language
of the core. The Python blocks run as tests (`tests/test_api_doc.py`); a `# array([...])` comment is the result of the
`.sample(...)` call above it. The Rust blocks are the target API for the Rust core (not implemented yet), except the formats: `Event::new`,
`Series::new`, `Grid::new` and `Bundle::from_json` exist, and their rustdoc examples (`crates/core/src/formats/`) run with
`cargo test --doc -p bullterrier-control`. R2 to R5 build to the rest.

## Principles

1. Code is the product; a file is a serialisation of the same graph.
2. Typed and discoverable: every source, shaper and operation has a real signature (IDE completion, mypy,
   rust-analyzer).
3. Reads as the pipeline: a signal flows left to right through its shapers.
4. One concept per name; seconds everywhere; parameters in the field's own words.
5. Python and Rust say the same thing in the same order; only the language's idiom differs (keywords against a
   parameter struct with `Default`, validated when the signal is built).

Shapers are taught as methods (`energy.slew(...)`). `btc.lib.<name>` is the same thing as a plain function for calling
(`lib.slew(signal, rise=0.2)`), typed by a generated stub. What a thing is (its description, parameters, units, ranges
and summary) comes from the registry: `registry.get("slew")`.

## Rules the Rust API follows (target)

Decided by the owner on 2026-10-29. They clarify the earlier decision that parameters are structs with `Default`.

1. **A `Signal` is a newtype over `Arc<dyn Node + Send + Sync>`**: cheap to clone, `Debug`, `Send` and `Sync`, with `Node`
   sealed. Shaper methods take `&self`, so one signal can feed several chains.
2. **Argument order:** required inputs first, then the parameter struct, then optional inputs
   (`envelope(&kicks, Envelope { .. })`; `lfo(Lfo { .. }, Some(&grid))`, where the grid is optional). A source or shaper
   without parameters takes no struct (`keyframes(&points)`). Inputs are borrowed (`&[Event]`, `&Grid`) and cloned inside.
3. **Parameters are a plain struct per thing**: public fields, `Default`, `Clone`, `Debug`, `PartialEq`, serde behind a
   feature, checked when passed in (the call returns `Result`). Struct literals always end in `..Default::default()`, so a
   field added later breaks no call. Before 1.0, decide between keeping this and `#[non_exhaustive]` with generated
   setters; `cargo-semver-checks` runs from the first publish.
4. **`events` and `from_values` take an options struct and return `Result`**, like everything else. No builders.
5. **One `#[non_exhaustive] enum Error`**, `std::error::Error + Send + Sync`, carrying the thing, field, rule and value
   with the same message as Python's. No panics, no `unwrap` or `expect`.
6. **Sampling:** `sample(&self, &[f64]) -> Result<Vec<f64>>`; `sample_into(&self, &[f64], &mut [f64]) -> Result<()>`,
   which allocates nothing, for a live loop; `at(&self, f64) -> Result<f64>`. Time is a plain `f64` in seconds, with no
   unit newtypes. The Python binding restores the input's shape (a 0-d result for a scalar).
7. **Combining:** `+`, `-`, `*` and negation work on `Signal` and `&Signal` with `f64`. `maximum` and `minimum` are
   infallible.
8. **Entry points:** sources are free functions under `lib::`; shapers are methods, and `pipe` takes any shaper. Plugins
   implement the public `Source` and `Shaper` traits.
9. **The core crate** forbids `unsafe`, does no I/O, builds for wasm32, uses `libm` for the same result on every target,
   documents every public item, and carries these examples as doctests.
10. **Concurrency:** no hand-written atomics. The only shared mutable state is the history cache of a stateful signal. A
    Rust lock is never held while calling into Python.

## 1. A curve from measured values

One call; the value between two samples is interpolated linearly, and the first and last hold beyond the ends.

```python
import bullterrier_control as btc

energy = btc.from_values([0.0, 1.0, 0.2, 0.0], rate=10)  # samples at 0, 0.1, 0.2 and 0.3 s
energy.sample([0.0, 0.05, 0.1, 0.25, 0.3, 1.0])  # halfway up, the peak, halfway down, then the last value holds
# array([0. , 0.5, 1. , 0.1, 0. , 0. ])
```

```rust
use bullterrier_control as btc;

let energy = btc::from_values(&[0.0, 1.0, 0.2, 0.0], 10.0, FromValues::default())?;
```

From a file: `btc.from_series(bundle.series["bass"])` and `btc::from_series(&bundle.series["bass"])`; the bundle is
`Bundle::from_json(&text)?` in Rust (run as the doctest of `Bundle::from_json`).

## 2. Shape it: a chain

A shaper is a method; each returns a new signal, so shapers chain. `slew` limits how fast the signal moves (`rise`
and `fall` are seconds to move by 1; it works in steps of 1 ms, which is why its values here are shown to two
decimals), `clip` keeps it in a range. The chain methods are for typed literal calls: a misspelt keyword is a type
error (and a `TypeError` when run). Parameters that come from data, such as a config dict, go through `lib.<name>` or
`signal.pipe(lib.<name>, **params)`, which raise `ParameterError` naming the field.

```python
import bullterrier_control as btc

energy = btc.from_values([0.0, 1.0, 0.2, 0.0], rate=10)
glow = energy.slew(rise=0.2, fall=0.5).clip(low=0.1, high=0.8)
glow.sample([0.0, 0.05, 0.1, 0.2, 0.3, 1.0])  # climbs 5 a second, falls 2 a second, stays within 0.1 to 0.8
# array([0.1 , 0.25, 0.5 , 0.57, 0.37, 0.1 ])
```

```rust
let glow = energy
    .slew(Slew { rise: 0.2, fall: 0.5, ..Default::default() })?
    .clip(Clip { low: 0.1, high: 0.8, ..Default::default() })?;
```

## 3. Read it

`sample` takes a number or an array of any shape, in any order, and gives float64 of the same shape.

```python
import bullterrier_control as btc
import numpy as np

glow = btc.from_values([0.0, 1.0, 0.2, 0.0], rate=10).slew(rise=0.2, fall=0.5).clip(low=0.1, high=0.8)
times = np.linspace(0.0, 0.4, 5)
glow.sample(times)  # one call for a whole timeline; one time per call gives the same values
# array([0.1 , 0.5 , 0.57, 0.37, 0.17])
```

```rust
let values: Vec<f64> = glow.sample(&times)?;
```

## 4. Events, and an envelope on each

`btc.events` makes events from times in one call; `Event(...)` still works. An envelope is a flare on each event: here
it jumps to 1 at a kick and fades with a time constant of 0.25 s, so a quarter second later it is at e^-1.

```python
import bullterrier_control as btc

kicks = btc.events([0.5, 1.0, 1.5], label="kick")
flare = btc.lib.envelope(events=kicks, decay=0.25)
flare.sample([0.5, 0.75, 1.0, 1.25])  # 1 at a kick, e^-1 a quarter second later, 1 again at the next kick
# array([1.    , 0.3679, 1.    , 0.3679])
```

```rust
let kicks = btc::events(&[0.5, 1.0, 1.5], Events { label: Some("kick".into()), ..Default::default() })?;
let flare = btc::lib::envelope(&kicks, Envelope { decay: 0.25, ..Default::default() })?;
```

## 5. Keyframes with easing

Values at chosen times, eased between. The ease belongs to the segment that starts at its point (`in-out` is
3x^2 - 2x^3 over the segment); the last value holds.

```python
import bullterrier_control as btc

ramp = btc.lib.keyframes(points=[btc.Keyframe(t=0, value=0, ease="in-out"), btc.Keyframe(t=5, value=0.9)])
ramp.sample([1.0, 2.5, 7.0])  # 0.9 * (3 * 0.2^2 - 2 * 0.2^3) = 0.0936, then halfway, then held
# array([0.0936, 0.45  , 0.9   ])
```

```rust
let ramp = btc::lib::keyframes(&[Keyframe::at(0.0, 0.0).ease(Ease::InOut), Keyframe::at(5.0, 0.9)])?;
```

## 6. Combine

Signals add, subtract and multiply with each other and with numbers, and `maximum` and `minimum` take the larger and
the smaller at every time.

```python
import bullterrier_control as btc

flare = btc.lib.envelope(events=btc.events([0.5, 1.0, 1.5]), decay=0.25)
look = flare * 0.5 + btc.lib.lfo(rate=2) * 0.25
look.sample([0.5, 0.625, 0.75])  # 0.5 * flare + 0.25 * (0.5 + 0.5 sin(2 pi * 2 t)): 0.625, 0.5533, 0.3089
# array([0.625 , 0.5533, 0.3089])
```

```python
import bullterrier_control as btc

flare = btc.lib.envelope(events=btc.events([0.5, 1.0, 1.5]), decay=0.25)
ramp = btc.lib.keyframes(points=[btc.Keyframe(t=0, value=0, ease="in-out"), btc.Keyframe(t=5, value=0.9)])
louder = btc.maximum(flare, ramp)
louder.sample([0.5, 1.2, 2.5])  # the flare while it is above the ramp (1, e^-0.8), then the ramp (0.45)
# array([1.    , 0.4493, 0.45  ])
```

```rust
let look = &flare * 0.5 + &btc::lib::lfo(Lfo { rate: 2.0, ..Default::default() }, None)? * 0.25;
let louder = btc::maximum(&flare, &ramp);
```

## 7. A beat grid and a tempo-synced wobble

A grid turns seconds into beats and back, so a rate "per beat" follows a tempo that changes. These beats slow down at
1 s (0.5 s apart, then 0.4 s), yet the wobble is at the same point of its cycle a quarter and three quarters of the way
through each beat.

```python
import bullterrier_control as btc

grid = btc.Grid(beats=[0.0, 0.5, 1.0, 1.4, 1.8], meter=4, first_downbeat=0)
wobble = btc.lib.lfo(rate=1, per="beat", grid=grid)
wobble.sample([0.125, 0.375, 1.1, 1.3])  # beat 0.25 and 0.75, then beat 2.25 and 2.75: top and bottom of a sine
# array([1., 0., 1., 0.])
```

```rust
let grid = Grid::new(beat_times, 4, 0)?; // run as the doctest of `Grid::new`
let wobble = btc::lib::lfo(Lfo { rate: 1.0, per: Per::Beat, ..Default::default() }, Some(&grid))?;
```

## 8. Seeded noise

The same seed gives the same values, on any platform and in any reading order.

```python
import bullterrier_control as btc
import numpy as np

times = np.linspace(0.0, 10.0, 101)
drift = btc.lib.noise(rate=4, seed=7, shape="smooth")
assert (drift.sample(times) == btc.lib.noise(rate=4, seed=7, shape="smooth").sample(times)).all()
assert (drift.sample(times) != btc.lib.noise(rate=4, seed=8, shape="smooth").sample(times)).any()
```

```rust
let drift = btc::lib::noise(Noise { rate: 4.0, seed: 7, shape: NoiseShape::Smooth, ..Default::default() })?;
```

## 9. Your own shaper (a plugin)

A shaper is a function from a signal to a signal with declared parameters. `pipe` puts any shaper, built-in or yours,
into a chain. (A package of shapers names its module as an entry point and they appear in `btc.lib`; here the shaper is
registered by being defined.)

```python
import numpy as np

import bullterrier_control as btc
from bullterrier_control.plugin import Finite, Params, Signal, param, shaper


class FoldParams(Params):
    """Where the signal folds back."""

    depth: Finite = param(0.5, doc="How far down from 1 the fold is.", unit="fraction", min=0, max=1)


class Folded(Signal):
    """A signal with what lies above the fold mirrored below it."""

    def __init__(self, signal: Signal, depth: float) -> None:
        self._signal, self._line = signal, 1.0 - depth

    def compute(self, times):
        values = self._signal.compute(times)
        return np.where(values > self._line, 2.0 * self._line - values, values)


@shaper(params=FoldParams)
def fold(signal: Signal, params: FoldParams) -> Signal:
    """A signal that mirrors what is above 1 - `depth` back below it."""
    return Folded(signal, params.depth)


glow = btc.from_values([0.0, 1.0, 0.2, 0.0], rate=10).slew(rise=0.2, fall=0.5).clip(low=0.1, high=0.8)
folded = glow.pipe(btc.lib.fold, depth=0.5)
folded.sample([0.05, 0.1, 0.2, 0.3])  # 0.25 and 0.5 are below the fold; 0.57 folds to 2 * 0.5 - 0.57 = 0.43; then 0.37
# array([0.25, 0.5 , 0.43, 0.37])
```

```rust
// a type implementing the shaper trait; `pipe` takes it the same way
let folded = glow.pipe(Fold { depth: 0.5, ..Default::default() })?;
```

## 10. The same look as a file (the extra)

A patch names sources, shapers and operations and their values, one table per node (`docs/formats.md`, "Patch"). It
builds the same signals, against a bundle of measured data; saving a code-built graph as a patch is planned for R4.

```python
import bullterrier_control as btc
from pathlib import Path

signals = btc.load_patch(Path("examples/glow.toml")).build(btc.load(Path("examples/kicks.json")))  # a dict of signals
glow = signals["glow"]
glow.sample([0.5, 0.75, 1.0, 1.5])  # a flare on each strong kick; the kick at 1.0 s is too weak
# array([1.    , 0.3679, 0.1353, 1.    ])
```

```rust
let signals = btc::Patch::from_toml(&text)?.build(&bundle)?;
```
