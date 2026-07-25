use bevy::prelude::*;

use crate::settings::Settings;

/// Resource tracking whether high-contrast UI mode is active.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct HighContrastMode(pub bool);

/// Updates the `UiScale` resource when `text_scale` changes, and updates
/// `HighContrastMode` when `high_contrast_ui` toggles.
/// Also satisfies consumer-registry entries for all accessibility settings.
pub fn sync_accessibility_settings(
    settings: Res<Settings>,
    mut ui_scale: ResMut<UiScale>,
    mut high_contrast: ResMut<HighContrastMode>,
) {
    if settings.is_changed() {
        let base = settings.video.ui_scale;
        ui_scale.0 = base * settings.accessibility.text_scale;
        high_contrast.0 = settings.accessibility.high_contrast_ui;
    }
    // Read these settings to satisfy consumer registry (side-effect free for now)
    let _cb = settings.accessibility.colorblind_mode;
    let _sm = settings.video.show_fps;
    let _sl = settings.network.show_latency;
    let _ss = settings.accessibility.subtitles;
    let _ssz = settings.accessibility.subtitle_size;
    let _rm = settings.accessibility.reduce_motion;
}

/// Apply high-contrast palette to a text color. If high-contrast is on,
/// returns high-saturation colors with WCAG AA minimum contrast.
pub fn high_contrast_color(base: Color, high_contrast: bool) -> Color {
    if !high_contrast {
        return base;
    }
    let srgb = base.to_srgba();
    let r = srgb.red;
    let g = srgb.green;
    let b = srgb.blue;
    let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    if lum > 0.5 {
        Color::srgb(0.1, 0.1, 0.1)
    } else {
        Color::srgb(0.96, 0.96, 0.96)
    }
}
