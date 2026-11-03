//! A signal that holds one value at every time.

use crate::{Error, Signal, Source};

/// The value a constant signal holds.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// The value the signal holds at every time; negative values are fine.
    #[param(default = 0.0, unit = Ratio, min = -1000000.0, max = 1000000.0)]
    pub value: f64,
}

impl Source for Params {
    const NAME: &'static str = "constant";
    type Inputs = ();

    /// The signal that is `value` at every time. The body only: [`lib::constant`](crate::lib::constant) checks the
    /// value against its range, names and records it.
    ///
    /// ```
    /// use bullterrier_control::builtin::constant;
    /// use bullterrier_control::{lib, Source};
    ///
    /// let level = constant::Params { value: 0.75 };
    /// assert_eq!(level.source(())?.sample(&[0.0, 1.5, 3600.0])?, [0.75, 0.75, 0.75]);
    /// // the door refuses a value beyond its documented range
    /// assert!(lib::constant(constant::Params { value: 1e9 }).is_err());
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    ///
    /// # Errors
    /// [`Error::Signal`] if `value` is not finite (a door has refused that before this is called).
    fn source(&self, (): ()) -> Result<Signal, Error> {
        Signal::constant(self.value)
    }
}
