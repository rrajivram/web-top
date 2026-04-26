use axum::{routing::get, Json, Router};
use shared::MetricsSnapshot;
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

async fn metrics() -> Json<MetricsSnapshot> {
    Json(MetricsSnapshot::default())
}

async fn health() -> &'static str {
    "ok"
}

#[tokio::main]
async fn main() {
    let dist_path = std::env::var("FRONTEND_DIST")
        .unwrap_or_else(|_| "crates/frontend/dist".to_string());

    let app = Router::new()
        .route("/api/metrics", get(metrics))
        .route("/api/health", get(health))
        .layer(CorsLayer::permissive())
        .fallback_service(ServeDir::new(dist_path));

    let addr: SocketAddr = "0.0.0.0:8080".parse().unwrap();
    println!("web-top listening on http://{addr}");
    println!("Access from phone: http://<your-mac-ip>:8080");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
