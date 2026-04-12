use super::{startup_mode, StartupMode};

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn startup_mode_is_update_when_only_update_subcommand_is_provided() {
    let actual = startup_mode(&args(&["vpt", "update"]));
    assert_eq!(actual, StartupMode::Update);
}

#[test]
fn startup_mode_is_check_when_only_check_subcommand_is_provided() {
    let actual = startup_mode(&args(&["vpt", "check"]));
    assert_eq!(actual, StartupMode::Check);
}

#[test]
fn startup_mode_is_not_update_when_extra_args_are_present() {
    let actual = startup_mode(&args(&["vpt", "update", "--clipboard"]));
    assert_eq!(actual, StartupMode::Clipboard);
}

#[test]
fn startup_mode_is_clipboard_when_clipboard_flag_is_present() {
    let actual = startup_mode(&args(&["vpt", "--clipboard"]));
    assert_eq!(actual, StartupMode::Clipboard);
}

#[test]
fn startup_mode_is_normal_when_check_subcommand_has_extra_args() {
    let actual = startup_mode(&args(&["vpt", "check", "--verbose"]));
    assert_eq!(actual, StartupMode::Normal);
}

#[test]
fn startup_mode_is_normal_without_update_or_clipboard() {
    let actual = startup_mode(&args(&["vpt"]));
    assert_eq!(actual, StartupMode::Normal);
}
