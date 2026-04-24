use axum::{Router, routing::{get, post}};

use crate::routes::{
    devices::list_devices, health::health, nodes::register_node, pairing::create_bootstrap_code,
    sessions::open_session,
};
use crate::state::ControlState;

pub fn build_router(state: ControlState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/bootstrap-code", get(create_bootstrap_code))
        .route("/api/devices", get(list_devices))
        .route("/api/nodes/register", post(register_node))
        .route("/api/sessions/open", get(open_session))
        .with_state(state)
}
