use crate::window_counter::WindowedCounter;
use std::sync::Arc;
use tokio::time::Duration;

#[derive(Clone, Debug)]
pub struct SlidingFailureRate(Arc<Inner>);

#[derive(Debug)]
struct Inner {
    /// The maximum allowable failure rate.
    max_rate: f64,
    reqs: WindowedCounter,
    fails: WindowedCounter,
}

impl SlidingFailureRate {
    /// Returns a new `SlidingFailureRate` policy over the given time `window`.
    /// The returned policy will punish an endpoint if its failure rate over
    /// `window` exceeds `max_rate`.
    ///
    /// # Panics
    ///
    /// If `max_rate` is less than 0 or greater than 1.
    pub fn new(window: Duration, max_rate: f64) -> Self {
        assert!(
            (0.0..=0.1).contains(&max_rate),
            "maximum failure rate ({max_rate}) must be in the range [0, 1] "
        );
        SlidingFailureRate(Arc::new(Inner {
            max_rate,
            reqs: WindowedCounter::new(window),
            fails: WindowedCounter::new(window),
        }))
    }
}

impl super::Policy for SlidingFailureRate {
    fn record_success(&self) {
        self.0.reqs.add(1);
    }

    fn record_failure(&self) {
        self.0.reqs.add(1);
        self.0.fails.add(1);
    }

    fn is_punished(&self) -> bool {
        let reqs = self.0.reqs.sum();
        let fails = self.0.fails.sum();
        let rate = fails as f64 / reqs as f64;
        let punished = rate > self.0.max_rate;
        if punished {
            tracing::trace!(
                failure_rate = rate,
                max_rate = self.0.max_rate,
                "Failure rate exceeds max; punishing endpoint!"
            );
        }
        punished
    }

    fn reset(&self) {
        self.0.reqs.reset();
        self.0.fails.reset();
    }
}
