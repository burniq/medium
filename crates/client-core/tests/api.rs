use client_core::api::normalize_label;

#[test]
fn normalize_label_trims_whitespace() {
    assert_eq!(normalize_label("  my phone  "), "my phone");
}
