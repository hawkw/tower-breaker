use crate::window_counter::WindowedCounter;
use std::sync::{
    atomic::{AtomicIsize, Ordering},
    Arc, Mutex,
};
use tokio::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct SlidingFailureRate(Arc<Inner>);

#[derive(Debug)]
struct Inner {
    current_gen: Mutex<Generation>,
    /// How many failures are allowed per second?
    allowable: isize,
    /// The total time window for each bucket.
    window: Duration,
    /// Buckets tracking the number of failures per window.
    windows: [AtomicIsize; NUM_WINDOWS],
    /// Changes to the current slot, committed when the slot expires.
    current: AtomicIsize,
    /// How much a single failure is "worth".
    failure_value: isize,
    /// How much a single success is "worth".
    success_value: isize,
}

const NUM_WINDOWS: usize = 10;

#[derive(Debug)]
struct Generation {
    /// Window index of the last generation.
    index: usize,
    /// The timestamp since the last generation expired.
    epoch: Instant,
}

impl SlidingFailureRate {
    /// Returns a new `SlidingFailureRate` policy over the given time `window`.
    /// The returned policy will punish an endpoint if its failure rate over
    /// `window` exceeds `max_rate`.
    ///
    /// # Arguments
    ///
    /// * `window`: The total duration of the time window over which the failure
    ///   rate is calculated.
    /// * `allowed_per_sec`: A number of failures that are always allowed per
    ///   second. This is in order to accomodate situations where a client has
    ///   not issued many requests, and the first request fails, having a
    ///   disproportionate impact on the failure rate.
    /// * `failure_percent`: The percentage of failures allowed before the
    ///   endpoint is punished. This is in addition to the failures allowed by
    ///   `allowed_per_sec`. This percentage is expressed as a value between 0
    ///   and 1000.
    ///
    /// For example, if the `failure_percent` is 0.1, then the endpoint will be
    /// punished if more than 1 request fails for every ten successes. If the
    /// `failure_percent` is 0.2, then the endpoint will be punished if more
    /// than 2 requests fail for every ten successes.
    ///
    /// # Panics
    ///
    /// * If `window` is less than 1 second or greater than 60 seconds.
    /// * If `allowed_per_sec` is greater than `i32::MAX`.
    /// * If `retry_percent` is less than 0 or greater than 1000.
    pub fn new(window: Duration, allowed_per_sec: u32, failure_percent: f32) -> Self {
        const MAX_RETRY_PERCENT: f32 = 1000.0;
        const NEW_ATOMIC_ISIZE: AtomicIsize = AtomicIsize::new(0);
        assert!(
            (Duration::from_secs(1)..Duration::from_secs(60)).contains(&window),
            "failure rate window must be between 1 and 60 seconds (got {window:?})"
        );
        assert!(
            (0.0..MAX_RETRY_PERCENT).contains(&failure_percent),
            "failure percent must be between 0 and {MAX_RETRY_PERCENT} seconds (got {failure_percent:?})"
        );
        assert!(allowed_per_sec < i32::MAX as u32);

        // What is the relative "value" of a success or a failure?
        let (success_value, failure_value) = match failure_percent {
            // If 0% of the overall request rate is allowed to fail, then any
            // failures over `allowed_per_sec` will result in the endpoint
            // being punished. This means a successful request is "worth"
            // nothing for the overall failure budget.
            0.0 => (0, 1),
            percent if percent <= 1.0 => (1, (1.0 / failure_percent) as isize),
            // The allowed failure percentage is between 1.0 and 1000.0, so for
            // every success `S`, `S*failure_percent` failures are allowed
            // before the endpoint is punished.
            _ => (1000, (MAX_RETRY_PERCENT / failure_percent) as isize),
        };

        let allowable = (allowed_per_sec as isize)
            .saturating_mul(window.as_secs() as isize) // this cast is okay if `window` is between 1 and 60 seconds
            .saturating_mul(failure_value);

        SlidingFailureRate(Arc::new(Inner {
            current_gen: Mutex::new(Generation {
                index: 0,
                epoch: Instant::now(),
            }),
            allowable,
            window: window / NUM_WINDOWS as u32,
            windows: [NEW_ATOMIC_ISIZE; NUM_WINDOWS],
            current: AtomicIsize::new(0),
            failure_value,
            success_value,
        }))
    }
}

impl Inner {
    /// Expire all counts outside the current time window.
    ///
    /// This locks the current generation.
    fn expire(&self) {
        let mut gen = self.current_gen.lock().unwrap();

        let now = Instant::now();
        let mut delta = now.saturating_duration_since(gen.epoch);

        // still in the current time window, we don't need to do anything.
        // TODO(eliza): it would be cool if we could do this check outside the lock...
        if delta < self.window {
            return;
        }

        // commit all the writes in the current window
        let to_commit = self.current.swap(0, Ordering::SeqCst);
        self.windows[gen.index].store(to_commit, Ordering::SeqCst);

        // clear all the windows that we've passed over based on the elapsed
        // time delta.
        let mut i = (gen.index + 1) % NUM_WINDOWS;
        while delta > self.window {
            self.windows[i].store(0, Ordering::SeqCst);
            delta -= self.window;
            i = (i + 1) % NUM_WINDOWS;
        }

        gen.index = i;
        gen.epoch = now;
    }

    fn sum(&self) -> isize {
        let current = self.current.load(Ordering::SeqCst);
        let windowed_sum: isize = self.windows.iter().fold(0, |acc, window| {
            acc.saturating_add(window.load(Ordering::SeqCst))
        });

        current
            .saturating_add(windowed_sum)
            .saturating_add(self.allowable)
    }

    fn record_failure(&self) {
        self.current.fetch_sub(self.failure_value, Ordering::SeqCst);
    }

    fn record_success(&self) {
        self.expire();
        self.current.fetch_add(self.success_value, Ordering::SeqCst);
    }
}

impl super::Policy for SlidingFailureRate {
    fn record_success(&self) {
        self.0.record_success()
    }

    fn record_failure(&self) {
        self.0.record_failure()
    }

    fn is_punished(&self) -> bool {
        todo!("eliza")
    }

    fn reset(&self) {
        todo!("eliza")
    }
}
