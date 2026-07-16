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

use crate::states::{CurrentLocation, GameMode};
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
    /// Mode transitions, discoverable in the world: the parked ship boards,
    /// the airlock hatch disembarks, the pilot seat takes the helm.
    Board,
    Disembark,
    Launch,
    TakeHelm,
    /// Climb between the ship's decks (rebuilds the interior scene on the
    /// other deck, keeping position).
    Ladder,
    /// A cryo pod (SHIPS.md §3): with a jump armed, climbing in beats the
    /// clock. Without one, the pod stays open.
    CryoPod,
    /// A compartment fire (SHIPS.md §4): E is one extinguisher action.
    FightFire,
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
    /// Where the currently open panel was opened from. Walking away from
    /// this point closes the panel (`LEAVE_RANGE`), so a conversation
    /// doesn't stay locked after you've left it.
    pub anchor: Option<Vec2>,
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
    /// S12 galactic news feed.
    News,
    Unknown,
}

/// How close (world px) the avatar must be to an `Interactable` to use it —
/// about 2.5 tiles at the pixel-art scale.
const REACH: f32 = 40.0;

/// How far (world px) the avatar can drift from the spot an interaction was
/// opened at before the panel closes on its own — wider than `REACH` so a
/// small shuffle doesn't drop a conversation, but walking off breaks the
/// focus without needing Esc.
const LEAVE_RANGE: f32 = 64.0;

/// Detect the nearest `Interactable` in reach of the avatar, show its prompt,
/// and on `E` open the matching panel (router inline — Bevy 0.18 has no
/// `EventReader`). Mode-transition kinds (Board / Disembark / TakeHelm) set
/// the next `GameMode` instead of opening a panel, so moving between the
/// station, the ship, and the helm is a visible thing you walk up to and
/// use — not a hidden keybind. Runs only in interior modes (wired in
/// `main.rs` under `in_any_interior`).
#[allow(clippy::too_many_arguments)]
pub fn try_interact(
    keys: Res<ButtonInput<KeyCode>>,
    avatar: Query<&Transform, With<PlayerAvatar>>,
    interactables: Query<(Entity, &Transform, &Interactable)>,
    mut prompt: ResMut<InteractionPrompt>,
    mut panel: ResMut<ActivePanel>,
    mut location: ResMut<CurrentLocation>,
    mut next: ResMut<NextState<GameMode>>,
    mut deck: ResMut<crate::systems::interior::ActiveDeck>,
    mut registry: ResMut<crate::states::SceneRegistry>,
    dialogue: Res<crate::systems::dialogue::DialogueSession>,
    mut plan: ResMut<crate::systems::cryojump::JumpPlan>,
    mut log: ResMut<crate::systems::contract::ShipLog>,
    mut fires: ResMut<crate::systems::crisis::ShipFires>,
    fire_refs: Query<&crate::systems::crisis::FireRef>,
) {
    // S16: free-input typing owns the keyboard (E would re-interact).
    if dialogue.typing() {
        return;
    }
    let Ok(av) = avatar.single() else {
        prompt.text = None;
        prompt.target = None;
        return;
    };
    let av_pos = av.translation.truncate();

    // A panel keeps focus only while you stay near what opened it. Walking
    // away breaks the conversation (Esc still works too), so interactions
    // never stay locked behind a panel you've left behind.
    if *panel != ActivePanel::None {
        match prompt.anchor {
            Some(anchor) if av_pos.distance(anchor) > LEAVE_RANGE => {
                *panel = ActivePanel::None;
                prompt.anchor = None;
            }
            _ => {}
        }
    } else {
        prompt.anchor = None;
    }

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
                match kind {
                    // Mode transitions — no panel, the world changes.
                    InteractKind::Board => {
                        location.is_docked = true;
                        // Boarding always puts you on the airlock deck.
                        deck.index = 0;
                        deck.spawn = None;
                        next.set(GameMode::OnBoard);
                    }
                    InteractKind::Ladder => {
                        // Climb: flip decks, come out beside the ladder.
                        // Clearing the registry makes `enter_interior`
                        // rebuild the scene on the new deck next frame.
                        deck.index = 1 - deck.index.min(1);
                        deck.spawn = Some(pos + Vec2::new(0.0, -24.0));
                        registry.scene = None;
                    }
                    InteractKind::Disembark => {
                        // Only meaningful hard-docked at a station.
                        if location.is_docked {
                            next.set(GameMode::Landed);
                        }
                    }
                    InteractKind::TakeHelm => {
                        next.set(GameMode::SpaceFlight);
                    }
                    InteractKind::FightFire => {
                        crate::systems::crisis::fight_fire_at(e, &fire_refs, &mut fires, &mut log);
                    }
                    InteractKind::CryoPod => {
                        // SHIPS.md §3 step 2: reaching the pod before the
                        // window opens is the whole game of the jump clock.
                        if plan.armed.is_some() {
                            plan.player_in_pod = true;
                            log.log(
                                "You seal the pod. The cold comes up through \
                                 the lining like a tide.",
                            );
                        } else {
                            log.log(
                                "The pod stays open — no jump is programmed. \
                                 (Arm one at the NAV console.)",
                            );
                        }
                    }
                    kind => {
                        prompt.anchor = Some(pos);
                        *panel = match kind {
                            InteractKind::Talk => ActivePanel::Dialogue(e),
                            InteractKind::Shop => ActivePanel::Market,
                            InteractKind::Crew => ActivePanel::Order(e),
                            InteractKind::Helm => ActivePanel::Helm,
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
            }
        }
        None => {
            prompt.text = None;
            prompt.target = None;
        }
    }
}
