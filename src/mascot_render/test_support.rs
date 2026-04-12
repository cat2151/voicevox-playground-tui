use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use super::DATA_ROOT_ENV;

const MAX_TEMP_DIR_RETRIES: usize = 1024;

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
    let _ = local_data_dir_env_name();
    crate::history::with_local_data_dir_override(value.map(PathBuf::from), f)
}

struct TempRequestLogDir {
    base_dir: PathBuf,
}

impl TempRequestLogDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let temp_root = std::env::temp_dir();
        let process_id = std::process::id();
        for _ in 0..MAX_TEMP_DIR_RETRIES {
            let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
            let base_dir = temp_root.join(format!("vpt-local-data-{process_id}-{unique}"));
            match fs::create_dir(&base_dir) {
                Ok(()) => return Self { base_dir },
                Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("failed to create temp request log dir: {error}"),
            }
        }

        panic!("failed to create unique temp request log dir after repeated retries");
    }
}

impl Drop for TempRequestLogDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base_dir);
    }
}

pub(super) fn with_temp_request_log_dir<T>(f: impl FnOnce(&Path) -> T) -> T {
    let temp_dir = TempRequestLogDir::new();
    with_local_data_dir_env(Some(temp_dir.base_dir.as_os_str().to_os_string()), || {
        let log_dir = crate::history::history_dir().join("logs");
        f(&log_dir)
    })
}
