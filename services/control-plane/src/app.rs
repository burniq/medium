use axum::{routing::get, Router};

use crate::routes::{
    health::health,
    pairing::create_bootstrap_code,
    sessions::open_session,
};

pub fn build_router() -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/bootstrap-code", get(create_bootstrap_code))
        .route("/api/sessions/open", get(open_session))
}
