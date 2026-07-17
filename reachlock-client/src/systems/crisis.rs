//! On-board crises, client side (S09f, docs/SHIPS.md §4): hull hits in the
//! flight view land INSIDE the ship as compartment fires. Fires grow and
//! spread (core `crisis` model, deterministic), pull crew off their
//! stations, force power triage in engineering, and are fought room by
//! room — or vented, if the compartment runs zero-g and you can live with
//! what that means. A ship on fire is a ship not shooting back.

use bevy::prelude::*;

use reachlock_core::crisis::{
    crisis_roll, effective_power_budget, CrisisEvent, FireState, SystemDamage, SystemState,
};
use reachlock_core::generator::{GeneratedLayout, RoomKind};
use reachlock_core::soul::SoulEvent;

use crate::pixel;
use crate::states::GameMode;
use crate::systems::contract::ShipLog;
use crate::systems::crew::CrewRoster;
use crate::systems::interaction::{InteractKind, Interactable};
use crate::systems::interior::{ysort, ActiveDeck, CurrentInterior};
use crate::systems::ship::{ShipCommand, ShipSystems, POWER_BUDGET};
use crate::systems::soul::SoulRegistry;
use crate::systems::ticker::UniverseTicker;

/// Seconds between crisis ticks (fire growth/spread).
const CRISIS_TICK_SECS: f32 = 1.0;
/// Hull damage (fixed-point per hit) below which a hit can't start a fire.
const IGNITION_DAMAGE_FLOOR: i64 = 80;
/// Chance in 1024 that a qualifying hit ignites a compartment.
const IGNITION_CHANCE: u64 = 480;

/// The ship's live fires + the crisis clock.
#[derive(Resource)]
pub struct ShipFires {
    pub state: FireState,
    /// S16B: per-system damage (SHIPS.md §4) — fires mark the room's
    /// system Damaged/Disabled; repair happens at that system's console.
    pub systems: SystemDamage,
    timer: Timer,
    /// Monotonic crisis-tick counter feeding the deterministic rolls (the
    /// universe tick advances too slowly to salt per-second spread).
    crisis_tick: u64,
}

impl Default for ShipFires {
    fn default() -> Self {
        ShipFires {
            state: FireState::default(),
            systems: SystemDamage::default(),
            timer: Timer::from_seconds(CRISIS_TICK_SECS, TimerMode::Repeating),
            crisis_tick: 0,
        }
    }
}

impl ShipFires {
    /// Fixed-point effectiveness of the system owned by `room`'s station,
    /// as an f32 multiplier for the render layer.
    pub fn effectiveness(&self, room: RoomKind) -> f32 {
        self.systems.state(room).factor() as f32 / 1024.0
    }
}

/// Marks a rendered fire; carries the deck/room it burns in so the fight
/// interaction knows what it's putting out.
#[derive(Component)]
pub struct FireRef {
    pub deck: usize,
    pub room: usize,
}

/// The Loup-Garou's deck layouts (unscaled grid — indices and doors are
/// what the fire model needs).
fn deck_layouts() -> Vec<GeneratedLayout> {
    reachlock_core::generator::ship::loup_garou_interior()
        .decks
        .into_iter()
        .map(|d| d.layout)
        .collect()
}

fn room_name(layouts: &[GeneratedLayout], deck: usize, room: usize) -> String {
    layouts
        .get(deck)
        .and_then(|l| l.rooms.get(room))
        .map(|r| format!("{:?}", r.kind).to_uppercase())
        .unwrap_or_else(|| "COMPARTMENT".into())
}

/// Hull hits in flight can start fires below decks (SHIPS.md §4: "damage
/// in the flight view lands on the interior").
pub fn ignite_from_damage(
    systems: Res<ShipSystems>,
    location: Res<crate::states::CurrentLocation>,
    mut fires: ResMut<ShipFires>,
    mut log: ResMut<ShipLog>,
    mut prev_hp: Local<Option<i64>>,
) {
    let hp = systems.hull_hp.0;
    let last = prev_hp.replace(hp).unwrap_or(hp);
    let damage = last - hp;
    if damage < IGNITION_DAMAGE_FLOOR {
        return;
    }
    let salt = fires.crisis_tick;
    let roll = crisis_roll(location.system_seed, salt, hp as u64, 0);
    if roll % 1024 >= IGNITION_CHANCE {
        return;
    }
    let layouts = deck_layouts();
    let deck =
        (crisis_roll(location.system_seed, salt, hp as u64, 1) % layouts.len() as u64) as usize;
    let rooms = layouts[deck].rooms.len().max(1) as u64;
    let room = (crisis_roll(location.system_seed, salt, hp as u64, 2) % rooms) as usize;
    fires.state.ignite(deck, room, 192 + damage / 4);
    log.log(format!(
        "FIRE — {} ({} deck). Someone get on it.",
        room_name(&layouts, deck, room),
        if deck == 0 { "gravity" } else { "zero-g" }
    ));
}

/// The crisis clock: fires grow, spread through doors, burn systems, pull
/// crew off stations, and force power triage when engineering burns.
#[allow(clippy::too_many_arguments)]
pub fn tick_fires(
    time: Res<Time>,
    mut fires: ResMut<ShipFires>,
    location: Res<crate::states::CurrentLocation>,
    mut roster: ResMut<CrewRoster>,
    mut souls: ResMut<SoulRegistry>,
    ticker: Res<UniverseTicker>,
    mut command: ResMut<ShipCommand>,
    mut log: ResMut<ShipLog>,
) {
    if !fires.timer.tick(time.delta()).is_finished() || fires.state.burning.is_empty() {
        return;
    }
    fires.crisis_tick += 1;
    let crisis_tick = fires.crisis_tick;
    let layouts = deck_layouts();

    let mut events = Vec::new();
    for (deck, layout) in layouts.iter().enumerate() {
        events.extend(
            fires
                .state
                .tick_deck(deck, layout, location.system_seed, crisis_tick),
        );
    }
    let room_kind = |deck: usize, room: usize| {
        layouts
            .get(deck)
            .and_then(|l| l.rooms.get(room))
            .map(|r| r.kind)
    };
    for event in &events {
        match event {
            CrisisEvent::Spread { deck, to, .. } => log.log(format!(
                "The fire spreads — {} is burning.",
                room_name(&layouts, *deck, *to)
            )),
            CrisisEvent::SystemsBurning { deck, room } => {
                // S16B: the room's system is now actually Damaged — the
                // station it names runs below capacity until repaired.
                if let Some(kind) = room_kind(*deck, *room) {
                    fires.systems.mark(kind, SystemState::Damaged);
                }
                log.log(format!(
                    "{} systems are cooking off — degraded until repaired at the console.",
                    room_name(&layouts, *deck, *room)
                ));
            }
            CrisisEvent::BurnedOut { deck, room } => {
                if let Some(kind) = room_kind(*deck, *room) {
                    fires.systems.mark(kind, SystemState::Disabled);
                }
                log.log(format!(
                    "{} burns out — its systems are DOWN until rebuilt at the console.",
                    room_name(&layouts, *deck, *room)
                ));
            }
            _ => {}
        }
    }

    // Crew interruption (the Among Us / Sea of Thieves loop): a body in a
    // burning room abandons its station for quarters, hurt and saying so.
    let burning_kinds: Vec<(usize, RoomKind)> = fires
        .state
        .burning
        .keys()
        .filter_map(|(deck, room)| {
            layouts
                .get(*deck)
                .and_then(|l| l.rooms.get(*room))
                .map(|r| (*deck, r.kind))
        })
        .collect();
    let mut someone_hurt = false;
    for member in roster.members.iter_mut() {
        if burning_kinds
            .iter()
            .any(|(_, kind)| *kind == member.current_room)
            && member.order != Some(RoomKind::Quarters)
        {
            someone_hurt = true;
            member.order = Some(RoomKind::Quarters);
            log.log(format!(
                "{} is pulled off station by the fire.",
                member.name
            ));
            let event = SoulEvent {
                event_type: "crew_member_injured".into(),
                player_involved: false,
                emotional_weight: 700,
                timestamp: ticker.state.tick_no,
                summary: format!("Burned at the {:?} station.", member.current_room),
                fields: Default::default(),
                relationship_deltas: vec![],
            };
            let id = member.id.clone();
            for output in souls.apply(&id, &event) {
                crate::systems::soul::log_soul_output(&mut log, &output);
            }
        }
    }
    if someone_hurt {
        // The rest of the crew feels it too (Boris's protective trigger).
        let witness = SoulEvent {
            event_type: "crew_member_injured".into(),
            player_involved: false,
            emotional_weight: 400,
            timestamp: ticker.state.tick_no,
            summary: "A crewmate was hurt in a compartment fire.".into(),
            fields: Default::default(),
            relationship_deltas: vec![],
        };
        let ids: Vec<String> = roster.members.iter().map(|m| m.id.clone()).collect();
        for id in ids {
            for output in souls.apply(&id, &witness) {
                crate::systems::soul::log_soul_output(&mut log, &output);
            }
        }
    }

    // Power triage (SHIPS.md §4): a burning reactor room cuts the budget;
    // routing sheds engines first, then sensors, then weapons — diverting
    // power IS the "stop what you're doing" moment.
    let deck_refs: Vec<&GeneratedLayout> = layouts.iter().collect();
    let effective = effective_power_budget(POWER_BUDGET, &fires.state, &deck_refs);
    let mut used = command.power_weapons + command.power_engines + command.power_sensors;
    let mut shed = false;
    while used > effective {
        if command.power_engines > 0 {
            command.power_engines -= 1;
        } else if command.power_sensors > 0 {
            command.power_sensors -= 1;
        } else if command.power_weapons > 0 {
            command.power_weapons -= 1;
        } else {
            break;
        }
        used -= 1;
        shed = true;
    }
    if shed {
        log.log(format!(
            "Engineering sheds load — reactor budget down to {effective} while the fire burns."
        ));
    }
}

/// Keep a flame sprite (and its fight interaction) on every burning room of
/// the ACTIVE deck. Fires on the other deck burn unseen — the log is the
/// only smoke you get through a sealed hatch.
pub fn sync_fire_sprites(
    fires: Res<ShipFires>,
    deck: Res<ActiveDeck>,
    interior: Res<CurrentInterior>,
    mode: Option<Res<State<GameMode>>>,
    existing: Query<(Entity, &FireRef)>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
) {
    let on_board = mode.is_some_and(|m| **m == GameMode::OnBoard);
    // Despawn sprites for rooms that stopped burning or the wrong deck/mode.
    for (entity, fire_ref) in &existing {
        if !on_board
            || fire_ref.deck != deck.index
            || !fires.state.is_burning(fire_ref.deck, fire_ref.room)
        {
            commands.entity(entity).despawn();
        }
    }
    if !on_board {
        return;
    }
    let Some(layout) = &interior.layout else {
        return;
    };
    for (fire_deck, room) in fires.state.burning.keys() {
        if *fire_deck != deck.index {
            continue;
        }
        if existing
            .iter()
            .any(|(_, f)| f.deck == *fire_deck && f.room == *room)
        {
            continue;
        }
        // The scaled active-deck layout shares room indices with the grid
        // layout the fire model walks.
        let Some(r) = layout.rooms.get(*room) else {
            continue;
        };
        let x = r.x as f32 + r.width as f32 * 0.5;
        let y = r.y as f32 + r.height as f32 * 0.35;
        commands.spawn((
            Sprite {
                image: images.add(pixel::fire_sprite()),
                ..default()
            },
            Transform::from_xyz(x, y, ysort(y)).with_scale(Vec3::splat(2.0)),
            crate::states::ModeScope(GameMode::OnBoard),
            FireRef {
                deck: *fire_deck,
                room: *room,
            },
            Interactable {
                label: "Fire — fight it".to_string(),
                kind: InteractKind::FightFire,
            },
        ));
    }
}

/// One extinguisher action against the fire the avatar is standing at
/// (called from `interaction::try_interact` on `E`).
pub fn fight_fire_at(
    entity: Entity,
    fire_refs: &Query<&FireRef>,
    fires: &mut ShipFires,
    log: &mut ShipLog,
) {
    let Ok(fire_ref) = fire_refs.get(entity) else {
        return;
    };
    match fires.state.fight(fire_ref.deck, fire_ref.room) {
        Some(CrisisEvent::Extinguished { .. }) => {
            log.log("The fire dies under the foam. The bulkheads tick as they cool.")
        }
        _ => log.log(format!(
            "You knock the fire back ({}%). It isn't done.",
            fires.state.intensity(fire_ref.deck, fire_ref.room) * 100 / 1024
        )),
    }
}
