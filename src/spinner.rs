use std::sync::OnceLock;
use std::time::{Duration, Instant};

const FRAMES: [&str; 4] = ["-", "\\", "|", "/"];
const FRAME_INTERVAL: Duration = Duration::from_millis(120);

fn started_at() -> Instant {
    static STARTED_AT: OnceLock<Instant> = OnceLock::new();
    *STARTED_AT.get_or_init(Instant::now)
}

pub(crate) fn frame_at(elapsed: Duration) -> &'static str {
    let index = (elapsed.as_millis() / FRAME_INTERVAL.as_millis()) as usize % FRAMES.len();
    FRAMES[index]
}

pub(crate) fn frame() -> &'static str {
    frame_at(started_at().elapsed())
}

pub(crate) fn decorate(message: &str) -> String {
    decorate_with_frame(message, frame())
}

pub(crate) fn decorate_with_frame(message: &str, frame: &str) -> String {
    format!("{frame} {message}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_at_advances_by_interval() {
        assert_eq!(frame_at(Duration::from_millis(0)), "-");
        assert_eq!(frame_at(Duration::from_millis(120)), "\\");
        assert_eq!(frame_at(Duration::from_millis(240)), "|");
        assert_eq!(frame_at(Duration::from_millis(360)), "/");
        assert_eq!(frame_at(Duration::from_millis(480)), "-");
    }

    #[test]
    fn decorate_with_frame_prefixes_message() {
        assert_eq!(
            decorate_with_frame("[startup] waiting...", "|"),
            "| [startup] waiting..."
        );
    }
}
