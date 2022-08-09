use crate::{Config, Policy};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::time::{self, Instant};
use tower_service::Service;

pub struct CircuitBreaker<P, S> {
    inner: S,
    config: Config<P>,
    tripped: bool,
    // TODO(eliza): exponential backoff?
    tripped_until: Pin<Box<time::Sleep>>,
}

pin_project_lite::pin_project! {
    #[derive(Debug)]
    pub struct ResponseFuture<P, F> {
        #[pin]
        future: F,
        policy: P,
    }
}

// === impl CircuitBreaker ===

impl<P, S> CircuitBreaker<P, S>
where
    P: Policy + Clone,
{
    pub fn new(config: Config<P>, inner: S) -> Self {
        // because we don't start in the "tripped" state, this initial sleep
        // will not be polled...
        let tripped_until = Box::pin(time::sleep(config.trip_for));
        CircuitBreaker {
            inner,
            config,
            tripped: false,
            tripped_until,
        }
    }
}

impl<P, S, Req> Service<Req> for CircuitBreaker<P, S>
where
    P: Policy + Clone,
    S: Service<Req>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = ResponseFuture<P, S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if self.config.policy.is_punished() {
            tracing::trace!(
                "service sent to the Punishment Zone for {:?}",
                self.config.trip_for
            );
            // trip the breaker
            self.tripped = true;
            // reset the policy
            self.config.policy.reset();
            self.tripped_until
                .as_mut()
                .reset(Instant::now() + self.config.trip_for);
        }

        if self.tripped {
            // are we still waiting to become un-punished?
            match self.tripped_until.as_mut().poll(cx) {
                Poll::Ready(_) => {
                    tracing::trace!("service released from Punishment Zone");
                    self.tripped = false;
                }
                Poll::Pending => return Poll::Pending,
            }
        }

        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Req) -> Self::Future {
        debug_assert!(!self.tripped, "tried to call a tripped circuit breaker!");
        ResponseFuture {
            future: self.inner.call(req),
            policy: self.config.policy.clone(),
        }
    }
}

// === impl ResponseFuture ===

impl<P, F, T, E> Future for ResponseFuture<P, F>
where
    F: Future<Output = Result<T, E>>,
    P: Policy,
{
    type Output = Result<T, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();
        match this.future.as_mut().poll(cx) {
            // TODO(eliza): integrate with response classification here...
            Poll::Ready(Ok(res)) => {
                this.policy.record_success();
                Poll::Ready(Ok(res))
            }
            Poll::Ready(Err(err)) => {
                this.policy.record_failure();
                Poll::Ready(Err(err))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
