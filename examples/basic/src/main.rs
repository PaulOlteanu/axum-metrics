use std::time::Duration;

use axum::{routing::get, Router};
use axum_metrics::MetricLayer;

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(root))
        .route_layer(MetricLayer {
            time_failures: false,
        });

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn root() -> &'static str {
    tokio::time::sleep(Duration::from_secs(5)).await;
    "Hello, World!"
}
