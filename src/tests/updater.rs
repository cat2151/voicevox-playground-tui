use super::{update_bat_content, GIT_URL};

#[test]
fn update_bat_contains_install_command() {
    let bat = update_bat_content(None);
    assert!(bat.contains("cargo install --force --git"));
    assert!(bat.contains(GIT_URL));
}

#[test]
fn self_update_bat_does_not_restart_vpt() {
    let bat = update_bat_content(None);
    assert!(!bat.contains("start \"\" /b vpt"));
}

#[test]
fn foreground_update_bat_restarts_vpt_only_after_install_succeeds() {
    let bat = update_bat_content(Some("vpt"));
    assert!(bat.contains("&& start \"\" /b vpt"));
    assert!(bat.contains("del \"%~f0\""));
    assert!(!bat.contains("(goto)"));
}
