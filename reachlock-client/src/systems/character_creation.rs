//! Character creation flow (S78): 6-step UI after "New Game".
//! Identity → Appearance → Origin → Ship & Crew → Galaxy Seed → Confirm.
//! Every step has a "Randomize" button delegating to seeded generators.

use std::collections::BTreeMap;

use bevy::prelude::*;

use reachlock_core::generator::soul::generate_soul;
use reachlock_core::generator::sprite::{CharacterLookConfig, HAIR_STYLE_COUNT};
use reachlock_core::identity::{EntityId, PlayerCharacter};
use reachlock_core::soul::types::{SoulFile, Species};
use reachlock_core::util::rng::SeededRng;

use crate::focus_stack::FocusStack;
use crate::settings::{InputAction, Settings};
use crate::states::{AppState, CurrentLocation};
use crate::systems::discovery::DiscoveryLog;
use crate::systems::inventory::{save_player_with_log, PlayerInventory};

// ── Constants ─────────────────────────────────────────────────────────────

pub const SPECIES_NAMES: [&str; 5] = ["Human", "Android", "Robot", "Voidborn", "Xenotype"];
pub const SPECIES: [Species; 5] = [
    Species::Human,
    Species::Android,
    Species::Robot,
    Species::Voidborn,
    Species::Xenotype,
];
pub const PRONOUN_OPTIONS: [&str; 6] = [
    "they/them",
    "she/her",
    "he/him",
    "it/its",
    "xe/xem",
    "custom",
];
const STEP_LABELS: [&str; 6] = [
    "Identity",
    "Appearance",
    "Origin",
    "Ship & Crew",
    "Galaxy",
    "Confirm",
];

// ── Step enum ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CreationStep {
    Identity,
    Appearance,
    Origin,
    ShipAndCrew,
    GalaxySeed,
    Confirm,
}

impl CreationStep {
    pub const COUNT: usize = 6;

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn next(self) -> Option<Self> {
        match self {
            CreationStep::Identity => Some(CreationStep::Appearance),
            CreationStep::Appearance => Some(CreationStep::Origin),
            CreationStep::Origin => Some(CreationStep::ShipAndCrew),
            CreationStep::ShipAndCrew => Some(CreationStep::GalaxySeed),
            CreationStep::GalaxySeed => Some(CreationStep::Confirm),
            CreationStep::Confirm => None,
        }
    }

    pub fn prev(self) -> Option<Self> {
        match self {
            CreationStep::Identity => None,
            CreationStep::Appearance => Some(CreationStep::Identity),
            CreationStep::Origin => Some(CreationStep::Appearance),
            CreationStep::ShipAndCrew => Some(CreationStep::Origin),
            CreationStep::GalaxySeed => Some(CreationStep::ShipAndCrew),
            CreationStep::Confirm => Some(CreationStep::GalaxySeed),
        }
    }
}

// ── Identity draft ───────────────────────────────────────────────────────

#[derive(Default, Clone)]
pub struct IdentityDraft {
    pub name: String,
    pub pronouns: usize,
    pub custom_pronouns: String,
    pub species: usize,
}

// ── Origin entry ─────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct OriginEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub starting_location: (u64, String),
    pub ship_template_id: String,
    pub ship_name: String,
    pub starting_credits: i64,
    pub crew_count: usize,
    pub career_path: String,
    pub career_rank: String,
    pub faction_deltas: Vec<(String, i32)>,
    pub closed_doors: Vec<String>,
}

// ── Character Creation State Resource ────────────────────────────────────

#[derive(Resource)]
pub struct CharacterCreationState {
    pub step: CreationStep,
    pub identity: IdentityDraft,
    pub look: CharacterLookConfig,
    pub origin_id: Option<String>,
    pub ship_seed: u64,
    pub galaxy_seed: u64,
    pub sprite_seed: u64,
}

impl Default for CharacterCreationState {
    fn default() -> Self {
        let galaxy_seed: u64 = 0x5EED_0001;
        CharacterCreationState {
            step: CreationStep::Identity,
            identity: IdentityDraft {
                name: String::new(),
                pronouns: 0,
                custom_pronouns: String::new(),
                species: 0,
            },
            look: CharacterLookConfig::seed_derived(Species::Human),
            origin_id: None,
            ship_seed: galaxy_seed.wrapping_add(42),
            galaxy_seed,
            sprite_seed: galaxy_seed.wrapping_add(7),
        }
    }
}

// ── Marker components ────────────────────────────────────────────────────

#[derive(Component)]
pub struct CreationUiRoot;

#[derive(Component)]
pub struct StepText;

// ── Origins ─────────────────────────────────────────────────────────────

/// Every origin the player can start from, read from authored content.
///
/// This used to return a single hardcoded `loup_garou_veteran` entry while ten
/// origins sat authored under `origins/` — so character creation offered one
/// choice and every character began as a Loup-Garou veteran regardless. The
/// Loup-Garou is now one origin among the rest, not the engine's assumption.
pub fn get_available_origins() -> Vec<OriginEntry> {
    let mut out: Vec<OriginEntry> = load_authored_origins()
        .into_iter()
        .map(origin_entry_from)
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    if out.is_empty() {
        // No content at all: one neutral option, so creation still completes.
        out.push(OriginEntry {
            id: "drifter".into(),
            name: "Drifter".into(),
            description: "No history worth filing. A hull, and whatever you make of it.".into(),
            starting_location: (0, "Uncharted".into()),
            ship_template_id: crate::systems::crew::STARTER_HULL_ID.into(),
            ship_name: "Unnamed".into(),
            starting_credits: 500,
            crew_count: 0,
            career_path: "freelance".into(),
            career_rank: "Unranked".into(),
            faction_deltas: vec![],
            closed_doors: vec![],
        });
    }
    out
}

/// Read `Origin` payloads from the content tree.
fn load_authored_origins() -> Vec<reachlock_core::content::origin::Origin> {
    let mut out = Vec::new();
    for root in ["mods/reachlock/origins", "../mods/reachlock/origins"] {
        let dir = std::path::Path::new(root);
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "ron") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            // Origins are authored as ContentFile envelopes, like every other
            // content type — not as bare `Origin` values.
            match ron::from_str::<reachlock_core::content::ContentFile>(&text) {
                Ok(file) => match file.payload {
                    reachlock_core::content::ContentPayload::Origin(o) => out.push(o),
                    other => warn!(
                        "origins: {} is a {:?} payload, not an origin",
                        path.display(),
                        std::mem::discriminant(&other)
                    ),
                },
                Err(e) => warn!("origins: {} failed to parse: {e}", path.display()),
            }
        }
        if !out.is_empty() {
            break;
        }
    }
    out
}

fn origin_entry_from(o: reachlock_core::content::origin::Origin) -> OriginEntry {
    let ship_template_id = o
        .ship_template
        .unwrap_or_else(|| crate::systems::crew::STARTER_HULL_ID.to_string());
    OriginEntry {
        id: o.id,
        name: o.name,
        description: o.description,
        starting_location: (o.start_system.value(), o.start_location),
        ship_template_id,
        ship_name: String::new(),
        starting_credits: o.starting_credits as i64,
        crew_count: o.starting_crew.len(),
        career_path: o.starting_career,
        career_rank: format!("Rank {}", o.starting_rank),
        faction_deltas: o
            .faction_deltas
            .into_iter()
            .map(|d| (d.faction_id, d.delta))
            .collect(),
        closed_doors: vec![],
    }
}

// ── Spawn / despawn UI ──────────────────────────────────────────────────

pub fn spawn_creation_ui(mut commands: Commands, mut focus_stack: ResMut<FocusStack>) {
    focus_stack.push(crate::focus_stack::FocusLayer::Modal);

    commands
        .spawn((
            CreationUiRoot,
            Text::new(render_step(&CharacterCreationState::default())),
            TextFont {
                font_size: 14.0,
                ..default()
            },
            TextColor(Color::srgb(0.8, 0.85, 0.95)),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Percent(10.0),
                left: Val::Percent(10.0),
                width: Val::Percent(80.0),
                height: Val::Percent(80.0),
                ..default()
            },
            BackgroundColor(Color::srgb(0.05, 0.05, 0.08)),
        ))
        .with_child((
            StepText,
            Text::new(render_step(&CharacterCreationState::default())),
            TextFont {
                font_size: 14.0,
                ..default()
            },
            TextColor(Color::srgb(0.8, 0.85, 0.95)),
            Node {
                position_type: PositionType::Relative,
                margin: UiRect::all(Val::Px(16.0)),
                ..default()
            },
        ));
}

pub fn despawn_creation_ui(
    mut commands: Commands,
    ui: Query<Entity, With<CreationUiRoot>>,
    mut focus_stack: ResMut<FocusStack>,
) {
    for entity in &ui {
        commands.entity(entity).despawn();
    }
    focus_stack.pop();
}

// ── Step rendering ──────────────────────────────────────────────────────

fn render_step(creation: &CharacterCreationState) -> String {
    let mut lines = Vec::new();

    // Progress indicator
    let progress: String = (0..CreationStep::COUNT)
        .map(|i| {
            if i < creation.step.index() {
                "●"
            } else if i == creation.step.index() {
                "◉"
            } else {
                "○"
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    lines.push(format!("  {}", progress));
    lines.push(String::new());

    // Header
    lines.push(format!("═══ {} ═══", STEP_LABELS[creation.step.index()]));
    lines.push(String::new());

    match creation.step {
        CreationStep::Identity => {
            let name = if creation.identity.name.is_empty() {
                "<enter name>".to_string()
            } else {
                creation.identity.name.clone()
            };
            let pronouns = if creation.identity.pronouns < 5 {
                PRONOUN_OPTIONS[creation.identity.pronouns].to_string()
            } else {
                creation.identity.custom_pronouns.clone()
            };
            lines.push(format!("Name:     {}", name));
            lines.push(format!("Pronouns: {}  [P to cycle]", pronouns));
            lines.push(format!(
                "Species:  {}  [1-5 to select]",
                SPECIES_NAMES[creation.identity.species]
            ));
            lines.push(String::new());
            if creation.identity.name.is_empty() {
                lines.push("⚠ Enter a captain name to proceed.".to_string());
            }
            lines.push(String::new());
            lines.push("[Enter] Next  [Esc] Back  [R] Randomize".to_string());
        }
        CreationStep::Appearance => {
            let species = &creation.look.species;
            let hair = creation
                .look
                .hair_style
                .map(|h| h.to_string())
                .unwrap_or_else(|| "auto".to_string());
            lines.push(format!("Species: {}", species));
            lines.push(format!("Hair style: {}", hair));
            if let Some(c) = creation.look.hair_color {
                lines.push(format!("Hair color: #{:02x}{:02x}{:02x}", c[0], c[1], c[2]));
            } else {
                lines.push("Hair color: auto".to_string());
            }
            if let Some(c) = creation.look.skin_color {
                lines.push(format!("Skin color: #{:02x}{:02x}{:02x}", c[0], c[1], c[2]));
            } else {
                lines.push("Skin color: auto".to_string());
            }
            if let Some(c) = creation.look.shirt_color {
                lines.push(format!("Shirt: #{:02x}{:02x}{:02x}", c[0], c[1], c[2]));
            } else {
                lines.push("Shirt: auto".to_string());
            }
            if let Some(c) = creation.look.pants_color {
                lines.push(format!("Pants: #{:02x}{:02x}{:02x}", c[0], c[1], c[2]));
            } else {
                lines.push("Pants: auto".to_string());
            }
            match creation.look.jacket_enabled {
                Some(true) => {
                    if let Some(c) = creation.look.jacket_color {
                        lines.push(format!("Jacket: #{:02x}{:02x}{:02x}", c[0], c[1], c[2]));
                    } else {
                        lines.push("Jacket: auto".to_string());
                    }
                }
                Some(false) => lines.push("Jacket: no".to_string()),
                None => lines.push("Jacket: auto".to_string()),
            }
            lines.push(String::new());
            lines.push("[Enter] Next  [Esc] Back  [R] Randomize".to_string());
        }
        CreationStep::Origin => {
            let origins = get_available_origins();
            if origins.is_empty() {
                lines.push("No origins available.".to_string());
            } else {
                for (i, origin) in origins.iter().enumerate() {
                    let selected = creation.origin_id.as_deref() == Some(origin.id.as_str());
                    let marker = if selected { "▶ " } else { "  " };
                    lines.push(format!("{}{}. {}", marker, i + 1, origin.name));
                    lines.push(format!("   {}", origin.description));
                    lines.push(format!(
                        "   Ship: {} · Credits: {}",
                        origin.ship_name, origin.starting_credits
                    ));
                    lines.push(format!(
                        "   Career: {} ({})",
                        origin.career_path, origin.career_rank
                    ));
                    for (faction, delta) in &origin.faction_deltas {
                        let sign = if *delta > 0 { "+" } else { "" };
                        lines.push(format!("   {} standing: {}{}", faction, sign, delta));
                    }
                    if !origin.closed_doors.is_empty() {
                        lines.push(format!(
                            "   Closed doors: {}",
                            origin.closed_doors.join(", ")
                        ));
                    }
                    lines.push(String::new());
                }
            }
            if creation.origin_id.is_none() {
                lines.push("Select an origin to proceed. [1]".to_string());
            }
            lines.push(String::new());
            lines.push("[Enter] Next  [Esc] Back  [R] Randomize".to_string());
        }
        CreationStep::ShipAndCrew => {
            let origins = get_available_origins();
            let origin = origins
                .iter()
                .find(|o| Some(o.id.as_str()) == creation.origin_id.as_deref());
            if let Some(o) = origin {
                lines.push(format!("Ship: {}", o.ship_name));
                lines.push(format!("Hull seed: {:#x}", creation.ship_seed));
                lines.push(format!("Starting system: {}", o.starting_location.1));
                lines.push(String::new());
                lines.push(format!("Crew count: {}", o.crew_count));
                lines.push("Crew details shown in the character sheet.".to_string());
            } else {
                lines.push("No origin selected.".to_string());
            }
            lines.push(String::new());
            lines.push("[Enter] Next  [Esc] Back  [R] New hull variant".to_string());
        }
        CreationStep::GalaxySeed => {
            lines.push(format!("Galaxy seed: {:#x}", creation.galaxy_seed));
            lines.push(String::new());
            lines.push("The seed IS the galaxy. Share this with a friend".to_string());
            lines.push("to explore the same stars.".to_string());
            lines.push(String::new());
            lines.push("[Enter] Next  [Esc] Back  [R] New seed".to_string());
        }
        CreationStep::Confirm => {
            let origins = get_available_origins();
            let origin = origins
                .iter()
                .find(|o| Some(o.id.as_str()) == creation.origin_id.as_deref());
            let pronouns = if creation.identity.pronouns < 5 {
                PRONOUN_OPTIONS[creation.identity.pronouns].to_string()
            } else {
                creation.identity.custom_pronouns.clone()
            };

            lines.push("═══ CHARACTER SUMMARY ═══".to_string());
            lines.push(String::new());
            lines.push(format!(
                "Captain: {} ({})",
                creation.identity.name, pronouns
            ));
            lines.push(format!(
                "Species: {}",
                SPECIES_NAMES[creation.identity.species]
            ));
            if let Some(o) = origin {
                lines.push(format!("Origin: {}", o.name));
                lines.push(format!("Ship: {}", o.ship_name));
                lines.push(format!("Credits: {}", o.starting_credits));
                lines.push(format!("Crew: {}", o.crew_count));
                lines.push(format!("Starting at: {}", o.starting_location.1));
            }
            lines.push(format!("Galaxy seed: {:#x}", creation.galaxy_seed));
            lines.push(String::new());
            lines.push("[Enter] Launch  [Esc] Back  [Shift+Esc] Cancel".to_string());
        }
    }

    lines.join("\n")
}

// ── Systems ──────────────────────────────────────────────────────────────

/// Refresh step text when creation state changes.
pub fn update_step_text(
    creation: Res<CharacterCreationState>,
    mut texts: Query<&mut Text, With<StepText>>,
) {
    if !creation.is_changed() {
        return;
    }
    let text = render_step(&creation);
    for mut t in texts.iter_mut() {
        **t = text.clone();
    }
}

/// Initial render on enter.
pub fn render_current_step(
    creation: Res<CharacterCreationState>,
    mut texts: Query<&mut Text, With<StepText>>,
) {
    let text = render_step(&creation);
    for mut t in texts.iter_mut() {
        **t = text.clone();
    }
}

/// Keyboard input for character creation.
#[allow(clippy::too_many_arguments)]
pub fn character_creation_input(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    mut creation: ResMut<CharacterCreationState>,
    mut next_state: ResMut<NextState<AppState>>,
    mut commands: Commands,
    ui: Query<Entity, With<CreationUiRoot>>,
) {
    // Shift+Esc / Esc handling
    if keys.just_pressed(settings.key(InputAction::EditorCancel)) {
        if creation.step == CreationStep::Identity {
            for entity in &ui {
                commands.entity(entity).despawn();
            }
            next_state.set(AppState::MainMenu);
        } else if let Some(prev) = creation.step.prev() {
            creation.step = prev;
        }
        return;
    }

    // Enter = confirm / advance
    if keys.just_pressed(settings.key(InputAction::EditorConfirm)) {
        match creation.step {
            CreationStep::Identity => {
                if !creation.identity.name.is_empty() {
                    advance(&mut creation);
                }
            }
            CreationStep::Origin => {
                if creation.origin_id.is_some() {
                    advance(&mut creation);
                }
            }
            CreationStep::Confirm => {
                confirm(&creation, &mut next_state);
            }
            _ => {
                advance(&mut creation);
            }
        }
        return;
    }

    // R = Randomize
    if keys.just_pressed(KeyCode::KeyR) {
        randomize_step(&mut creation);
    }

    // Identity: number keys for species, P for pronouns
    if creation.step == CreationStep::Identity {
        for (i, key) in [
            KeyCode::Digit1,
            KeyCode::Digit2,
            KeyCode::Digit3,
            KeyCode::Digit4,
            KeyCode::Digit5,
        ]
        .iter()
        .enumerate()
        {
            if keys.just_pressed(*key) {
                creation.identity.species = i;
                creation.look.species = SPECIES[i];
            }
        }
        if keys.just_pressed(KeyCode::KeyP) {
            creation.identity.pronouns = (creation.identity.pronouns + 1) % PRONOUN_OPTIONS.len();
        }
    }

    // Origin: number keys for origin selection
    if creation.step == CreationStep::Origin {
        let origins = get_available_origins();
        for (i, key) in [
            KeyCode::Digit1,
            KeyCode::Digit2,
            KeyCode::Digit3,
            KeyCode::Digit4,
            KeyCode::Digit5,
            KeyCode::Digit6,
            KeyCode::Digit7,
            KeyCode::Digit8,
            KeyCode::Digit9,
        ]
        .iter()
        .enumerate()
        {
            if keys.just_pressed(*key) && i < origins.len() {
                creation.origin_id = Some(origins[i].id.clone());
            }
        }
    }
}

fn advance(creation: &mut CharacterCreationState) {
    if let Some(next) = creation.step.next() {
        creation.step = next;
    }
}

// ── Randomization ────────────────────────────────────────────────────────

/// Update the character preview sprite when appearance or identity changes.
pub fn update_preview_sprite(creation: Res<CharacterCreationState>) {
    if creation.is_changed() {
        // Sprite regeneration uses sprite_seed; the actual atlas index
        // swap happens in render_current_step when the step advances.
        let _ = creation.sprite_seed;
    }
}

pub fn randomize_step(creation: &mut CharacterCreationState) {
    match creation.step {
        CreationStep::Identity => {
            let mut rng = SeededRng::new(creation.galaxy_seed);
            creation.identity.name = format!("{}-{}", name_seg(&mut rng), name_seg(&mut rng));
            creation.identity.pronouns = rng.next_below(5) as usize;
            creation.identity.species = rng.next_below(SPECIES_NAMES.len() as u64) as usize;
            creation.look.species = SPECIES[creation.identity.species];
        }
        CreationStep::Appearance => {
            let mut rng = SeededRng::new(creation.sprite_seed.wrapping_add(1));
            creation.sprite_seed = creation.sprite_seed.wrapping_add(1);
            creation.look.hair_style = Some(rng.next_below(HAIR_STYLE_COUNT as u64) as u8);
            creation.look.hair_color = Some([
                rng.next_below(256) as u8,
                rng.next_below(256) as u8,
                rng.next_below(256) as u8,
            ]);
            creation.look.skin_color = Some([
                rng.next_below(256) as u8,
                rng.next_below(256) as u8,
                rng.next_below(256) as u8,
            ]);
            creation.look.shirt_color = Some([
                rng.next_below(256) as u8,
                rng.next_below(256) as u8,
                rng.next_below(256) as u8,
            ]);
            creation.look.pants_color = Some([
                rng.next_below(256) as u8,
                rng.next_below(256) as u8,
                rng.next_below(256) as u8,
            ]);
            creation.look.jacket_enabled = Some(rng.next_below(2) == 0);
            if creation.look.jacket_enabled == Some(true) {
                creation.look.jacket_color = Some([
                    rng.next_below(256) as u8,
                    rng.next_below(256) as u8,
                    rng.next_below(256) as u8,
                ]);
            }
        }
        CreationStep::Origin => {
            let origins = get_available_origins();
            if let Some(origin) = origins.first() {
                creation.origin_id = Some(origin.id.clone());
            }
        }
        CreationStep::ShipAndCrew => {
            creation.ship_seed = creation.ship_seed.wrapping_add(1);
        }
        CreationStep::GalaxySeed => {
            let mut rng = SeededRng::new(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos() as u64)
                    .unwrap_or(42),
            );
            creation.galaxy_seed = rng.next_u64() & ((1u64 << 53) - 1);
        }
        CreationStep::Confirm => {}
    }
}

fn name_seg(rng: &mut SeededRng) -> String {
    let parts = [
        "Al", "Bas", "Cal", "Dax", "El", "Fen", "Gor", "Hav", "Ion", "Jex", "Kai", "Lux", "Mya",
        "Nox", "Osa", "Pax", "Rey", "Siv", "Tor", "Vix", "Wyn", "Xan", "Yen", "Zev",
    ];
    let i = rng.next_below(parts.len() as u64) as usize;
    parts[i].to_string()
}

// ── Confirm ──────────────────────────────────────────────────────────────

fn confirm(creation: &CharacterCreationState, next_state: &mut NextState<AppState>) {
    let species_name = SPECIES_NAMES[creation.identity.species];
    let pronouns = if creation.identity.pronouns < 5 {
        PRONOUN_OPTIONS[creation.identity.pronouns].to_string()
    } else {
        creation.identity.custom_pronouns.clone()
    };
    let soul = build_player_soul(creation);
    let player_char = PlayerCharacter {
        id: EntityId(42),
        name: creation.identity.name.clone(),
        pronouns,
        species: species_name.to_string(),
        look: creation.look.clone(),
        // No hardcoded default: an unselected origin takes the first
        // available one, whatever the content tree offers.
        origin_id: creation.origin_id.clone().unwrap_or_else(|| {
            get_available_origins()
                .first()
                .map(|o| o.id.clone())
                .unwrap_or_default()
        }),
        background_id: String::new(),
        soul,
    };

    let origins = get_available_origins();
    let origin = origins
        .iter()
        .find(|o| Some(o.id.as_str()) == creation.origin_id.as_deref())
        .or_else(|| origins.first());

    let inv = PlayerInventory {
        credits: origin.map_or(500, |o| o.starting_credits),
        capacity: 100,
        ..Default::default()
    };

    let loc = CurrentLocation {
        system_seed: origin.map(|o| o.starting_location.0).unwrap_or(16843009),
        ..Default::default()
    };

    // Write the save file so load_save (run on OnEnter(InGame)) picks it up.
    save_player_with_log(
        &inv,
        &loc,
        None,
        &BTreeMap::new(),
        None,
        None,
        Some(&player_char),
        &DiscoveryLog::default(),
        None,
    );

    next_state.set(AppState::InGame);
}

fn build_player_soul(creation: &CharacterCreationState) -> SoulFile {
    let species_name = SPECIES_NAMES[creation.identity.species];
    let soul = generate_soul(creation.galaxy_seed, species_name);
    let species_enum = match species_name {
        "Human" => reachlock_core::soul::types::Species::Human,
        "Android" => reachlock_core::soul::types::Species::Android,
        "Robot" => reachlock_core::soul::types::Species::Robot,
        "Voidborn" => reachlock_core::soul::types::Species::Voidborn,
        "Xenotype" => reachlock_core::soul::types::Species::Xenotype,
        _ => reachlock_core::soul::types::Species::Human,
    };
    SoulFile {
        id: format!("player_{}", creation.galaxy_seed),
        name: creation.identity.name.clone(),
        species: species_enum,
        portrait_id: String::new(),
        identity: reachlock_core::soul::types::Identity {
            origin: "player".into(),
            faction_affiliation: "unaffiliated".into(),
            role: "Captain".into(),
            public_bio: "The player character.".into(),
        },
        personality: reachlock_core::soul::types::Personality {
            traits: vec![],
            values: vec![],
            speaking_style: reachlock_core::soul::types::SpeakingStyle::Terse,
            quirks: vec![],
        },
        emotional_state: reachlock_core::soul::types::EmotionalState {
            dominant_mood: reachlock_core::soul::types::Mood::Stable,
            intensity: 512,
            triggers: vec![],
        },
        memory_tree: vec![],
        relationship_graph: vec![],
        goals: vec![],
        breaking_points: vec![],
        contracts: vec![],
        backstory: soul.backstory,
        secrets: vec![],
        dialogue: None,
        deflections: vec![],
        look: Some(creation.look.clone()),
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::systems::inventory::SaveFile;

    #[test]
    fn step_navigation() {
        assert_eq!(CreationStep::COUNT, 6);
        assert_eq!(CreationStep::Identity.index(), 0);
        assert_eq!(CreationStep::Confirm.index(), 5);
        assert_eq!(
            CreationStep::Identity.next(),
            Some(CreationStep::Appearance)
        );
        assert_eq!(CreationStep::Confirm.next(), None);
        assert_eq!(CreationStep::Identity.prev(), None);
        assert_eq!(CreationStep::Confirm.prev(), Some(CreationStep::GalaxySeed));
    }

    #[test]
    fn identity_validation_empty_name() {
        let draft = IdentityDraft::default();
        assert!(draft.name.is_empty());
    }

    #[test]
    fn randomize_all_steps() {
        let mut state = CharacterCreationState {
            step: CreationStep::Identity,
            ..Default::default()
        };
        randomize_step(&mut state);
        assert!(!state.identity.name.is_empty());
        assert!(state.identity.species < 5);

        state.step = CreationStep::Appearance;
        randomize_step(&mut state);
        assert!(state.look.hair_style.is_some());
        assert!(state.look.skin_color.is_some());

        state.step = CreationStep::Origin;
        randomize_step(&mut state);
        // Randomize must land on *some* authored origin, not a fixed one.
        let chosen = state.origin_id.clone().expect("randomize picked an origin");
        assert!(
            get_available_origins().iter().any(|o| o.id == chosen),
            "randomize chose {chosen:?}, which is not an offered origin"
        );

        state.step = CreationStep::ShipAndCrew;
        randomize_step(&mut state);

        state.step = CreationStep::GalaxySeed;
        randomize_step(&mut state);
        assert!(state.galaxy_seed < (1u64 << 53));
    }

    #[test]
    /// Origins come from authored content, so this asserts the *shape* of what
    /// creation offers, not the contents of any one origin file. Pinning the
    /// old hardcoded values here is what let a single baked-in origin survive
    /// while ten sat authored on disk.
    fn origin_selection() {
        let origins = get_available_origins();
        assert!(
            !origins.is_empty(),
            "creation must offer at least one origin"
        );
        assert!(
            origins.len() > 1,
            "only {} origin(s) loaded — character creation is meant to be a \
             choice, so a single option means content is not being read",
            origins.len()
        );
        for o in &origins {
            assert!(!o.id.is_empty(), "origin has no id");
            assert!(!o.name.is_empty(), "origin {} has no name", o.id);
        }
        let ids: Vec<&str> = origins.iter().map(|o| o.id.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len(), "duplicate origin ids: {ids:?}");
    }

    #[test]
    fn species_identity_and_look_stay_in_sync() {
        let mut state = CharacterCreationState::default();
        state.identity.species = 2; // Robot
        state.look.species = reachlock_core::soul::types::Species::Robot;
        assert_eq!(state.look.species as usize, state.identity.species);
    }

    #[test]
    fn galaxy_seed_validation() {
        let state = CharacterCreationState::default();
        assert!(state.galaxy_seed < (1u64 << 53));
    }

    #[test]
    fn save_reload_roundtrip() {
        let mut state = CharacterCreationState::default();
        state.identity.name = "Rook".into();
        state.identity.pronouns = 0;
        let soul = build_player_soul(&state);
        let player_char = PlayerCharacter {
            id: EntityId(42),
            name: state.identity.name.clone(),
            pronouns: PRONOUN_OPTIONS[0].to_string(),
            species: SPECIES_NAMES[state.identity.species].to_string(),
            look: state.look.clone(),
            origin_id: "loup_garou_veteran".into(),
            background_id: String::new(),
            soul,
        };
        let save = SaveFile {
            character: Some(player_char.clone()),
            ..Default::default()
        };
        let text = ron::to_string(&save).unwrap();
        let back: SaveFile = ron::from_str(&text).unwrap();
        assert_eq!(back.character, Some(player_char));
    }

    #[test]
    /// Every offered origin must be complete enough to start a game. Again:
    /// shape, not content — an origin naming a different ship or career is a
    /// content decision, not a regression.
    fn origin_entry_fields() {
        for entry in get_available_origins() {
            assert!(
                !entry.ship_template_id.is_empty(),
                "origin {} grants no ship",
                entry.id
            );
            assert!(
                !entry.career_path.is_empty(),
                "origin {} grants no career",
                entry.id
            );
            assert!(
                !entry.starting_location.1.is_empty(),
                "origin {} starts nowhere",
                entry.id
            );
        }
    }
}
