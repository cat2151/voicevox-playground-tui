use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use super::DATA_ROOT_ENV;

pub(super) fn with_data_root_env<T>(value: Option<OsString>, f: impl FnOnce() -> T) -> T {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    struct EnvGuard {
        original: Option<OsString>,
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match self.original.as_ref() {
                Some(value) => std::env::set_var(DATA_ROOT_ENV, value),
                None => std::env::remove_var(DATA_ROOT_ENV),
            }
        }
    }

    let _mutex_guard = LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let original = std::env::var_os(DATA_ROOT_ENV);
    match value.as_ref() {
        Some(value) => std::env::set_var(DATA_ROOT_ENV, value),
        None => std::env::remove_var(DATA_ROOT_ENV),
    }
    let _env_guard = EnvGuard { original };

    f()
}

fn local_data_dir_env_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "LOCALAPPDATA"
    }
    #[cfg(target_os = "macos")]
    {
        "HOME"
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        "XDG_DATA_HOME"
    }
}

pub(super) fn with_local_data_dir_env<T>(value: Option<OsString>, f: impl FnOnce() -> T) -> T {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    struct EnvGuard {
        name: &'static str,
        original: Option<OsString>,
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match self.original.as_ref() {
                Some(value) => std::env::set_var(self.name, value),
                None => std::env::remove_var(self.name),
            }
        }
    }

    let _mutex_guard = LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let name = local_data_dir_env_name();
    let original = std::env::var_os(name);
    match value.as_ref() {
        Some(value) => std::env::set_var(name, value),
        None => std::env::remove_var(name),
    }
    let _env_guard = EnvGuard { name, original };

    f()
}

pub(super) fn with_temp_request_log_dir<T>(f: impl FnOnce(&Path) -> T) -> T {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base_dir = std::env::temp_dir().join(format!("vpt-local-data-{unique}"));
    let result = with_local_data_dir_env(Some(base_dir.as_os_str().to_os_string()), || {
        let log_dir = crate::history::history_dir().join("logs");
        f(&log_dir)
    });
    let _ = fs::remove_dir_all(&base_dir);
    result
}
