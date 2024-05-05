use std::{
    error::Error,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Instant,
};

use pin_project::{pin_project, pinned_drop};
use tower::{Layer, Service};

#[derive(Debug, Clone)]
pub struct MetricLayer {
    pub time_failures: bool,
}

impl<S> Layer<S> for MetricLayer {
    type Service = MetricService<S>;

    fn layer(&self, service: S) -> Self::Service {
        MetricService {
            time_failures: self.time_failures,
            service,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MetricService<S> {
    time_failures: bool,
    service: S,
}

struct RequestMetadata {}

impl From<&axum::extract::Request> for RequestMetadata {
    fn from(value: &axum::extract::Request) -> Self {
        Self {}
    }
}

impl<S, Request> Service<Request> for MetricService<S>
where
    S: Service<Request>,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn Error + Send + Sync>> + 'static,
    S::Response: 'static,
    RequestMetadata: for<'a> std::convert::From<&'a Request>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = ObservedFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let metadata = RequestMetadata::from(&request);
        let fut = self.service.call(request);

        ObservedFuture {
            response_future: fut,
            time_failures: self.time_failures,
            started_at: None,
            metadata,
        }
    }
}

#[pin_project(PinnedDrop)]
pub struct ObservedFuture<F> {
    #[pin]
    response_future: F,
    time_failures: bool,
    started_at: Option<Instant>,
    metadata: RequestMetadata,
}

#[pinned_drop]
impl<F> PinnedDrop for ObservedFuture<F> {
    fn drop(self: Pin<&mut Self>) {
        let this = self.project();
        if let Some(started_at) = this.started_at {
            println!("duration: {:#?}", started_at.elapsed());
        }
    }
}

impl<F, Response, Error> Future for ObservedFuture<F>
where
    F: Future<Output = Result<Response, Error>>,
{
    type Output = Result<Response, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        if this.started_at.is_none() {
            *this.started_at = Some(Instant::now());
        }

        if let Poll::Ready(result) = this.response_future.poll(cx) {
            Poll::Ready(result)
        } else {
            Poll::Pending
        }
    }
}
