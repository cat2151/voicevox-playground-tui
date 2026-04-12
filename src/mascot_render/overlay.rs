use std::sync::{Mutex, OnceLock};
use std::time::Instant;

#[cfg(test)]
use super::OVERLAY_DURATION;

#[derive(Debug, Clone)]
struct OverlayMessage {
    text: String,
    expires_at: Option<Instant>,
    dismiss_with_enter: bool,
}

fn overlay_message_slot() -> &'static Mutex<Option<OverlayMessage>> {
    static SLOT: OnceLock<Mutex<Option<OverlayMessage>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

fn startup_overlay_message_slot() -> &'static Mutex<Option<String>> {
    static SLOT: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

#[cfg(test)]
pub(super) fn set_overlay_message(text: String) {
    let mut slot = overlay_message_slot()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if slot
        .as_ref()
        .is_some_and(|message| message.dismiss_with_enter)
    {
        return;
    }
    *slot = Some(OverlayMessage {
        text,
        expires_at: Some(Instant::now() + OVERLAY_DURATION),
        dismiss_with_enter: false,
    });
}

pub(crate) fn set_blocking_overlay_message(text: impl Into<String>) {
    *overlay_message_slot()
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Some(OverlayMessage {
        text: text.into(),
        expires_at: None,
        dismiss_with_enter: true,
    });
}

pub(super) fn clear_overlay_message() {
    let mut slot = overlay_message_slot()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if slot
        .as_ref()
        .is_some_and(|message| message.dismiss_with_enter)
    {
        return;
    }
    *slot = None;
}

pub(crate) fn set_startup_overlay_message(text: impl Into<String>) {
    *startup_overlay_message_slot()
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Some(text.into());
}

pub(crate) fn clear_startup_overlay_message() {
    *startup_overlay_message_slot()
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = None;
}

pub(crate) fn current_overlay_message() -> Option<(String, bool)> {
    let mut slot = overlay_message_slot()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    match slot.as_ref() {
        Some(message) if message.dismiss_with_enter => {
            Some((message.text.clone(), message.dismiss_with_enter))
        }
        Some(message)
            if message
                .expires_at
                .is_some_and(|expires_at| expires_at > Instant::now()) =>
        {
            Some((message.text.clone(), message.dismiss_with_enter))
        }
        Some(_) => {
            *slot = None;
            None
        }
        None => None,
    }
}

pub(crate) fn current_startup_overlay_message() -> Option<String> {
    startup_overlay_message_slot()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
}

pub(crate) fn has_blocking_overlay_message() -> bool {
    overlay_message_slot()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .as_ref()
        .is_some_and(|message| message.dismiss_with_enter)
}

pub(crate) fn dismiss_blocking_overlay_message() {
    let mut slot = overlay_message_slot()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if slot
        .as_ref()
        .is_some_and(|message| message.dismiss_with_enter)
    {
        *slot = None;
    }
}
