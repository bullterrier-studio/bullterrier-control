//! The events at positions `offset`, `offset + n`, `offset + 2n`, ... of the events in time order.
//!
//! Events at the same time count one by one, in the order given. Every field of a kept event is unchanged.

use crate::formats::{Event, Problem};
use crate::registry::{Output, Returns};
use crate::{Error, Operation};

/// Which of the events in time order to keep: every `n`th, counting from `offset`.
#[derive(Clone, Debug, PartialEq, crate::Params)]
#[param(rule = rule)]
pub struct Params {
    /// Keep one event in this many, 1 to 1000: 1 keeps all, 2 every second one, 3 every third.
    #[param(default = 2, unit = Count, min = 1, max = 1000)]
    pub n: i64,
    /// How many events to skip before the first one kept, 0 to 999 and below `n`: 0 starts with the first event, 1
    /// with the second.
    #[param(default = 0, unit = Count, min = 0, max = 999)]
    pub offset: i64,
}

impl Operation for Params {
    const NAME: &'static str = "every";
    type Inputs = Vec<Event>;
    const RETURNS: Returns = Returns::Events;

    /// The events at positions `offset`, `offset + n`, ... of `events`, each as it was. The door gives `events` in time
    /// order (those at one time as given), and the positions count in that order.
    ///
    /// ```
    /// use bullterrier_control::builtin::every;
    /// use bullterrier_control::formats::{Event, EventOptions};
    /// use bullterrier_control::registry::Output;
    /// use bullterrier_control::Operation;
    ///
    /// let events = (0..5)
    ///     .map(|i| Event::new(f64::from(i), EventOptions::default()))
    ///     .collect::<Result<Vec<_>, _>>()?;
    /// let second = every::Params { n: 2, offset: 1 };
    /// assert!(matches!(second.run(events.clone())?, Output::Events(kept) if kept == [events[1].clone(), events[3].clone()]));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Errors
    /// None: every set of events can be thinned.
    fn run(&self, events: Vec<Event>) -> Result<Output, Error> {
        // counted in i64, the type of the parameters, which the table keeps in 1..=1000 and 0..=999
        let kept = (0_i64..)
            .zip(events)
            .filter(|(position, _)| *position >= self.offset && (position - self.offset) % self.n == 0)
            .map(|(_, event)| event)
            .collect();
        Ok(Output::Events(kept))
    }
}

/// `offset` must lie below `n`: the kept positions are `offset`, `offset + n`, ... and the first lies within the first
/// `n`.
fn rule(params: &Params) -> Vec<Problem> {
    if params.offset < params.n {
        return Vec::new();
    }
    vec![Problem {
        path: "offset".to_owned(),
        rule: format!("must be below n = {} (got {})", params.n, params.offset),
    }]
}
