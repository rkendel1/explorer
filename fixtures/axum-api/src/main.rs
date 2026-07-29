use axum::{routing::{get,post}, Router};
fn app() -> Router {
    Router::new()
        .route("/users", get(|| async { "ok" }))
        .route("/users", post(|| async { "ok" }))
}
