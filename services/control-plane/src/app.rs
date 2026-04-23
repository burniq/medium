use axum::{routing::get, Router};

use crate::routes::health::health;

pub fn build_router() -> Router {
    Router::new().route("/health", get(health))
}
