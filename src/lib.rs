//! Tower circuit breaker experiments.
pub mod policy;
pub mod service;
mod window_counter;

pub use self::{policy::Policy, service::CircuitBreaker};
use tokio::time::Duration;

/// Configures a [`CircuitBreaker`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Config<P> {
    /// The policy used to determine whether the circuit breaker has tripped.
    pub policy: P,
    /// How long a breaker remains tripped once the policy determines it to be
    /// tripped.
    pub trip_for: Duration,
}
