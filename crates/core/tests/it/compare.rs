//! The comparison of values across routes (`common::compare`), proved on known-good and known-bad input.

use crate::common::compare::{TOLERANCE, first_difference, same_reads};

#[test]
fn values_inside_the_tolerance_agree_and_one_just_over_it_does_not() {
    let inside = 1.0 + TOLERANCE * 0.5;
    let over = 1.0 + TOLERANCE * 2.0;
    assert_eq!(first_difference(&[0.0, 1.0, 2.0], &[0.0, inside, 2.0]), None);
    assert_eq!(first_difference(&[0.0, 1.0, 2.0], &[0.0, 1.0, over + 1.0]), Some(2));
    assert_eq!(first_difference(&[0.0, 1.0], &[0.0, over]), Some(1));
}

#[test]
fn a_large_value_is_compared_relative_to_its_size() {
    assert_eq!(first_difference(&[1e6], &[1e6 + 1e-4]), None);
    assert_eq!(first_difference(&[1e6], &[1e6 + 1e-2]), Some(0));
}

#[test]
fn nan_agrees_with_nan_only_and_the_signs_of_zero_agree() {
    assert_eq!(first_difference(&[f64::NAN], &[f64::NAN]), None);
    assert_eq!(first_difference(&[f64::NAN], &[0.0]), Some(0));
    assert_eq!(first_difference(&[f64::INFINITY], &[f64::INFINITY]), None);
    assert_eq!(first_difference(&[f64::INFINITY], &[f64::MAX]), Some(0));
    assert_eq!(first_difference(&[0.0], &[-0.0]), None);
}

#[test]
fn a_different_length_is_a_difference() {
    assert_eq!(first_difference(&[1.0, 2.0], &[1.0]), Some(1));
    assert_eq!(first_difference(&[1.0], &[1.0, 2.0]), Some(1));
}

#[test]
fn reads_agree_when_values_agree_or_the_refusals_are_equal() {
    let ok = |values: &[f64]| Ok(values.to_vec());
    let no = |text: &str| Err(text.to_string());
    assert_eq!(same_reads(&ok(&[1.0]), &ok(&[1.0])), None);
    assert_eq!(same_reads(&ok(&[1.0]), &ok(&[2.0])), Some(0));
    assert_eq!(same_reads(&no("bad"), &no("bad")), None);
    assert_eq!(same_reads(&no("bad"), &no("worse")), Some(0));
    assert_eq!(same_reads(&ok(&[1.0]), &no("bad")), Some(0));
}
