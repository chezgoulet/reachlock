//! Shared panel widgets (S94).
//!
//! **Adoption is pending.** S94 delivered this abstraction to replace the
//! "build a String, assign to Text, read ButtonInput inline" pattern copied
//! across `factions`, `discovery`, `career`, `log_ui`, `culture_view`,
//! `mission_board`, `contract_crafting`, `contract_library` and `settings_ui`.
//! The abstraction shipped; not one panel was ported to it, so every widget
//! here is unconstructed.
//!
//! The allow below is deliberately at module scope and nowhere wider: it names
//! exactly the code waiting on that port, and leaves the compiler's dead-code
//! check active over the rest of the client. Porting the panels is what
//! removes it — see `docs/sprints/S94-panel-widget-abstraction.md`.
#![allow(dead_code)]

pub mod button;
pub mod dropdown;
pub mod list;
pub mod panel;
pub mod scroll_area;
pub mod slider;
pub mod text_input;
pub mod toggle;
pub mod tooltip;

pub struct WidgetKit;
