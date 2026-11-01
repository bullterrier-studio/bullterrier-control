# The code-first API

The programmatic API is the product. A patch file is a serialisation of the same graph, built only through this API
(task 10, last, as the extra it is). Each task below is shown in Python, which exists today, and in Rust, the language
of the core. The Python blocks run as tests (`tests/test_api_doc.py`); a `# array([...])` comment is the result of the
`.sample(...)` call above it. The Rust blocks marked **implemented** exist and are the real signatures; the others are the target API that R4 and R5 build
to. Implemented: the formats (`Event::new`, `Series::new`, `Grid::new`, `Bundle::from_json`), the parameters
(`#[derive(Params)]` structs, `ParamTable`, `ParamUnit`, `parameters(name)`, `builtins()`) and the signal graph (R3):
`Signal` (`constant`, `from_series`, `from_values`, `sample`, `sample_into`, `at`, `+ - *` and negation, `maximum`,
`minimum`, `from_fn`, `rendered`), the inputs (R4a: `InputKind::check`, `Input::from_*`, `Keyframe`, `matching`), the registry and its traits (R4b:
`Registry`, `Thing`, `Source`, `Shaper`, `Operation`, `Signal::pipe`, `Signal::recipe`), the `slew` and `follow` shapers and the primitives (`Ease`, `hashing::uniform`,
`scaling::between`, `rounding`, `cadence::positions`). Their rustdoc examples (`crates/core/src/`) run with
`cargo test --doc -p bullterrier-control`.

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

1. **A `Signal`** (implemented) **is a newtype over `Arc<dyn Node + Send + Sync>`**: cheap to clone, `Debug`, `Send` and `Sync`, with `Node`
   sealed. Shaper methods take `&self`, so one signal can feed several chains.
2. **Argument order:** required inputs first, then the parameter struct, then optional inputs
   (`envelope(&kicks, Envelope { .. })`; `lfo(Lfo { .. }, Some(&grid))`, where the grid is optional). A source or shaper
   without parameters takes no struct (`keyframes(&points)`). Inputs are borrowed (`&[Event]`, `&Grid`) and cloned inside.
3. **Parameters are a plain struct per thing**, declared with `#[derive(Params)]`: public fields, a doc comment and a
   `#[param(..)]` attribute each; the derive writes `Default` and the `ParamTable` that checks values (`Params::parse`,
   strict, every broken rule listed). Checked when passed in (the call returns `Result`). Struct literals always end in `..Default::default()`, so a
   field added later breaks no call. Before 1.0, decide between keeping this and `#[non_exhaustive]` with generated
   setters; `cargo-semver-checks` runs from the first publish.
4. **`events` and `from_values` take an options struct and return `Result`**, like everything else. No builders.
   `Signal::from_values(&values, rate, FromValues { start }, Interpolation)` is implemented.
5. **One error type per module family** (`Error` for rendering, `FormatError` for files, `ParamError` for a parameter
   table), each `#[non_exhaustive]`, `std::error::Error + Send + Sync`, carrying the problems (field and rule, with the
   value) with the same message as Python's. No panics, no `unwrap` or `expect`.
6. **Sampling** (implemented): `sample(&self, &[f64]) -> Result<Vec<f64>, Error>`; `sample_into(&self, &[f64], &mut [f64])
   -> Result<(), Error>`, for a live loop; `at(&self, f64) -> Result<f64, Error>`. A buffer of the wrong length is an
   error, not a panic. Time is a plain `f64` in seconds, with no
   unit newtypes. The Python binding restores the input's shape (a 0-d result for a scalar).
7. **Combining** (implemented): `+`, `-`, `*` and negation work on `Signal` and `&Signal` with `f64`. `maximum` and `minimum` are
   infallible.
8. **Entry points:** sources are free functions under `lib::`; shapers are methods, and `pipe` takes any shaper. Plugins
   implement the public `Source`, `Shaper` and `Operation` traits (implemented), each with its one `NAME`; every way
   of making a signal from one checks, names and records it alike. `Signal::from_fn` and `Signal::rendered` are how
   code outside the crate adds a node.
9. **The core crate** forbids `unsafe`, does no I/O, builds for wasm32, gives the same result on every target (no
   platform math in easing or hashing; `follow` still calls std `exp`, which `libm` replaces before 1.0), documents
   every public item, and carries these examples as doctests.
10. **Concurrency** (implemented): no hand-written atomics. The only shared mutable state is the history cache of a
    stateful signal. A Rust lock is never held while calling into Python. A graph is at most `MAX_DEPTH` (1000) nodes
    deep.

## 1. A curve from measured values

One call; the value between two samples is interpolated linearly, and the first and last hold beyond the ends.

```python
import bullterrier_control as btc

energy = btc.from_values([0.0, 1.0, 0.2, 0.0], rate=10)  # samples at 0, 0.1, 0.2 and 0.3 s
energy.sample([0.0, 0.05, 0.1, 0.25, 0.3, 1.0])  # halfway up, the peak, halfway down, then the last value holds
# array([0. , 0.5, 1. , 0.1, 0. , 0. ])
```

```rust
// implemented (doctest of `Signal::from_values`)
use bullterrier_control::{FromValues, Interpolation, Signal};

let energy = Signal::from_values(&[0.0, 1.0, 0.2, 0.0], 10.0, FromValues::default(), Interpolation::Linear)?;
assert_eq!(energy.sample(&[0.05, 0.1, 0.3, 1.0])?, [0.5, 1.0, 0.0, 0.0]);
```

From a file: `btc.from_series(bundle.series["bass"])` and `Signal::from_series(&series, Interpolation::Linear)` (the
series is a `Series`; the bundle is `Bundle::from_json(&text)?`, run as the doctest of `Bundle::from_json`).

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
// `slew` and `follow` are implemented (`Signal::slew`, `Signal::follow`); `clip` comes with R5
use bullterrier_control::builtin::slew;

let glow = energy
    .slew(slew::Params { rise: 0.2, fall: 0.5, ..Default::default() })?
    .clip(clip::Params { low: 0.1, high: 0.8, ..Default::default() })?;
```

The parameters of `slew` as the core declares them (`bullterrier_control::builtin::slew::Params`):

```rust
/// How fast the output may climb and fall.
#[derive(Clone, Debug, PartialEq, Params)]
pub struct Params {
    /// Seconds the output takes to climb by 1; larger is slower. 0 follows a rise at once.
    #[param(default = 0.1, unit = Seconds, min = 0, max = 3600)]
    pub rise: f64,
    /// Seconds the output takes to fall by 1; larger is slower. 0 follows a fall at once.
    #[param(default = 0.1, unit = Seconds, min = 0, max = 3600)]
    pub fall: f64,
    /// Internal samples per second; the output is exact to one sample. Raising it is more exact and slower.
    #[param(default = 1000.0, unit = Hertz, min = 10, max = 48000, advanced)]
    pub rate: f64,
}
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
// implemented
let values: Vec<f64> = glow.sample(&times)?;
let mut frame = [0.0; 3];
glow.sample_into(&[0.0, 0.1, 0.2], &mut frame)?; // into a buffer you keep, for a loop
let now: f64 = glow.at(0.25)?;
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

Every input is checked by the Rust core, once, where it enters. Keyframes must start at 0 or later and strictly
increase in `t`; a broken rule is a `ParameterError` that lists the first 20 as `path: rule`, then
`... and N more`.

```python
import bullterrier_control as btc

try:
    btc.lib.keyframes(points=[btc.Keyframe(t=0, value=0), btc.Keyframe(t=0, value=1)])
except btc.ParameterError as error:
    print(error)  # source 'keyframes': points[1].t: 0.0 is not after the previous t = 0.0
```

The checking itself (**implemented**) is `bullterrier_control::inputs`: data, by the input's name, or typed values, held to
the same rules (`Error::Input`).

```rust
use bullterrier_control::easing::Ease;
use bullterrier_control::inputs::{Input, InputKind, Keyframe};
use serde_json::json;

let kind: InputKind = "points".parse()?;
let points = kind.check(&json!([{"t": 0, "value": 0, "ease": "in-out"}, {"t": 5, "value": 0.9}]))?;
assert_eq!(points, Input::Points(vec![Keyframe::at(0.0, 0.0).ease(Ease::InOut), Keyframe::at(5.0, 0.9)]));

let refused = Input::from_points(vec![Keyframe::at(1.0, 0.0), Keyframe::at(1.0, 2.0)]);
assert_eq!(refused.map_err(|error| error.to_string()), Err("points[1].t: 1.0 is not after the previous t = 1.0".into()));
```

## 6. Combine

Signals add, subtract and multiply with each other and with numbers, and `maximum` and `minimum` take the larger and
the smaller at every time. A crossfade is `a * (1 - m) + b * m`.

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
// implemented, for any signals (`lfo` and `envelope` come with R5)
use bullterrier_control::maximum;

let look = &flare * 0.5 + &lfo * 0.25;
let louder = maximum(&flare, &ramp);
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
registered by being defined.) The class becomes a parameter table of the Rust core when it is made, and the values pass
through that table: the declaration rules and the value checks are Rust's, so a broken declaration or a bad value raises
`ParameterError` naming the field.

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
// implemented (the doctest of `Shaper`): the trait; `pipe` takes it the same way as a built-in
use bullterrier_control::{Error, Params, Shaper, Signal};

/// Where the signal folds back.
#[derive(Clone, Debug, PartialEq, Params)]
pub struct Fold {
    /// How far down from 1 the fold is.
    #[param(default = 0.5, unit = Fraction, min = 0, max = 1)]
    pub depth: f64,
}

impl Shaper for Fold {
    const NAME: &'static str = "fold"; // its one name: in the registry, in its errors and in its recipe

    fn shape(&self, input: &Signal) -> Result<Signal, Error> {
        let (line, input) = (1.0 - self.depth, input.clone());
        Ok(Signal::from_fn(move |times, out| {
            input.read_into(times, out)?;
            for value in out.iter_mut() {
                if *value > line {
                    *value = 2.0 * line - *value;
                }
            }
            Ok::<(), Error>(())
        }))
    }
}

// checked by the table first (a depth outside 0 to 1 is refused, naming "fold" and the field)
let folded = glow.pipe(Fold { depth: 0.5 })?;
```

A signal that needs history is `Signal::rendered(&input, Rendering { rate, ..Default::default() }, run)`: `run` is a
causal recurrence over the input sampled at `rate`, and the signal keeps what it rendered (Python: `Rendered`).

```rust
// implemented (doctest of `Signal::rendered`): the running sum of a constant 1 at 10 samples a second
use bullterrier_control::{Rendering, Signal};

let integral = Signal::rendered(&Signal::constant(1.0)?, Rendering { rate: 10.0, ..Default::default() }, |inputs| {
    let mut sum = 0.0;
    Ok::<_, std::fmt::Error>(inputs.iter().map(|input| { sum += input; sum / 10.0 }).collect())
})?;
assert_eq!(integral.sample(&[0.0, 0.15, 2.55])?, [0.1, 0.2, 2.6]);
```

The derive names the crate `bullterrier_control` in the code it writes, so a dependency renamed in `Cargo.toml` breaks it.
Rename the path in the source instead (`use bullterrier_control as btc;`), which works.

## 10. The same look as a file (the extra)

A patch names sources, shapers and operations and their values, one table per node (`docs/formats.md`, "Patch"). It
builds the same signals, against a bundle of measured data. A graph built in code through the registry saves as a
patch whose data is inline; an operator, values given as they are or a callback in it is refused, naming the signal.

```python
import bullterrier_control as btc
from pathlib import Path

signals = btc.load_patch(Path("examples/glow.toml")).build(btc.load(Path("examples/kicks.json")))  # a dict of signals
glow = signals["glow"]
text = btc.Patch.from_signals({"glow": glow}).to_toml()  # the graph as a patch again, the kicks it chose inline
glow.sample([0.5, 0.75, 1.0, 1.5])  # a flare on each strong kick; the kick at 1.0 s is too weak
# array([1.    , 0.3679, 0.1353, 1.    ])
```

```rust
let signals = btc::Patch::from_toml(&text)?.build(&bundle)?;
let saved = btc::Patch::from_signals(&[("glow", &glow)])?.to_toml(); // a graph built in code, as a patch
```
