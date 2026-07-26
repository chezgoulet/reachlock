use bevy::prelude::*;

use crate::settings::Settings;

/// Resource tracking whether high-contrast UI mode is active.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct HighContrastMode(pub bool);

/// Apply the accessibility settings that have a real effect: UI text scale and
/// the high-contrast flag.
///
/// This system existed but was registered nowhere, so `text_scale` and
/// `high_contrast_ui` did nothing at all — the sliders moved and the game did
/// not change.
///
/// It used to end with a block of `let _cb = settings.accessibility.…;` reads
/// whose only purpose was to satisfy `settings_consumer_registry`'s
/// completeness test. That is a gate measuring the wrong thing: a discarded
/// read is not a consumer. Those settings are listed in
/// `docs/sprints/00-INDEX.md` as genuinely unconsumed instead of being
/// disguised here.
pub fn sync_accessibility_settings(
    settings: Res<Settings>,
    mut ui_scale: ResMut<UiScale>,
    mut high_contrast: ResMut<HighContrastMode>,
) {
    if !settings.is_changed() {
        return;
    }
    ui_scale.0 = settings.video.ui_scale * settings.accessibility.text_scale;
    high_contrast.0 = settings.accessibility.high_contrast_ui;
}

// `high_contrast_color` was removed. It recomputed a contrasting colour at the
// call site, which is exactly what `make check-theme` forbids: UI colour comes
// from `assets/ui/phosphor.ron` and nowhere else. High contrast belongs in the
// stylesheet as a variant the theme loader selects on `HighContrastMode`, not
// as a runtime transform sprinkled over call sites.
