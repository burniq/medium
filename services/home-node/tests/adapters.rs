use home_node::adapters::normalize_target_addr;

#[test]
fn target_address_is_passed_through() {
    assert_eq!(normalize_target_addr("127.0.0.1:3000"), "127.0.0.1:3000");
}
