#[test]
fn self_update_uses_empty_crates_list_with_latest_api() {
    assert!(super::self_update_crates().is_empty());
}
