//! Signals combined point by point: `+`, `-`, `*` and negation of signals and numbers, and the larger or smaller of
//! two.

use std::ops::{Add, Mul, Neg, Sub};

use super::node::{Node, Refused};
use super::{Signal, finite};
use crate::Error;

/// What a [`Combined`] node does with the two values at a time.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Operation {
    Add,
    Subtract,
    Multiply,
    Maximum,
    Minimum,
}

impl Operation {
    fn apply(self, first: f64, second: f64) -> f64 {
        match self {
            Self::Add => first + second,
            Self::Subtract => first - second,
            Self::Multiply => first * second,
            // a NaN stays NaN, as numpy's maximum and minimum keep it, so `sample` refuses it instead of hiding it
            Self::Maximum | Self::Minimum if first.is_nan() || second.is_nan() => f64::NAN,
            Self::Maximum => first.max(second),
            Self::Minimum => first.min(second),
        }
    }
}

/// The two operands, in order; at least one is a signal.
#[derive(Debug)]
enum Operands {
    Signals(Signal, Signal),
    NumberAfter(Signal, f64),
    NumberFirst(f64, Signal),
}

/// Two operands combined by an operation at every time.
#[derive(Debug)]
struct Combined {
    operation: Operation,
    operands: Operands,
}

impl Node for Combined {
    fn read(&self, times: &[f64], out: &mut [f64]) -> Result<(), Error> {
        let operation = self.operation;
        match &self.operands {
            Operands::Signals(first, second) => {
                first.0.read(times, out)?;
                let mut seconds = vec![0.0; times.len()];
                second.0.read(times, &mut seconds)?;
                for (value, second) in out.iter_mut().zip(seconds) {
                    *value = operation.apply(*value, second);
                }
            }
            Operands::NumberAfter(first, number) => {
                first.0.read(times, out)?;
                for value in out {
                    *value = operation.apply(*value, *number);
                }
            }
            Operands::NumberFirst(number, second) => {
                second.0.read(times, out)?;
                for value in out {
                    *value = operation.apply(*number, *value);
                }
            }
        }
        Ok(())
    }
}

/// `first` and `second` combined by `operation`.
pub(crate) fn signals(operation: Operation, first: Signal, second: Signal) -> Signal {
    Signal::of(Combined {
        operation,
        operands: Operands::Signals(first, second),
    })
}

/// `signal` combined with `number` by `operation`, the number second, or first if `number_first`. A number that is not
/// finite gives a signal refusing every read with the error [`Signal::constant`] gives for it: an operator cannot
/// return an error.
fn with_number(operation: Operation, signal: Signal, number: f64, number_first: bool) -> Signal {
    match finite(number) {
        Ok(number) if number_first => Signal::of(Combined {
            operation,
            operands: Operands::NumberFirst(number, signal),
        }),
        Ok(number) => Signal::of(Combined {
            operation,
            operands: Operands::NumberAfter(signal, number),
        }),
        Err(error) => Signal::of(Refused(error)),
    }
}

/// `+`, `-` or `*` of a signal (owned or borrowed) with a signal or an `f64`, on either side.
macro_rules! operator {
    ($trait:ident, $method:ident, $operation:expr) => {
        impl $trait<&Signal> for &Signal {
            type Output = Signal;
            fn $method(self, other: &Signal) -> Signal {
                signals($operation, self.clone(), other.clone())
            }
        }

        impl $trait<Signal> for &Signal {
            type Output = Signal;
            fn $method(self, other: Signal) -> Signal {
                signals($operation, self.clone(), other)
            }
        }

        impl $trait<&Signal> for Signal {
            type Output = Signal;
            fn $method(self, other: &Signal) -> Signal {
                signals($operation, self, other.clone())
            }
        }

        impl $trait<Signal> for Signal {
            type Output = Signal;
            fn $method(self, other: Signal) -> Signal {
                signals($operation, self, other)
            }
        }

        impl $trait<f64> for &Signal {
            type Output = Signal;
            fn $method(self, other: f64) -> Signal {
                with_number($operation, self.clone(), other, false)
            }
        }

        impl $trait<f64> for Signal {
            type Output = Signal;
            fn $method(self, other: f64) -> Signal {
                with_number($operation, self, other, false)
            }
        }

        impl $trait<&Signal> for f64 {
            type Output = Signal;
            fn $method(self, other: &Signal) -> Signal {
                with_number($operation, other.clone(), self, true)
            }
        }

        impl $trait<Signal> for f64 {
            type Output = Signal;
            fn $method(self, other: Signal) -> Signal {
                with_number($operation, other, self, true)
            }
        }
    };
}

operator!(Add, add, Operation::Add);
operator!(Sub, sub, Operation::Subtract);
operator!(Mul, mul, Operation::Multiply);

impl Neg for &Signal {
    type Output = Signal;
    fn neg(self) -> Signal {
        -self.clone()
    }
}

impl Neg for Signal {
    type Output = Signal;
    fn neg(self) -> Signal {
        // times -1 flips the sign and nothing else: the bits of negation
        with_number(Operation::Multiply, self, -1.0, false)
    }
}
