use crate::window_counter::WindowedCounter;
use std::sync::{Arc, Mutex};
use tokio::time::Duration;

#[derive(Clone, Debug)]
pub struct SlidingFailureRate(Arc<Mutex<Inner>>);

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
        SlidingFailureRate(Arc::new(Mutex::new(Inner {
            max_rate,
            reqs: WindowedCounter::new(window),
            fails: WindowedCounter::new(window),
        })))
    }
}

impl super::Policy for SlidingFailureRate {
    fn record_success(&self) {
        self.0.lock().unwrap().reqs.add(1);
    }

    fn record_failure(&self) {
        let mut inner = self.0.lock().unwrap();
        inner.fails.add(1);
        inner.reqs.add(1);
    }

    fn is_punished(&self) -> bool {
        let mut inner = self.0.lock().unwrap();
        let reqs = inner.reqs.total();
        let fails = inner.fails.total();
        let rate = fails as f64 / reqs as f64;
        let punished = rate > inner.max_rate;
        if punished {
            tracing::trace!(
                failure_rate = rate,
                max_rate = inner.max_rate,
                "Failure rate exceeds max; punishing endpoint!"
            );
        }
        punished
    }

    fn reset(&self) {
        let mut inner = self.0.lock().unwrap();
        inner.reqs.clear();
        inner.fails.clear();
    }
}
