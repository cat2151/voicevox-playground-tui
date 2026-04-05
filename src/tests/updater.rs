use cat_self_update_lib::compare_hashes;

#[test]
fn update_is_available_when_hashes_differ_and_local_hash_is_known() {
    let result = compare_hashes("local-hash", "remote-hash");
    assert!(super::is_update_available(&result));
}

#[test]
fn update_is_not_available_when_hashes_match() {
    let result = compare_hashes("same-hash", "same-hash");
    assert!(!super::is_update_available(&result));
}

#[test]
fn update_is_not_available_when_local_hash_is_unknown() {
    let result = compare_hashes("unknown", "remote-hash");
    assert!(!super::is_update_available(&result));
}
