//! The jump-cryo loop (docs/SHIPS.md §3 — signature gameplay): a
//! self-generated jump is programmed at the nav station, a jump clock
//! opens the window, and every *human* aboard must reach a cryo pod before
//! it does. Androids and robots run the crossing — Prudence's canonical
//! role — and revive the sleepers on emergence.
//!
//! The tension is the clock versus bodies reaching pods. A human awake
//! when the window opens is not a rules violation; it is vascular and
//! psychological ruin (docs/LORE.md §III) — the player's death/respawn
//! beat, or a crew member's trauma, both recoverable, neither silent.

use bevy::prelude::*;

use reachlock_core::agency::{consider_order, Consideration, Dispatch};
use reachlock_core::contract::engine::EvalContext;
use reachlock_core::generator::transit::transit_destination;
use reachlock_core::soul::SoulEvent;

use crate::pixel::{self, BodyKind};
use crate::states::GameMode;
use crate::systems::contract::ShipLog;
use crate::systems::crew::CrewRoster;
use crate::systems::jump::{TransitState, TRANSIT_SECS};
use crate::systems::mode::PlayerAvatar;
use crate::systems::ship::ShipSystems;
use crate::systems::soul::SoulRegistry;
use crate::systems::ticker::UniverseTicker;

/// Seconds from arming the jump to the window opening. The walk from the
/// bridge to the cryo chamber is most of it — that's the game.
pub const JUMP_WINDOW_SECS: f32 = 30.0;

/// The armed jump: destination and the closing window.
pub struct ArmedJump {
    pub dest_seed: u64,
    pub window: Timer,
}

/// The ship's jump plan. `Idle` between jumps; `armed` while the clock
/// runs; `cryo_wake` marks an in-flight cryo transit so the wake lands in
/// the cryo chamber instead of the pilot seat.
#[derive(Resource, Default)]
pub struct JumpPlan {
    pub armed: Option<ArmedJump>,
    pub player_in_pod: bool,
    pub cryo_wake: bool,
}

/// Program + arm the jump from the nav console (called from the Nav panel
/// arm in `onboard.rs` on `J`). Orders every human crew member to the cryo
/// chamber and lets the dispatch hand the crossing to Prudence — who, being
/// an android, gets to consider it (S15).
pub fn arm_jump(
    plan: &mut JumpPlan,
    transit: &TransitState,
    system_seed: u64,
    roster: &mut CrewRoster,
    log: &mut ShipLog,
    feed: &mut crate::systems::comms::CommFeed,
) {
    if plan.armed.is_some() || transit.active {
        return;
    }
    let dest_seed = transit_destination(system_seed, transit.jump_count);
    plan.armed = Some(ArmedJump {
        dest_seed,
        window: Timer::from_seconds(JUMP_WINDOW_SECS, TimerMode::Once),
    });
    plan.player_in_pod = false;
    // Every biological body gets the same order: pods, now.
    for member in roster.members.iter_mut() {
        let body = pixel::crew_look(&member.id).body;
        // Humans and xenotypes are biological and need cryo; androids, robots,
        // and voidborn are not biological life that the crossing harms.
        if body == BodyKind::Human || body == BodyKind::Xenotype {
            member.order = Some(reachlock_core::generator::RoomKind::Cryo);
        }
    }
    log.log(format!(
        "Jump programmed → system {dest_seed:#x}. Window opens in {JUMP_WINDOW_SECS:.0}s — all biologicals to cryo."
    ));
    // The dispatch routes the crossing. Prudence is an android: she gets to
    // consider it, and she checks her own sensors first (spec §18).
    let dispatch = Dispatch {
        contract_ids: vec!["cryo-pilot".into()],
    };
    if let Some(routed) = dispatch.route(
        reachlock_core::soul::types::Species::Android,
        "run the crossing",
    ) {
        let _ = routed;
        let ctx = EvalContext::default();
        match consider_order("run the crossing", &ctx) {
            Consideration::Accept => {
                log.log("Prudence: \"Vectors are mine. Go to sleep.\"");
                feed.say("Prudence", "Vectors are mine. Go to sleep.");
            }
            Consideration::Counter { rationale, .. } => {
                log.log(format!("Prudence: {rationale}"));
                feed.say("Prudence", rationale);
            }
        }
    }
}

/// Tick the jump clock. When the window opens: sleepers cross, the awake
/// are ruined — the player fatally (the respawn beat tells the vascular
/// story), human crew traumatically (a heavy soul event; the log says who
/// was awake). Then the crossing starts with Prudence at the helm.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn jump_clock(
    time: Res<Time>,
    mut plan: ResMut<JumpPlan>,
    mut transit: ResMut<TransitState>,
    mut systems: ResMut<ShipSystems>,
    roster: Res<CrewRoster>,
    mut souls: ResMut<SoulRegistry>,
    ticker: Res<UniverseTicker>,
    mut log: ResMut<ShipLog>,
    mut next: ResMut<NextState<GameMode>>,
    mode: Option<Res<State<GameMode>>>,
) {
    let Some(armed) = &mut plan.armed else {
        return;
    };
    if !armed.window.tick(time.delta()).is_finished() {
        return;
    }
    let dest_seed = armed.dest_seed;
    plan.armed = None;

    // The player is human. No pod, no partial credit (SHIPS.md §3).
    if !plan.player_in_pod {
        systems.dead = true;
        log.log(
            "The window opened with you awake. Capillary trauma is immediate; \
             what the crossing does to a conscious mind is worse. \
             (Emergency revival engaged — Lou keeps her captain.)",
        );
        // The jump itself is scrubbed: nobody flew it.
        return;
    }

    // Human and xenotype crew outside the cryo chamber cross awake — ruined,
    // not erased. Androids, robots, and voidborn are unaffected.
    for member in &roster.members {
        let body = pixel::crew_look(&member.id).body;
        if (body == BodyKind::Human || body == BodyKind::Xenotype)
            && member.current_room != reachlock_core::generator::RoomKind::Cryo
        {
            log.log(format!(
                "{} was awake for the crossing. They will not talk about it. \
                 They may not be able to.",
                member.name
            ));
            let event = SoulEvent {
                event_type: "awake_crossing".into(),
                player_involved: true,
                emotional_weight: 1000,
                timestamp: ticker.state.tick_no,
                summary: "Crossed hyperspace conscious and unshielded.".into(),
                fields: Default::default(),
                relationship_deltas: vec![("player".into(), -200, 0)],
            };
            for output in souls.apply(&member.id, &event) {
                crate::systems::soul::log_soul_output(&mut log, &output);
            }
        }
    }

    // The synthetic crew runs the crossing (SHIPS.md §3 step 3).
    transit.active = true;
    transit.anomaly_fired = false;
    transit.dest_seed = dest_seed;
    transit.timer = Timer::from_seconds(TRANSIT_SECS, TimerMode::Once);
    transit.pilot = "Prudence".into();
    plan.cryo_wake = true;
    log.log("The pods seal. Prudence has the ship. The Loup-Garou goes through.");
    // Entering Hyperspace from any mode is legal; the enter teardown scopes
    // out whatever scene was live.
    if mode.is_some() {
        next.set(GameMode::Hyperspace);
    }
}

/// While the player is in a pod, the avatar neither renders nor walks.
pub fn pod_stasis(plan: Res<JumpPlan>, mut avatar: Query<&mut Visibility, With<PlayerAvatar>>) {
    if let Ok(mut vis) = avatar.single_mut() {
        *vis = if plan.player_in_pod {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
}

/// Revival (SHIPS.md §3 step 4): called by `jump::hyperspace_tick` when a
/// cryo transit completes. Returns the log beats; the caller routes the
/// wake into the cryo chamber.
pub fn revive(
    plan: &mut JumpPlan,
    roster: &mut CrewRoster,
    log: &mut ShipLog,
    feed: &mut crate::systems::comms::CommFeed,
) {
    plan.cryo_wake = false;
    plan.player_in_pod = false;
    for member in roster.members.iter_mut() {
        if member.order == Some(reachlock_core::generator::RoomKind::Cryo) {
            member.order = None; // back to their shifts
        }
    }
    log.log("The pods open. Med-bay light, recycled antiseptic — arrival.");
    log.log("Prudence: \"Crossing complete. All sleepers viable. You are welcome.\"");
    feed.say(
        "Prudence",
        "Crossing complete. All sleepers viable. You are welcome.",
    );
}
