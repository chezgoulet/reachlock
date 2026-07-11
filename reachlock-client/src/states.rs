//! Application states (spec §9). v0 ships two; Docked, JumpTransition and
//! friends arrive with their systems — an enum variant without a system is
//! a lie the compiler can't catch.

use bevy::prelude::*;

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    MainMenu,
    Playing,
}
