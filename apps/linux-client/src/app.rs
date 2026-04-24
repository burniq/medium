use client_core::api::normalize_label;

pub const PRODUCT_NAME: &str = "Overlay";

pub fn title() -> &'static str {
    PRODUCT_NAME
}

pub fn summary() -> &'static str {
    "Overlay CLI"
}

pub fn normalize_device_label(raw: &str) -> String {
    normalize_label(raw)
}
