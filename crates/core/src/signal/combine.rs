//! Signals combined point by point: `+`, `-`, `*` and negation of signals and numbers, and the larger or smaller of
//! two.

use std::fmt;
use std::ops::{Add, Mul, Neg, Sub};

use super::node::{Node, Read, Refused};
use super::{Signal, finite};

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
    /// `values` op `seconds`, value by value, into `values`.
    fn signals(self, values: &mut [f64], seconds: &[f64]) {
        for (value, second) in values.iter_mut().zip(seconds) {
            *value = self.apply(*value, *second);
        }
    }

    /// `values` op `number`, into `values`.
    fn number_after(self, values: &mut [f64], number: f64) {
        for value in values {
            *value = self.apply(*value, number);
        }
    }

    /// `number` op `values`, into `values`.
    fn number_first(self, number: f64, values: &mut [f64]) {
        for value in values {
            *value = self.apply(number, *value);
        }
    }

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
enum Operands {
    Signals(Signal, Signal),
    NumberAfter(Signal, f64),
    NumberFirst(f64, Signal),
}

/// Two operands combined by an operation at every time.
struct Combined {
    operation: Operation,
    operands: Operands,
    /// One more than the deeper operand's.
    depth: usize,
}

impl Combined {
    /// The signal combining `operands` by `operation`; a graph deeper than [`MAX_DEPTH`](super::MAX_DEPTH) gives a
    /// signal refusing every read, since an operator cannot return an error.
    fn signal(operation: Operation, operands: Operands) -> Signal {
        let depth = 1 + match &operands {
            Operands::Signals(first, second) => first.0.depth().max(second.0.depth()),
            Operands::NumberAfter(signal, _) | Operands::NumberFirst(_, signal) => signal.0.depth(),
        };
        Signal::checked(Self {
            operation,
            operands,
            depth,
        })
        .unwrap_or_else(|error| Signal::of(Refused(error)))
    }
}

impl fmt::Debug for Combined {
    // not the operands: formatting a graph would recurse as deep as it is, as a read does, at more stack a level
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Combined")
            .field("operation", &self.operation)
            .field("depth", &self.depth)
            .finish_non_exhaustive()
    }
}

impl Node for Combined {
    // the loops are functions of their own, out of this frame: a read recurses through it as deep as the graph, so
    // the stack it takes bounds `MAX_DEPTH`
    fn read(&self, times: &[f64], out: &mut [f64]) -> Read {
        match &self.operands {
            Operands::Signals(first, second) => {
                first.0.read(times, out)?;
                let mut seconds = vec![0.0; times.len()];
                second.0.read(times, &mut seconds)?;
                self.operation.signals(out, &seconds);
            }
            Operands::NumberAfter(first, number) => {
                first.0.read(times, out)?;
                self.operation.number_after(out, *number);
            }
            Operands::NumberFirst(number, second) => {
                second.0.read(times, out)?;
                self.operation.number_first(*number, out);
            }
        }
        Ok(())
    }

    fn depth(&self) -> usize {
        self.depth
    }
}

/// `first` and `second` combined by `operation`.
pub(crate) fn signals(operation: Operation, first: Signal, second: Signal) -> Signal {
    Combined::signal(operation, Operands::Signals(first, second))
}

/// `signal` combined with `number` by `operation`, the number second, or first if `number_first`. A number that is not
/// finite gives a signal refusing every read with the error [`Signal::constant`] gives for it: an operator cannot
/// return an error.
fn with_number(operation: Operation, signal: Signal, number: f64, number_first: bool) -> Signal {
    match finite(number) {
        Ok(number) if number_first => Combined::signal(operation, Operands::NumberFirst(number, signal)),
        Ok(number) => Combined::signal(operation, Operands::NumberAfter(signal, number)),
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
