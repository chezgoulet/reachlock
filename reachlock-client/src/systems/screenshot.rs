//! Env-gated screenshot capture, for verifying UI work without a compositor
//! screenshot tool (Wayland blocks both `x11grab` and the GNOME portal for
//! non-interactive callers).
//!
//! ```sh
//! REACHLOCK_SCREENSHOT=/tmp/menu.png cargo run -p reachlock-client
//! REACHLOCK_SCREENSHOT=/tmp/shot.png REACHLOCK_SCREENSHOT_DELAY=90 …
//! ```
//!
//! Captures after a delay (default 60 frames, so fonts and layout have
//! settled), writes the PNG, then exits. Does nothing at all when the
//! variable is unset, so normal play never pays for it.

use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};

/// Frames to wait before capturing, if `REACHLOCK_SCREENSHOT_DELAY` is unset.
const DEFAULT_DELAY_FRAMES: u32 = 60;
/// Frames to keep running after the capture so the write can finish.
const FRAMES_AFTER_CAPTURE: u32 = 15;

#[derive(Resource)]
pub struct ScreenshotRequest {
    path: String,
    delay: u32,
    frame: u32,
    captured: bool,
}

impl ScreenshotRequest {
    /// Read the request from the environment, or `None` when unset.
    pub fn from_env() -> Option<Self> {
        let path = std::env::var("REACHLOCK_SCREENSHOT").ok()?;
        if path.is_empty() {
            return None;
        }
        let delay = std::env::var("REACHLOCK_SCREENSHOT_DELAY")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_DELAY_FRAMES);
        Some(ScreenshotRequest {
            path,
            delay,
            frame: 0,
            captured: false,
        })
    }
}

pub fn capture_screenshot(
    mut commands: Commands,
    request: Option<ResMut<ScreenshotRequest>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(mut request) = request else {
        return;
    };
    request.frame += 1;

    if !request.captured && request.frame >= request.delay {
        info!("screenshot: capturing to {}", request.path);
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(request.path.clone()));
        request.captured = true;
        return;
    }

    // Give the observer a few frames to write the file before quitting.
    if request.captured && request.frame >= request.delay + FRAMES_AFTER_CAPTURE {
        info!("screenshot: written, exiting");
        exit.write(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Unset means genuinely off — no resource, no systems doing work.
    fn absent_env_makes_no_request() {
        std::env::remove_var("REACHLOCK_SCREENSHOT");
        assert!(ScreenshotRequest::from_env().is_none());
    }

    #[test]
    /// An empty value is a common shell accident (`VAR= cmd`) and must read
    /// as "off", not as a request to write to "".
    fn empty_env_makes_no_request() {
        std::env::set_var("REACHLOCK_SCREENSHOT", "");
        assert!(ScreenshotRequest::from_env().is_none());
        std::env::remove_var("REACHLOCK_SCREENSHOT");
    }

    #[test]
    fn delay_falls_back_when_unparseable() {
        std::env::set_var("REACHLOCK_SCREENSHOT", "/tmp/x.png");
        std::env::set_var("REACHLOCK_SCREENSHOT_DELAY", "not-a-number");
        let request = ScreenshotRequest::from_env().expect("path set means a request");
        assert_eq!(request.delay, DEFAULT_DELAY_FRAMES);
        std::env::set_var("REACHLOCK_SCREENSHOT_DELAY", "5");
        assert_eq!(ScreenshotRequest::from_env().unwrap().delay, 5);
        std::env::remove_var("REACHLOCK_SCREENSHOT");
        std::env::remove_var("REACHLOCK_SCREENSHOT_DELAY");
    }
}
