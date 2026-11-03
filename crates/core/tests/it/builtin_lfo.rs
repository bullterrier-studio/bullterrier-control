//! The built-in `lfo` through its public face.

#![allow(
    clippy::float_cmp,
    reason = "the values worked out by hand are exact unless a tolerance is named"
)]

use std::f64::consts::FRAC_1_SQRT_2;

use proptest::prelude::*;
use serde_json::json;

use bullterrier_control::builtin::lfo::*;
use bullterrier_control::cadence::Per;
use bullterrier_control::formats::Grid;
use bullterrier_control::registry::{Call, Kind, Output};
use bullterrier_control::{Error, Signal, Source};
use bullterrier_control::{Registry, builtin::lfo, lib};
use std::f64::consts::PI;

fn grid(beats: &[f64], meter: u32) -> Result<Grid, Error> {
    Grid::new(beats.to_vec(), meter, 0).map_err(|source| Error::Format { source })
}

/// Beats of 0.5, 0.6 and 0.7 s.
fn slowing() -> Result<Grid, Error> {
    grid(&[0.0, 0.5, 1.1, 1.8], 4)
}

/// Beats of 0.5 s from 1.0 s on: time 0 is beat -2.
fn late() -> Result<Grid, Error> {
    grid(&[1.0, 1.5, 2.0], 1)
}

/// A wave once a second from -1 to 3: a range of 4 that does not start at 0, so a swapped or ignored bound shows.
fn wave(shape: Shape) -> lfo::Params {
    lfo::Params {
        shape,
        low: -1.0,
        high: 3.0,
        ..Default::default()
    }
}

const QUARTERS: [f64; 4] = [0.0, 0.25, 0.5, 0.75];

// each expected value worked out by hand, on 0..1 then low + 4 * wave
#[test]
fn each_shape_reads_its_values_a_quarter_cycle_apart() -> Result<(), Error> {
    let rows: [(Shape, [f64; 4]); 4] = [
        // 1 - |2x - 1| = 0, 0.5, 1, 0.5
        (Shape::Triangle, [-1.0, 1.0, 3.0, 1.0]),
        // 1 while x < 0.5: the halfway point itself is already low
        (Shape::Square, [3.0, 3.0, -1.0, -1.0]),
        // x
        (Shape::Saw, [-1.0, 0.0, 1.0, 2.0]),
        // 1 - x
        (Shape::SawDown, [3.0, 2.0, 1.0, 0.0]),
    ];
    for (shape, expected) in rows {
        assert_eq!(lib::lfo(wave(shape), None)?.sample(&QUARTERS)?, expected, "{shape:?}");
    }
    Ok(())
}

#[test]
fn a_sine_is_half_plus_half_the_sine_of_a_turn_per_cycle() -> Result<(), Error> {
    // 0.5 + 0.5 sin(2 pi x) at x = 0, 1/8, 1/4, 3/8, 1/2, 3/4: 1/2, (2 + sqrt 2) / 4, 1, (2 + sqrt 2) / 4, 1/2, 0;
    // on -1..3 that is -1 + 4 times it. sin(pi) of the float nearest pi is 1.2e-16, not 0: within 1e-15
    let half_root = 0.5 + 0.5 * FRAC_1_SQRT_2;
    let expected = [1.0, -1.0 + 4.0 * half_root, 3.0, -1.0 + 4.0 * half_root, 1.0, -1.0];
    let read = lib::lfo(wave(Shape::Sine), None)?.sample(&[0.0, 0.125, 0.25, 0.375, 0.5, 0.75])?;
    for (got, expected) in read.iter().zip(expected) {
        assert!((got - expected).abs() <= 1e-15, "{read:?} vs {expected}");
    }
    // the top and the bottom are exact: sin of the float nearest pi / 2 is 1, and of 3 pi / 2 is -1
    assert_eq!([read[2], read[5]], [3.0, -1.0]);
    Ok(())
}

#[test]
fn rate_is_cycles_per_second_and_0_holds_the_phase() -> Result<(), Error> {
    let saw = |rate: f64, phase: f64| {
        lib::lfo(
            lfo::Params {
                rate,
                phase,
                ..wave(Shape::Saw)
            },
            None,
        )
    };
    // twice a second: a quarter cycle is 0.125 s; every other second: 0.5 s
    assert_eq!(saw(2.0, 0.0)?.sample(&[0.125, 0.375, 0.5])?, [0.0, 2.0, -1.0]);
    assert_eq!(saw(0.5, 0.0)?.sample(&[0.5, 1.5, 2.0])?, [0.0, 2.0, -1.0]);
    assert_eq!(saw(0.0, 0.25)?.sample(&[0.0, 3.3, 1e6, 1e300])?, [0.0; 4]);
    // it repeats every cycle: 2.25 s is a quarter of the way in, 7.5 s halfway
    assert_eq!(saw(1.0, 0.0)?.sample(&[2.25, 7.5])?, [0.0, 1.0]);
    Ok(())
}

#[test]
fn phase_starts_the_cycle_that_far_in_and_wraps_whatever_its_value() -> Result<(), Error> {
    let saw = |phase: f64| lfo::Params {
        phase,
        ..wave(Shape::Saw)
    };
    // 0.75 in: 2, then the cycle starts again at 0.25 s
    assert_eq!(lib::lfo(saw(0.75), None)?.sample(&[0.0, 0.25, 0.5])?, [2.0, -1.0, 0.0]);
    // through the door the table wraps it; called directly the body does: either way 2.75 and -0.25 are 0.75
    let times: Vec<f64> = (0..1001).map(|step| f64::from(step) / 100.0).collect();
    let inside = saw(0.75).source(None)?.sample(&times)?;
    for phase in [2.75, -0.25, -1.25] {
        assert_eq!(lib::lfo(saw(phase), None)?.sample(&times)?, inside, "door, {phase}");
        assert_eq!(saw(phase).source(None)?.sample(&times)?, inside, "direct, {phase}");
    }
    Ok(())
}

#[test]
fn a_phase_of_1_is_the_phase_of_0_exactly_even_at_a_decimal_time() -> Result<(), Error> {
    // 3.1 sits a hair either side of a cycle edge in floats: adding a whole cycle before the wrap would move it
    let times: Vec<f64> = (0..1001).map(|step| f64::from(step) / 100.0).collect();
    for shape in [Shape::Sine, Shape::Triangle, Shape::Square, Shape::Saw, Shape::SawDown] {
        let at = |phase: f64| {
            lfo::Params {
                shape,
                phase,
                duty: 0.1,
                ..Default::default()
            }
            .source(None)
        };
        assert_eq!(at(1.0)?.sample(&times)?, at(0.0)?.sample(&times)?, "{shape:?}");
    }
    Ok(())
}

#[test]
fn duty_is_the_share_of_the_cycle_a_square_is_high_and_nothing_to_the_others() -> Result<(), Error> {
    let square = |duty: f64| {
        lib::lfo(
            lfo::Params {
                duty,
                ..wave(Shape::Square)
            },
            None,
        )
    };
    let rows: [(f64, [f64; 4]); 5] = [
        (0.25, [3.0, -1.0, -1.0, -1.0]), // high only up to 0.25, not at it
        (0.75, [3.0, 3.0, 3.0, -1.0]),
        (0.0, [-1.0; 4]),                  // never high
        (1.0, [3.0; 4]),                   // always high
        (5e-324, [3.0, -1.0, -1.0, -1.0]), // high only exactly at the start of a cycle
    ];
    for (duty, expected) in rows {
        assert_eq!(square(duty)?.sample(&QUARTERS)?, expected, "{duty:e}");
    }
    // duty 1 is high also one ulp before a cycle ends
    assert_eq!(
        square(1.0)?.sample(&[1.0_f64.next_down(), 2.0_f64.next_down()])?,
        [3.0, 3.0]
    );
    let saw = |duty: f64| {
        lib::lfo(
            lfo::Params {
                duty,
                ..wave(Shape::Saw)
            },
            None,
        )
    };
    assert_eq!(saw(0.1)?.sample(&QUARTERS)?, saw(0.9)?.sample(&QUARTERS)?);
    Ok(())
}

#[test]
fn low_and_high_place_the_wave_inverted_equal_or_exactly_at_the_top() -> Result<(), Error> {
    let saw = |low: f64, high: f64| {
        lib::lfo(
            lfo::Params {
                shape: Shape::Saw,
                low,
                high,
                ..Default::default()
            },
            None,
        )
    };
    assert_eq!(
        saw(3.0, -1.0)?.sample(&QUARTERS)?,
        [3.0, 2.0, 1.0, 0.0],
        "low above high inverts it"
    );
    assert_eq!(
        saw(2.0, 2.0)?.sample(&QUARTERS)?,
        [2.0; 4],
        "low equal to high is constant"
    );
    // 0.3 + (0.9 - 0.3) * 1 is 0.9000000000000001 in floats: a square with duty 1 sits exactly at the top
    let top = lib::lfo(
        lfo::Params {
            shape: Shape::Square,
            duty: 1.0,
            low: 0.3,
            high: 0.9,
            ..Default::default()
        },
        None,
    )?;
    assert_eq!(top.sample(&[0.0, 0.4, 0.99])?, [0.9; 3]);
    // a zero of the wave on 0..1 is +0.0 whatever the sign of low's zero: -0.0 + 0.0 is +0.0
    let zeros = saw(-0.0, -0.0)?.sample(&[0.0, 0.5])?;
    assert_eq!(
        zeros.iter().map(|zero| zero.to_bits()).collect::<Vec<_>>(),
        [0.0_f64.to_bits(); 2]
    );
    Ok(())
}

#[test]
fn per_beat_one_cycle_a_beat_follows_the_tempo_change() -> Result<(), Error> {
    let saw = lfo::Params {
        shape: Shape::Saw,
        per: Per::Beat,
        ..Default::default()
    };
    // beats at 0, 0.5, 1.1 and 1.8 s, then every 0.7 s: each beat starts a cycle, each middle is halfway
    let signal = lib::lfo(saw.clone(), Some(&slowing()?))?;
    let read = signal.sample(&[0.0, 0.5, 1.1, 1.8, 0.25, 0.8, 1.45, 1.8 + 0.7 * 1000.0 + 0.35])?;
    let expected = [0.0, 0.0, 0.0, 0.0, 0.5, 0.5, 0.5, 0.5];
    for (got, expected) in read.iter().zip(expected) {
        // a beat's position is a whole number to within the grid's float rounding, a hair either side
        let off = (got - expected).abs().min(1.0 - (got - expected).abs());
        assert!(off <= 1e-9, "{read:?}");
    }
    // two cycles a beat: a quarter of a 0.5 s beat (0.125 s) is half a cycle, an eighth (0.0625 s) a quarter
    let fast = lib::lfo(
        lfo::Params {
            rate: 2.0,
            ..wave(Shape::Triangle)
        },
        Some(&slowing()?),
    )?;
    let fast_beat = lib::lfo(
        lfo::Params {
            rate: 2.0,
            per: Per::Beat,
            ..wave(Shape::Triangle)
        },
        Some(&slowing()?),
    )?;
    assert_eq!(fast_beat.sample(&[0.0625, 0.125])?, [1.0, 3.0]);
    // per second the grid is ignored: twice a second, 0.0625 s is an eighth of a cycle
    assert_eq!(fast.sample(&[0.0625, 0.25])?, [0.0, 3.0]);
    // before a first beat at 1 s the cycle runs backwards from it: 0 s is beat -2, 0.75 s beat -0.5
    assert_eq!(
        lib::lfo(saw, Some(&late()?))?.sample(&[0.0, 0.75, 1.0])?,
        [0.0, 0.5, 0.0]
    );
    Ok(())
}

#[test]
fn just_below_a_cycle_edge_the_wave_is_at_the_end_of_its_cycle_not_the_start() -> Result<(), Error> {
    // beat_at(0) is -2e-20 here: mod 1 rounds it to exactly 1.0, the end of a cycle, read one step below it
    // (1 - 2^-53): a square with duty 1 is high, a saw at the top of its ramp, a saw-down and a triangle at the
    // bottom, one and two steps of 2^-53 above it
    let hair = grid(&[1e-20, 0.5], 1)?;
    let rows = [
        (Shape::Square, 1.0),
        (Shape::Saw, 1.0_f64.next_down()),
        (Shape::SawDown, f64::EPSILON / 2.0), // 2^-53
        (Shape::Triangle, f64::EPSILON),      // 2^-52
    ];
    for (shape, expected) in rows {
        let params = lfo::Params {
            shape,
            per: Per::Beat,
            duty: 1.0,
            ..Default::default()
        };
        assert_eq!(lib::lfo(params, Some(&hair))?.sample(&[0.0])?, [expected], "{shape:?}");
    }
    Ok(())
}

#[test]
fn a_time_before_0_reads_as_0_and_far_times_keep_the_phase_or_are_refused() -> Result<(), Error> {
    let saw = lib::lfo(
        lfo::Params {
            shape: Shape::Saw,
            ..Default::default()
        },
        None,
    )?;
    let read = saw.sample(&[-1.0, -0.0, 0.0, 5e-324, 1e15, 1e300, f64::MAX])?;
    let bits: Vec<u64> = read.iter().map(|value| value.to_bits()).collect();
    assert_eq!(bits, [0.0, 0.0, 0.0, 5e-324, 0.0, 0.0, 0.0].map(f64::to_bits));
    // 1000 cycles a second for 1e9 s is exactly 1e12 cycles: the phase 0.25 is what is left
    let fast = lib::lfo(
        lfo::Params {
            rate: 1000.0,
            phase: 0.25,
            ..wave(Shape::Saw)
        },
        None,
    )?;
    assert_eq!(fast.sample(&[1e9])?, [0.0]);
    for per in [Per::Second, Per::Beat] {
        let fast = lib::lfo(
            lfo::Params {
                rate: 1000.0,
                per,
                ..Default::default()
            },
            Some(&slowing()?),
        )?;
        let refused = fast.sample(&[1.797e308]);
        assert!(matches!(&refused, Err(Error::Signal { .. })), "{per:?}: {refused:?}");
    }
    Ok(())
}

#[test]
fn per_beat_without_a_grid_is_refused_naming_per_when_made_even_called_directly() {
    let params = lfo::Params {
        per: Per::Beat,
        ..Default::default()
    };
    for refused in [
        lib::lfo(params.clone(), None).map(|_| ()),
        params.source(None).map(|_| ()),
    ] {
        assert!(
            matches!(&refused, Err(Error::Params { problems, .. }) if problems.len() == 1 && problems[0].path == "per"),
            "{refused:?}"
        );
    }
}

#[test]
fn parameters_no_table_accepts_never_panic_a_read_is_finite_or_refused() -> Result<(), Error> {
    for params in [
        lfo::Params {
            phase: f64::NAN,
            ..Default::default()
        },
        lfo::Params {
            rate: f64::NAN,
            ..Default::default()
        },
        lfo::Params {
            low: f64::INFINITY,
            ..Default::default()
        },
        lfo::Params {
            rate: -1.0,
            phase: 1e300,
            duty: f64::NAN,
            shape: Shape::Square,
            ..Default::default()
        },
    ] {
        match params.source(None)?.sample(&[0.0, 0.3]) {
            Ok(values) => assert!(values.iter().all(|value| value.is_finite()), "{params:?}: {values:?}"),
            Err(error) => assert!(matches!(error, Error::Signal { .. }), "{params:?}: {error:?}"),
        }
    }
    Ok(())
}

#[test]
fn through_the_registry_it_waves_records_the_grid_it_got_and_refuses_what_is_wrong() -> Result<(), Error> {
    let thing = Registry::builtin().get("lfo")?;
    assert_eq!(thing.kind(), Kind::Source);
    let call = |values: serde_json::Value, grid: Option<Grid>| Call {
        inputs: grid
            .into_iter()
            .map(|grid| ("grid".to_owned(), bullterrier_control::inputs::Input::Grid(grid)))
            .collect(),
        values,
        ..Call::default()
    };
    let names = |signal: &Signal| -> Vec<String> {
        signal
            .recipe()
            .map(|recipe| recipe.inputs.iter().map(|(name, _)| name.clone()).collect())
            .unwrap_or_default()
    };
    let Output::Signal(beat) = thing.call(&call(json!({"shape": "saw", "per": "beat"}), Some(late()?)))? else {
        return Err(Error::Call {
            message: "not a signal".into(),
        });
    };
    assert_eq!(beat.sample(&[0.0, 0.75])?, [0.0, 0.5]);
    assert_eq!(names(&beat), ["grid"]);
    let Output::Signal(free) = thing.call(&call(json!({"shape": "saw-down", "phase": 1.25}), None))? else {
        return Err(Error::Call {
            message: "not a signal".into(),
        });
    };
    assert_eq!(free.sample(&[0.0, 0.5])?, [0.75, 0.25]);
    assert_eq!(names(&free), Vec::<String>::new());
    let without_grid = thing.call(&call(json!({"per": "beat"}), None)).map(|_| ());
    assert!(
        matches!(&without_grid, Err(Error::Params { problems, .. }) if problems[0].path == "per"),
        "{without_grid:?}"
    );
    let too_fast = thing.call(&call(json!({"rate": 1000.5, "duty": 2}), None)).map(|_| ());
    assert!(
        matches!(&too_fast, Err(Error::Params { shaper, problems, .. })
            if shaper == "lfo" && problems.iter().map(|problem| problem.path.as_str()).eq(["rate", "duty"])),
        "{too_fast:?}"
    );
    Ok(())
}

fn shape() -> impl Strategy<Value = Shape> {
    prop_oneof![
        Just(Shape::Sine),
        Just(Shape::Triangle),
        Just(Shape::Square),
        Just(Shape::Saw),
        Just(Shape::SawDown)
    ]
}

proptest! {
    #[test]
    fn the_output_never_leaves_low_and_high_whichever_way_round(
        shape in shape(),
        rate in 0.0..=1000.0_f64,
        phase in 0.0..=1.0_f64,
        duty in 0.0..=1.0_f64,
        low in -1e6..=1e6_f64,
        high in -1e6..=1e6_f64,
        time in 0.0..1e300_f64,
    ) {
        let read = lib::lfo(lfo::Params { shape, rate, phase, low, high, duty, ..Default::default() }, None)?.sample(&[time])?;
        prop_assert!(low.min(high) <= read[0] && read[0] <= low.max(high), "{} not in {}..{}", read[0], low, high);
    }

    // the continuous shapes, read one period later: the cycle there differs only by the rounding of rate * t
    // and of t + 1 / rate, each at most an ulp of rate * t + 1 (2^-52 of it), so by 4 * 2^-52 * (rate * t + 2)
    // cycles at most; the wave moves at most pi (a sine) per cycle, times |high - low|, plus an ulp of the value
    #[test]
    fn a_continuous_wave_is_the_same_one_period_later(
        sine in any::<bool>(),
        rate in 0.01..=100.0_f64,
        phase in 0.0..=1.0_f64,
        low in -1e6..=1e6_f64,
        high in -1e6..=1e6_f64,
        time in 0.0..=100.0_f64,
    ) {
        let shape = if sine { Shape::Sine } else { Shape::Triangle };
        let signal = lib::lfo(lfo::Params { shape, rate, phase, low, high, ..Default::default() }, None)?;
        let read = signal.sample(&[time, time + 1.0 / rate])?;
        let cycles = 4.0 * f64::EPSILON * (rate * time + 2.0);
        let tolerance = cycles * PI * (high - low).abs() + 2.0 * f64::EPSILON * low.abs().max(high.abs());
        prop_assert!((read[0] - read[1]).abs() <= tolerance, "{:?}, tolerance {}", read, tolerance);
    }

    #[test]
    fn a_read_is_the_same_whatever_the_order_of_the_times(
        shape in shape(),
        rate in 0.0..=1000.0_f64,
        times in prop::collection::vec(0.0..1e6_f64, 1..20),
    ) {
        let signal = lib::lfo(lfo::Params { shape, rate, ..Default::default() }, Some(&slowing()?))?;
        let forward = signal.sample(&times)?;
        let reversed: Vec<f64> = times.iter().rev().copied().collect();
        let mut backward = signal.sample(&reversed)?;
        backward.reverse();
        prop_assert_eq!(forward.iter().map(|value| value.to_bits()).collect::<Vec<_>>(),
            backward.iter().map(|value| value.to_bits()).collect::<Vec<_>>());
    }
}
