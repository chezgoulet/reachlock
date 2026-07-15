//! Generic interaction (spec §14; S07 freeze, reused by S08). Every future
//! verb — talk, shop, helm, engineering, nav, log, fuel — goes through a
//! single `Interactable` component + the `InteractKind`. The interaction
//! *surface* stays one place (S07/S18 gotcha: "keep `Interactable`
//! generic, not shop-specific"). A tiny router maps an `Interactable`'s
//! `kind` to the panel it opens; the panels themselves live in their own
//! systems.
//!
//! Bevy 0.18 dropped `EventReader`/`EventWriter`; interaction is resolved
//! inline in `try_interact` (no event plumbing needed for a single nearest
//! target).

use bevy::prelude::*;

use crate::systems::mode::PlayerAvatar;

/// What kind of thing you can interact with. Pure data — no behaviour. The
/// router turns this into an `ActivePanel`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum InteractKind {
    Talk,
    Shop,
    Crew,
    Helm,
    Engineering,
    Nav,
    Log,
    Fuel,
    Board,
    Launch,
    TakeHelm,
    /// S09b consoles (spec §22): drive the ship's flight systems from OnBoard.
    Gunner,
    Scanner,
    Miner,
    Power,
    Unknown,
}

/// Placed in the world next to something the player can use. `label` is the
/// prompt text (`"Mara"`, `"MARKET"`, …); `kind` selects the panel.
#[derive(Component, Clone, Debug)]
pub struct Interactable {
    pub label: String,
    pub kind: InteractKind,
}

/// An NPC figure (S07). Carries the authored/seed dialogue the talk verb
/// surfaces, so the dialogue panel can read it off the entity without a
/// second lookup. Souls arrive in S13; here it's just name + lines.
#[derive(Component, Clone, Debug)]
pub struct Npc {
    pub name: String,
    pub dialogue: Vec<String>,
}

/// The interaction the avatar is currently in reach of: the prompt string
/// (`"E: Mara"`), and the target's world position for the highlight ring.
/// Rendered by `hud::update_hud_status` (text) and
/// `interior::highlight_interactable` (ring).
#[derive(Resource, Default)]
pub struct InteractionPrompt {
    pub text: Option<String>,
    pub target: Option<Vec2>,
}

/// Which interaction panel (if any) is currently open. Set by `try_interact`
/// on `E`; cleared by `pause::toggle_pause` (Esc). Drives the HUD.
#[derive(Resource, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ActivePanel {
    #[default]
    None,
    Dialogue(Entity),
    Market,
    Helm,
    Engineering,
    Nav,
    Log,
    Fuel,
    Order(Entity),
    /// S09b console panels (spec §22).
    Gunner,
    Scanner,
    Miner,
    Power,
    Unknown,
}

/// How close (world units) the avatar must be to an `Interactable` to use it.
const REACH: f32 = 26.0;

/// Detect the nearest `Interactable` in reach of the avatar, show its prompt,
/// and on `E` open the matching panel (router inline — Bevy 0.18 has no
/// `EventReader`). Runs only in interior modes (wired in `main.rs` under
/// `in_any_interior`).
pub fn try_interact(
    keys: Res<ButtonInput<KeyCode>>,
    avatar: Query<&Transform, With<PlayerAvatar>>,
    interactables: Query<(Entity, &Transform, &Interactable)>,
    mut prompt: ResMut<InteractionPrompt>,
    mut panel: ResMut<ActivePanel>,
) {
    let Ok(av) = avatar.single() else {
        prompt.text = None;
        prompt.target = None;
        return;
    };
    let av_pos = av.translation.truncate();

    let mut nearest: Option<(f32, Entity, String, InteractKind, Vec2)> = None;
    for (e, t, inter) in &interactables {
        let pos = t.translation.truncate();
        let d = pos.distance(av_pos);
        if d <= REACH {
            let better = match &nearest {
                None => true,
                Some(n) => d < n.0,
            };
            if better {
                nearest = Some((d, e, inter.label.clone(), inter.kind, pos));
            }
        }
    }

    match nearest {
        Some((_, e, label, kind, pos)) => {
            prompt.text = Some(format!("[E] {label}"));
            prompt.target = Some(pos);
            if keys.just_pressed(KeyCode::KeyE) && *panel == ActivePanel::None {
                *panel = match kind {
                    InteractKind::Talk => ActivePanel::Dialogue(e),
                    InteractKind::Shop => ActivePanel::Market,
                    InteractKind::Crew => ActivePanel::Order(e),
                    InteractKind::Helm | InteractKind::TakeHelm => ActivePanel::Helm,
                    InteractKind::Engineering => ActivePanel::Engineering,
                    InteractKind::Nav => ActivePanel::Nav,
                    InteractKind::Log => ActivePanel::Log,
                    InteractKind::Fuel => ActivePanel::Fuel,
                    InteractKind::Gunner => ActivePanel::Gunner,
                    InteractKind::Scanner => ActivePanel::Scanner,
                    InteractKind::Miner => ActivePanel::Miner,
                    InteractKind::Power => ActivePanel::Power,
                    _ => ActivePanel::Unknown,
                };
            }
        }
        None => {
            prompt.text = None;
            prompt.target = None;
        }
    }
}
