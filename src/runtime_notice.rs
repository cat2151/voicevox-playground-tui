use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const NOTICE_DURATION: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
struct RuntimeNotice {
    text: String,
    expires_at: Instant,
}

fn runtime_notice_slot() -> &'static Mutex<Option<RuntimeNotice>> {
    static SLOT: OnceLock<Mutex<Option<RuntimeNotice>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

pub(crate) fn set_runtime_notice(text: impl Into<String>) {
    *runtime_notice_slot()
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Some(RuntimeNotice {
        text: text.into(),
        expires_at: Instant::now() + NOTICE_DURATION,
    });
}

pub(crate) fn current_runtime_notice() -> Option<String> {
    let mut slot = runtime_notice_slot()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    match slot.as_ref() {
        Some(message) if message.expires_at > Instant::now() => Some(message.text.clone()),
        Some(_) => {
            *slot = None;
            None
        }
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{current_runtime_notice, set_runtime_notice};

    #[test]
    fn runtime_notice_returns_latest_message() {
        set_runtime_notice("hello");
        assert_eq!(current_runtime_notice().as_deref(), Some("hello"));
    }
}
