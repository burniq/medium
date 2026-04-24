pub mod api;

use api::normalize_label;

uniffi::include_scaffolding!("client_core");

fn normalized_label(raw: String) -> String {
    normalize_label(&raw)
}
