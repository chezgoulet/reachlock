//! Ship Exterior & Interior Editor (spec §19; S17/S18).
//!
//! Opened from a SHIPYARD terminal while docked. Two sub-editors share the
//! frame resolution and the applied ship config. Keyboard-driven: Tab cycles
//! tabs, W/S selects a row, A/D cycles the choice, Enter/Space applies.
//!
//! Split from the monolithic `shipeditor.rs` in the Stage 3 audit refactor.

pub mod exterior;
pub mod interior;

use bevy::prelude::*;

use reachlock_core::content::{AssetType, ContentPayload};
use reachlock_core::editor::exterior::{HullConfiguration, HullFrame};
use reachlock_core::generator::hull::HullClass;

use crate::systems::content_index::ContentIndex;

// ---------------------------------------------------------------------------
// Shared types
// ---------------------------------------------------------------------------

/// The APPLIED exterior configuration — what the flight ship spawns from —
/// plus its derived handling (cached so `ship::control` doesn't recompose
/// every frame). `None` = the pre-S17 default Loup-Garou.
#[derive(Resource, Default)]
pub struct ShipConfig {
    pub config: Option<HullConfiguration>,
    pub handling: Option<reachlock_core::generator::hull::HullHandling>,
}

impl ShipConfig {
    pub fn set(&mut self, config: HullConfiguration, content: &ContentIndex) {
        let frame = frame_for(content, &config.hull_id);
        self.handling = Some(reachlock_core::editor::exterior::handling(&config, &frame));
        self.config = Some(config);
    }
}

// ---------------------------------------------------------------------------
// Re-exports for backward compatibility — callers that used
// `crate::systems::shipeditor::Thing` still compile.
// ---------------------------------------------------------------------------
pub use exterior::{
    default_config, editor_panel_text, editor_preview, editor_system, refit_cost, EditorTab,
    ExteriorPreview, ShipEditorState,
};
pub use interior::{
    interior_editor_preview, interior_editor_system, interior_panel_text,
    templates_for, InteriorConfig, InteriorEditorState,
};
pub const FRAME_IDS: [(&str, HullClass); 3] = [
    ("frame_shuttle", HullClass::Shuttle),
    ("frame_corvette", HullClass::Corvette),
    ("frame_freighter", HullClass::Freighter),
];

/// Resolve a frame by id: authored content first, reference fallback.
pub fn frame_for(content: &ContentIndex, hull_id: &str) -> HullFrame {
    for file in &content.files {
        if file.asset_type == AssetType::HullFrame && file.id == hull_id {
            if let ContentPayload::HullFrame(frame) = &file.payload {
                return frame.clone();
            }
        }
    }
    let class = FRAME_IDS
        .iter()
        .find(|(id, _)| *id == hull_id)
        .map(|(_, class)| *class)
        .unwrap_or(HullClass::Corvette);
    HullFrame::reference(class)
}
