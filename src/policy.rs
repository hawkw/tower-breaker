pub trait Policy {
    fn record_success(&self);

    fn record_failure(&self);

    fn is_punished(&self) -> bool;

    fn reset(&self);
}

mod failure_rate;
pub use failure_rate::SlidingFailureRate;
