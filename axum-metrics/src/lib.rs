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
            time_incomplete: self.time_failures,
            service,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MetricService<S> {
    time_incomplete: bool,
    service: S,
}

struct RequestMetadata {
    method: String,
    path: String,
}

impl From<&axum::extract::Request> for RequestMetadata {
    fn from(value: &axum::extract::Request) -> Self {
        Self {
            method: value.method().to_string(),
            path: value.uri().path().to_string(),
        }
    }
}

struct ResponseMetadata {
    code: usize,
}

impl From<&axum::response::Response> for ResponseMetadata {
    fn from(value: &axum::response::Response) -> Self {
        Self {
            code: value.status().as_u16() as usize,
        }
    }
}

impl<S, Request> Service<Request> for MetricService<S>
where
    S: Service<Request>,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn Error + Send + Sync>> + 'static,
    S::Response: 'static,
    RequestMetadata: for<'a> std::convert::From<&'a Request>,
    ResponseMetadata: for<'a> std::convert::From<&'a S::Response>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = ObservedFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let request_metadata = RequestMetadata::from(&request);
        let fut = self.service.call(request);

        ObservedFuture {
            response_future: fut,
            time_failures: self.time_incomplete,
            started_at: None,
            request_metadata,
            response_metadata: None,
        }
    }
}

#[pin_project(PinnedDrop)]
pub struct ObservedFuture<F> {
    #[pin]
    response_future: F,
    time_failures: bool,
    started_at: Option<Instant>,
    request_metadata: RequestMetadata,
    response_metadata: Option<ResponseMetadata>,
}

#[pinned_drop]
impl<F> PinnedDrop for ObservedFuture<F> {
    fn drop(self: Pin<&mut Self>) {
        let this = self.project();
        if let Some(started_at) = this.started_at {
            println!("duration: {:#?}", started_at.elapsed());
            if let Some(response_metadata) = this.response_metadata {
                println!(
                    "{}, {}, {}",
                    this.request_metadata.method,
                    this.request_metadata.path,
                    response_metadata.code
                );
            } else {
                println!(
                    "{}, {}",
                    this.request_metadata.method, this.request_metadata.path
                );
            }
        }
    }
}

impl<F, Response, Error> Future for ObservedFuture<F>
where
    F: Future<Output = Result<Response, Error>>,
    ResponseMetadata: for<'a> std::convert::From<&'a Response>,
{
    type Output = Result<Response, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        if this.started_at.is_none() {
            *this.started_at = Some(Instant::now());
        }

        if let Poll::Ready(result) = this.response_future.poll(cx) {
            if let Ok(response) = result.as_ref() {
                *this.response_metadata = Some(ResponseMetadata::from(response));
            }
            Poll::Ready(result)
        } else {
            Poll::Pending
        }
    }
}
