//! Character creation flow (S78): 6-step UI after "New Game".
//! Identity → Appearance → Origin → Ship & Crew → Galaxy Seed → Confirm.
//!
//! The screen is a form, driven like a form: **↑/↓** moves between fields,
//! **←/→** changes the focused field's value, printable keys type into text
//! fields, **Enter** advances, **Esc** goes back, **Ctrl+R** randomizes the
//! current step.
//!
//! Randomize is on Ctrl+R rather than plain R for a reason: the captain name
//! is a text field, and a bare letter shortcut makes the name untypeable.

use std::collections::BTreeMap;

use bevy::ecs::message::MessageReader;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::ButtonState;
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
use crate::theme;

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
/// Index of the "custom" pronoun entry, which turns the field into a text box.
const PRONOUN_CUSTOM: usize = 5;

const STEP_LABELS: [&str; 6] = [
    "Identity",
    "Appearance",
    "Origin",
    "Ship & Crew",
    "Galaxy",
    "Confirm",
];

const MAX_NAME_CHARS: usize = 24;

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

impl IdentityDraft {
    /// The pronouns as they will be written to the character.
    pub fn pronoun_text(&self) -> String {
        if self.pronouns == PRONOUN_CUSTOM {
            self.custom_pronouns.clone()
        } else {
            PRONOUN_OPTIONS[self.pronouns].to_string()
        }
    }
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
    /// Which field within the current step has the cursor.
    pub focus: usize,
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
            focus: 0,
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

impl CharacterCreationState {
    /// How many focusable fields the current step has. Steps that are pure
    /// summaries have none.
    pub fn field_count(&self) -> usize {
        match self.step {
            CreationStep::Identity => 3,
            CreationStep::Appearance => 6,
            CreationStep::Origin => get_available_origins().len(),
            CreationStep::ShipAndCrew => 0,
            CreationStep::GalaxySeed => 1,
            CreationStep::Confirm => 0,
        }
    }

    /// Whether the focused field accepts typed characters.
    pub fn focused_field_is_text(&self) -> bool {
        match self.step {
            CreationStep::Identity => {
                self.focus == 0 || (self.focus == 1 && self.identity.pronouns == PRONOUN_CUSTOM)
            }
            _ => false,
        }
    }

    /// Move the cursor, wrapping at both ends.
    pub fn move_focus(&mut self, delta: i32) {
        let count = self.field_count();
        if count == 0 {
            self.focus = 0;
            return;
        }
        let count = count as i32;
        self.focus = (((self.focus as i32 + delta) % count + count) % count) as usize;
        self.sync_focus_selection();
    }

    /// On list steps the selection follows the cursor, so there is no way to
    /// leave the step with nothing chosen.
    fn sync_focus_selection(&mut self) {
        if self.step == CreationStep::Origin {
            let origins = get_available_origins();
            if let Some(origin) = origins.get(self.focus) {
                self.origin_id = Some(origin.id.clone());
            }
        }
    }

    /// Change the focused field's value. `delta` is -1 or +1.
    pub fn adjust(&mut self, delta: i32) {
        match self.step {
            CreationStep::Identity => match self.focus {
                1 => {
                    let n = PRONOUN_OPTIONS.len() as i32;
                    self.identity.pronouns =
                        (((self.identity.pronouns as i32 + delta) % n + n) % n) as usize;
                }
                2 => {
                    let n = SPECIES_NAMES.len() as i32;
                    self.identity.species =
                        (((self.identity.species as i32 + delta) % n + n) % n) as usize;
                    self.look.species = SPECIES[self.identity.species];
                }
                _ => {}
            },
            CreationStep::Appearance => self.adjust_appearance(delta),
            CreationStep::Origin => {}
            CreationStep::ShipAndCrew => {}
            CreationStep::GalaxySeed => {
                self.galaxy_seed = self
                    .galaxy_seed
                    .wrapping_add_signed(delta as i64)
                    .min(MAX_SEED);
            }
            CreationStep::Confirm => {}
        }
    }

    fn adjust_appearance(&mut self, delta: i32) {
        // Colors aren't an ordered set, so ←/→ rerolls them from a moving
        // seed rather than pretending to step through a sequence.
        self.sprite_seed = self.sprite_seed.wrapping_add_signed(delta as i64);
        let mut rng = SeededRng::new(self.sprite_seed);
        let mut color = || {
            [
                rng.next_below(256) as u8,
                rng.next_below(256) as u8,
                rng.next_below(256) as u8,
            ]
        };
        match self.focus {
            0 => {
                let n = HAIR_STYLE_COUNT as i32;
                let current = self.look.hair_style.unwrap_or(0) as i32;
                self.look.hair_style = Some((((current + delta) % n + n) % n) as u8);
            }
            1 => self.look.hair_color = Some(color()),
            2 => self.look.skin_color = Some(color()),
            3 => self.look.shirt_color = Some(color()),
            4 => self.look.pants_color = Some(color()),
            5 => {
                self.look.jacket_enabled = match self.look.jacket_enabled {
                    Some(true) => Some(false),
                    Some(false) => None,
                    None => Some(true),
                };
                if self.look.jacket_enabled == Some(true) && self.look.jacket_color.is_none() {
                    self.look.jacket_color = Some(color());
                }
            }
            _ => {}
        }
    }

    /// Whether the current step is complete enough to advance from.
    pub fn blocker(&self) -> Option<&'static str> {
        match self.step {
            CreationStep::Identity => {
                if self.identity.name.trim().is_empty() {
                    Some("Enter a captain name to continue.")
                } else if self.identity.pronouns == PRONOUN_CUSTOM
                    && self.identity.custom_pronouns.trim().is_empty()
                {
                    Some("Enter your custom pronouns, or pick a preset.")
                } else {
                    None
                }
            }
            CreationStep::Origin => {
                if self.origin_id.is_none() {
                    Some("Choose an origin to continue.")
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// The text buffer the focused field types into, if any.
    fn text_buffer(&mut self) -> Option<(&mut String, usize)> {
        match (self.step, self.focus) {
            (CreationStep::Identity, 0) => Some((&mut self.identity.name, MAX_NAME_CHARS)),
            (CreationStep::Identity, 1) if self.identity.pronouns == PRONOUN_CUSTOM => {
                Some((&mut self.identity.custom_pronouns, MAX_NAME_CHARS))
            }
            _ => None,
        }
    }
}

/// Seeds must survive a JSON round-trip (spec: ≤ 2^53).
const MAX_SEED: u64 = (1 << 53) - 1;

// ── Marker components ────────────────────────────────────────────────────

#[derive(Component)]
pub struct CreationUiRoot;

/// The region rebuilt whenever the step or its data changes.
#[derive(Component)]
pub struct CreationBody;

/// The header line showing which step this is.
#[derive(Component)]
pub struct StepHeading;

/// One dot in the step indicator, tagged with its step index.
#[derive(Component)]
pub struct StepDot(pub usize);

/// The blinking text-entry caret.
#[derive(Component)]
pub struct Caret(pub Timer);

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
    let dir = reachlock_core::paths::content_root().join("origins");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        warn!("origins: cannot read {}", dir.display());
        return out;
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

/// Build the persistent chrome. The body is filled by [`rebuild_body`], which
/// runs whenever the creation state changes.
pub fn spawn_creation_ui(
    mut commands: Commands,
    mut focus_stack: ResMut<FocusStack>,
    mut creation: ResMut<CharacterCreationState>,
) {
    focus_stack.push(crate::focus_stack::FocusLayer::Modal);
    // Entering the step fresh: put the cursor on the first field and let the
    // list steps adopt their selection.
    creation.focus = 0;
    creation.sync_focus_selection();

    commands
        .spawn((
            CreationUiRoot,
            theme::node_with(
                "screen",
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
            ),
        ))
        .with_children(|root| {
            root.spawn(theme::node_with(
                "frame",
                Node {
                    width: Val::Px(760.0),
                    max_height: Val::Percent(88.0),
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
            ))
            .with_children(|frame| {
                // Header: step dots on the left, step name on the right.
                frame
                    .spawn(theme::node_with(
                        "frame.header",
                        Node {
                            flex_direction: FlexDirection::Row,
                            justify_content: JustifyContent::SpaceBetween,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                    ))
                    .with_children(|header| {
                        header
                            .spawn(theme::node_with(
                                "row",
                                Node {
                                    column_gap: Val::Px(8.0),
                                    padding: UiRect::ZERO,
                                    ..default()
                                },
                            ))
                            .with_children(|dots| {
                                for i in 0..CreationStep::COUNT {
                                    dots.spawn((StepDot(i), theme::text("step.todo", "○")));
                                }
                            });
                        header.spawn((StepHeading, theme::text("heading", STEP_LABELS[0])));
                    });

                // Body: rebuilt on every state change.
                frame.spawn((
                    CreationBody,
                    theme::node_with(
                        "frame.body",
                        Node {
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Stretch,
                            flex_grow: 1.0,
                            overflow: Overflow::clip_y(),
                            ..default()
                        },
                    ),
                ));

                // Footer: the controls, stated plainly.
                frame
                    .spawn(theme::node_with(
                        "frame.footer",
                        Node {
                            flex_direction: FlexDirection::Row,
                            flex_wrap: FlexWrap::Wrap,
                            ..default()
                        },
                    ))
                    .with_children(|footer| {
                        for (key, desc) in [
                            ("↑↓", "field"),
                            ("←→", "change"),
                            ("Enter", "next"),
                            ("Esc", "back"),
                            ("Ctrl+R", "randomize"),
                        ] {
                            footer
                                .spawn(theme::node_with(
                                    "row",
                                    Node {
                                        column_gap: Val::Px(6.0),
                                        padding: UiRect::ZERO,
                                        ..default()
                                    },
                                ))
                                .with_children(|row| {
                                    row.spawn(theme::text("keycap", key));
                                    row.spawn(theme::text("keycap.desc", desc));
                                });
                        }
                    });
            });
        });
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

// ── Body rendering ──────────────────────────────────────────────────────

/// One line of the step body.
enum Line {
    /// A label/value pair, optionally with an affordance hint.
    Field {
        label: String,
        value: String,
        placeholder: bool,
        hint: &'static str,
    },
    /// A selectable list entry.
    Choice { label: String, detail: String },
    /// Free-standing prose.
    Note(String),
    /// A blank spacer.
    Gap,
}

/// Describe the current step as lines. Kept free of Bevy types so the shape
/// of every screen is unit-testable without spinning up an App.
fn step_lines(creation: &CharacterCreationState) -> Vec<Line> {
    let mut lines = Vec::new();
    match creation.step {
        CreationStep::Identity => {
            let name_empty = creation.identity.name.is_empty();
            lines.push(Line::Field {
                label: "Name".into(),
                value: if name_empty {
                    "type a name".into()
                } else {
                    creation.identity.name.clone()
                },
                placeholder: name_empty,
                hint: "",
            });
            let custom = creation.identity.pronouns == PRONOUN_CUSTOM;
            let pronouns = creation.identity.pronoun_text();
            let pronoun_empty = custom && pronouns.is_empty();
            lines.push(Line::Field {
                label: "Pronouns".into(),
                value: if pronoun_empty {
                    "type your pronouns".into()
                } else {
                    pronouns
                },
                placeholder: pronoun_empty,
                hint: "◂ ▸",
            });
            lines.push(Line::Field {
                label: "Species".into(),
                value: SPECIES_NAMES[creation.identity.species].into(),
                placeholder: false,
                hint: "◂ ▸",
            });
        }
        CreationStep::Appearance => {
            let hex = |c: Option<[u8; 3]>| match c {
                Some(c) => format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]),
                None => "auto".to_string(),
            };
            lines.push(Line::Field {
                label: "Hair style".into(),
                value: creation
                    .look
                    .hair_style
                    .map(|h| h.to_string())
                    .unwrap_or_else(|| "auto".into()),
                placeholder: false,
                hint: "◂ ▸",
            });
            for (label, color) in [
                ("Hair color", creation.look.hair_color),
                ("Skin", creation.look.skin_color),
                ("Shirt", creation.look.shirt_color),
                ("Pants", creation.look.pants_color),
            ] {
                lines.push(Line::Field {
                    label: label.into(),
                    value: hex(color),
                    placeholder: false,
                    hint: "◂ ▸",
                });
            }
            lines.push(Line::Field {
                label: "Jacket".into(),
                value: match creation.look.jacket_enabled {
                    Some(true) => hex(creation.look.jacket_color),
                    Some(false) => "none".into(),
                    None => "auto".into(),
                },
                placeholder: false,
                hint: "◂ ▸",
            });
        }
        CreationStep::Origin => {
            for origin in get_available_origins() {
                let mut detail = origin.description.clone();
                detail.push_str(&format!(
                    "\n{} · {} credits · crew of {}",
                    origin.career_path, origin.starting_credits, origin.crew_count
                ));
                if !origin.faction_deltas.is_empty() {
                    let standings: Vec<String> = origin
                        .faction_deltas
                        .iter()
                        .map(|(f, d)| format!("{f} {d:+}"))
                        .collect();
                    detail.push_str(&format!("\n{}", standings.join("  ")));
                }
                lines.push(Line::Choice {
                    label: origin.name.clone(),
                    detail,
                });
            }
        }
        CreationStep::ShipAndCrew => {
            let origins = get_available_origins();
            match origins
                .iter()
                .find(|o| Some(o.id.as_str()) == creation.origin_id.as_deref())
            {
                Some(o) => {
                    let ship = if o.ship_name.is_empty() {
                        o.ship_template_id.clone()
                    } else {
                        o.ship_name.clone()
                    };
                    lines.push(Line::Field {
                        label: "Ship".into(),
                        value: ship,
                        placeholder: false,
                        hint: "",
                    });
                    lines.push(Line::Field {
                        label: "Hull seed".into(),
                        value: format!("{:#x}", creation.ship_seed),
                        placeholder: false,
                        hint: "Ctrl+R",
                    });
                    lines.push(Line::Field {
                        label: "Starting at".into(),
                        value: o.starting_location.1.clone(),
                        placeholder: false,
                        hint: "",
                    });
                    lines.push(Line::Field {
                        label: "Crew".into(),
                        value: o.crew_count.to_string(),
                        placeholder: false,
                        hint: "",
                    });
                    lines.push(Line::Gap);
                    lines.push(Line::Note(
                        "Your crew is listed in the character sheet once you launch.".into(),
                    ));
                }
                None => lines.push(Line::Note("No origin selected.".into())),
            }
        }
        CreationStep::GalaxySeed => {
            lines.push(Line::Field {
                label: "Galaxy seed".into(),
                value: format!("{:#x}", creation.galaxy_seed),
                placeholder: false,
                hint: "◂ ▸  Ctrl+R",
            });
            lines.push(Line::Gap);
            lines.push(Line::Note(
                "The seed IS the galaxy. Share it and a friend explores the same stars.".into(),
            ));
        }
        CreationStep::Confirm => {
            let origins = get_available_origins();
            let origin = origins
                .iter()
                .find(|o| Some(o.id.as_str()) == creation.origin_id.as_deref());
            let field = |label: &str, value: String| Line::Field {
                label: label.into(),
                value,
                placeholder: false,
                hint: "",
            };
            lines.push(field("Captain", creation.identity.name.clone()));
            lines.push(field("Pronouns", creation.identity.pronoun_text()));
            lines.push(field(
                "Species",
                SPECIES_NAMES[creation.identity.species].into(),
            ));
            if let Some(o) = origin {
                lines.push(field("Origin", o.name.clone()));
                lines.push(field("Credits", o.starting_credits.to_string()));
                lines.push(field("Crew", o.crew_count.to_string()));
                lines.push(field("Starting at", o.starting_location.1.clone()));
            }
            lines.push(field("Galaxy seed", format!("{:#x}", creation.galaxy_seed)));
            lines.push(Line::Gap);
            lines.push(Line::Note("Enter launches. This is your character.".into()));
        }
    }
    lines
}

/// Repaint the step indicator and heading.
pub fn update_step_header(
    creation: Res<CharacterCreationState>,
    mut dots: Query<
        (
            &StepDot,
            &mut theme::Styled,
            &mut Text,
            &mut theme::SourceText,
        ),
        Without<StepHeading>,
    >,
    mut heading: Query<(&mut Text, &mut theme::SourceText), With<StepHeading>>,
) {
    if !creation.is_changed() {
        return;
    }
    let current = creation.step.index();
    for (dot, mut styled, mut text, mut source) in dots.iter_mut() {
        // Shape carries the state as well as color, so the indicator still
        // reads without relying on hue alone.
        let (class, glyph) = match dot.0.cmp(&current) {
            std::cmp::Ordering::Less => ("step.done", "●"),
            std::cmp::Ordering::Equal => ("step.current", "◉"),
            std::cmp::Ordering::Greater => ("step.todo", "○"),
        };
        if styled.0 != class {
            *styled = theme::Styled::new(class);
        }
        theme::set_text(&mut text, Some(&mut source), glyph);
    }
    for (mut text, mut source) in heading.iter_mut() {
        theme::set_text(&mut text, Some(&mut source), STEP_LABELS[current]);
    }
}

/// Rebuild the step body whenever the creation state changes.
///
/// Rebuilding wholesale rather than diffing keeps the render a pure function
/// of the state — there is no path where a stale row survives a step change.
pub fn rebuild_body(
    mut commands: Commands,
    creation: Res<CharacterCreationState>,
    body: Query<(Entity, Option<&Children>), With<CreationBody>>,
) {
    if !creation.is_changed() {
        return;
    }
    let Ok((body, children)) = body.single() else {
        return;
    };
    if let Some(children) = children {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }

    let lines = step_lines(&creation);
    let focus = creation.focus;
    let text_field = creation.focused_field_is_text();
    let blocker = creation.blocker();

    commands.entity(body).with_children(|body| {
        let mut field_index = 0usize;
        for line in &lines {
            match line {
                Line::Field {
                    label,
                    value,
                    placeholder,
                    hint,
                } => {
                    let focused = creation.field_count() > 0 && field_index == focus;
                    let show_caret = focused && text_field;
                    spawn_field_row(body, label, value, *placeholder, hint, focused, show_caret);
                    field_index += 1;
                }
                Line::Choice { label, detail } => {
                    let focused = field_index == focus;
                    spawn_choice(body, label, detail, focused);
                    field_index += 1;
                }
                Line::Note(text) => {
                    body.spawn(theme::text("muted", text.clone()));
                }
                Line::Gap => {
                    body.spawn(theme::node_with(
                        "row",
                        Node {
                            height: Val::Px(12.0),
                            padding: UiRect::ZERO,
                            ..default()
                        },
                    ));
                }
            }
        }
        if let Some(blocker) = blocker {
            body.spawn(theme::node_with(
                "row",
                Node {
                    height: Val::Px(12.0),
                    padding: UiRect::ZERO,
                    ..default()
                },
            ));
            body.spawn(theme::text("status.warn", format!("⚠ {blocker}")));
        }
    });
}

fn spawn_field_row(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    value: &str,
    placeholder: bool,
    hint: &str,
    focused: bool,
    show_caret: bool,
) {
    parent
        .spawn(theme::node_with(
            "row",
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .with_children(|row| {
            let label_class = if focused {
                "row.label.focused"
            } else {
                "row.label"
            };
            let marker = if focused { "▸ " } else { "  " };
            row.spawn(theme::text(label_class, format!("{marker}{label}")));
            let value_class = if placeholder {
                "row.value.placeholder"
            } else {
                "row.value"
            };
            row.spawn(theme::text(value_class, value));
            if show_caret {
                row.spawn((
                    Caret(Timer::from_seconds(0.53, TimerMode::Repeating)),
                    theme::text("caret", "▏"),
                ));
            }
            if !hint.is_empty() && focused {
                row.spawn(theme::text("row.affordance", hint));
            }
        });
}

fn spawn_choice(parent: &mut ChildSpawnerCommands, label: &str, detail: &str, focused: bool) {
    parent
        .spawn(theme::node_with(
            "row",
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::axes(Val::Px(0.0), Val::Px(6.0)),
                ..default()
            },
        ))
        .with_children(|row| {
            let marker = if focused { "▸ " } else { "  " };
            let class = if focused { "item.selected" } else { "item" };
            row.spawn(theme::text(class, format!("{marker}{label}")));
            for detail_line in detail.lines() {
                row.spawn(theme::text("muted", format!("    {detail_line}")));
            }
        });
}

/// Blink the text-entry caret so an empty field still reads as focused.
pub fn blink_caret(time: Res<Time>, mut carets: Query<(&mut Caret, &mut Text)>) {
    for (mut caret, mut text) in carets.iter_mut() {
        caret.0.tick(time.delta());
        if caret.0.just_finished() {
            text.0 = if text.0 == "▏" {
                " ".into()
            } else {
                "▏".into()
            };
        }
    }
}

// ── Input ────────────────────────────────────────────────────────────────

/// Keyboard handling for character creation.
#[allow(clippy::too_many_arguments)]
pub fn character_creation_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut typed: MessageReader<KeyboardInput>,
    settings: Res<Settings>,
    mut creation: ResMut<CharacterCreationState>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);

    // Esc: back a step, or out to the menu. The UI is torn down by
    // OnExit(CharacterCreation), never here.
    if keys.just_pressed(settings.key(InputAction::EditorCancel)) {
        typed.clear();
        match creation.step.prev() {
            Some(prev) => {
                creation.step = prev;
                creation.focus = 0;
                creation.sync_focus_selection();
            }
            None => next_state.set(AppState::MainMenu),
        }
        return;
    }

    // Enter: advance, or launch from the summary.
    if keys.just_pressed(settings.key(InputAction::EditorConfirm)) {
        typed.clear();
        if creation.blocker().is_some() {
            return;
        }
        match creation.step.next() {
            Some(next) => {
                creation.step = next;
                creation.focus = 0;
                creation.sync_focus_selection();
            }
            None => confirm(&creation, &mut next_state),
        }
        return;
    }

    if ctrl && keys.just_pressed(KeyCode::KeyR) {
        randomize_step(&mut creation);
        typed.clear();
        return;
    }

    if keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::Tab) {
        creation.move_focus(1);
    }
    if keys.just_pressed(KeyCode::ArrowUp) {
        creation.move_focus(-1);
    }
    if keys.just_pressed(KeyCode::ArrowRight) {
        creation.adjust(1);
    }
    if keys.just_pressed(KeyCode::ArrowLeft) {
        creation.adjust(-1);
    }

    // Typed characters go to the focused text field. Ctrl chords are
    // shortcuts, not text, so they never reach the buffer.
    if ctrl {
        typed.clear();
        return;
    }
    let Some((buffer, max)) = creation.text_buffer() else {
        typed.clear();
        return;
    };
    let mut edited = false;
    for input in typed.read() {
        if input.state != ButtonState::Pressed {
            continue;
        }
        match &input.logical_key {
            Key::Character(s) => {
                for c in s.chars().filter(|c| !c.is_control()) {
                    if buffer.chars().count() < max {
                        buffer.push(c);
                        edited = true;
                    }
                }
            }
            Key::Space => {
                if buffer.chars().count() < max {
                    buffer.push(' ');
                    edited = true;
                }
            }
            Key::Backspace => {
                buffer.pop();
                edited = true;
            }
            _ => {}
        }
    }
    if !edited {
        // Nothing changed: don't wake change detection and rebuild the body.
        creation.bypass_change_detection();
    }
}

// ── Randomization ────────────────────────────────────────────────────────

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
            if !origins.is_empty() {
                let mut rng = SeededRng::new(creation.galaxy_seed ^ 0xA5A5);
                let pick = rng.next_below(origins.len() as u64) as usize;
                creation.focus = pick;
                creation.origin_id = Some(origins[pick].id.clone());
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
            creation.galaxy_seed = rng.next_u64() & MAX_SEED;
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
    let pronouns = creation.identity.pronoun_text();
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
    let species_enum = SPECIES[creation.identity.species];
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
    /// The name field is typeable. It previously was not: the only way to set
    /// a name was the randomize shortcut, so the prompt "enter a captain name"
    /// described something the player could not do.
    fn name_is_a_real_text_field() {
        let mut state = CharacterCreationState {
            focus: 0,
            ..Default::default()
        };
        assert!(state.focused_field_is_text());
        let (buffer, max) = state.text_buffer().expect("name is a text buffer");
        assert_eq!(max, MAX_NAME_CHARS);
        buffer.push_str("Vex");
        assert_eq!(state.identity.name, "Vex");
    }

    #[test]
    /// Custom pronouns turn the pronoun field into a text box; presets do not.
    fn custom_pronouns_become_typeable() {
        let mut state = CharacterCreationState {
            focus: 1,
            ..Default::default()
        };
        assert!(
            !state.focused_field_is_text(),
            "presets are cycled, not typed"
        );
        state.identity.pronouns = PRONOUN_CUSTOM;
        assert!(state.focused_field_is_text());
        state
            .text_buffer()
            .expect("custom pronouns are a text buffer")
            .0
            .push_str("ey/em");
        assert_eq!(state.identity.pronoun_text(), "ey/em");
    }

    #[test]
    fn focus_wraps_in_both_directions() {
        let mut state = CharacterCreationState::default();
        assert_eq!(state.field_count(), 3);
        state.move_focus(-1);
        assert_eq!(state.focus, 2, "up from the first field wraps to the last");
        state.move_focus(1);
        assert_eq!(state.focus, 0, "down from the last wraps to the first");
    }

    #[test]
    /// Summary steps have nothing to focus, and must not divide by zero or
    /// strand the cursor on a field that isn't there.
    fn steps_without_fields_keep_focus_at_zero() {
        let mut state = CharacterCreationState {
            step: CreationStep::Confirm,
            ..Default::default()
        };
        assert_eq!(state.field_count(), 0);
        state.focus = 3;
        state.move_focus(1);
        assert_eq!(state.focus, 0);
    }

    #[test]
    fn arrows_cycle_species_and_pronouns() {
        let mut state = CharacterCreationState {
            focus: 2,
            ..Default::default()
        };
        state.adjust(1);
        assert_eq!(state.identity.species, 1);
        assert_eq!(
            state.look.species, SPECIES[1],
            "look must follow the identity species"
        );
        state.adjust(-1);
        assert_eq!(state.identity.species, 0);
        state.adjust(-1);
        assert_eq!(
            state.identity.species,
            SPECIES_NAMES.len() - 1,
            "cycling below zero wraps to the last species"
        );

        state.focus = 1;
        state.adjust(-1);
        assert_eq!(state.identity.pronouns, PRONOUN_OPTIONS.len() - 1);
    }

    #[test]
    /// A step must state what is blocking it rather than silently refusing to
    /// advance, which is what the old flow did.
    fn blockers_name_what_is_missing() {
        let mut state = CharacterCreationState::default();
        assert!(state.blocker().is_some(), "an empty name blocks Identity");
        state.identity.name = "  ".into();
        assert!(state.blocker().is_some(), "whitespace is not a name");
        state.identity.name = "Vex".into();
        assert!(state.blocker().is_none());

        state.identity.pronouns = PRONOUN_CUSTOM;
        assert!(
            state.blocker().is_some(),
            "custom pronouns left blank block the step"
        );
        state.identity.custom_pronouns = "ey/em".into();
        assert!(state.blocker().is_none());
    }

    #[test]
    /// Selection follows the cursor on list steps, so the player cannot land
    /// on the Origin step and be unable to work out how to choose.
    fn origin_selection_follows_the_cursor() {
        let mut state = CharacterCreationState {
            step: CreationStep::Origin,
            focus: 0,
            ..Default::default()
        };
        state.sync_focus_selection();
        let first = state.origin_id.clone().expect("cursor selects an origin");
        assert!(state.blocker().is_none(), "a selected origin unblocks");

        if state.field_count() > 1 {
            state.move_focus(1);
            assert_ne!(
                state.origin_id.as_deref(),
                Some(first.as_str()),
                "moving the cursor must move the selection"
            );
        }
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
        assert!(state.galaxy_seed <= MAX_SEED);
    }

    #[test]
    /// Nudging the seed by hand must respect the same 2^53 ceiling that
    /// randomize does, or a hand-tuned seed stops surviving JSON.
    fn seed_stays_within_the_json_safe_range() {
        let mut state = CharacterCreationState {
            step: CreationStep::GalaxySeed,
            ..Default::default()
        };
        state.galaxy_seed = MAX_SEED;
        state.adjust(1);
        assert!(state.galaxy_seed <= MAX_SEED);
    }

    #[test]
    /// Every step must render without panicking, for any reachable state.
    /// The old flow had steps that produced an empty screen.
    fn every_step_renders_lines() {
        let mut state = CharacterCreationState::default();
        state.identity.name = "Vex".into();
        for step in [
            CreationStep::Identity,
            CreationStep::Appearance,
            CreationStep::Origin,
            CreationStep::ShipAndCrew,
            CreationStep::GalaxySeed,
            CreationStep::Confirm,
        ] {
            state.step = step;
            state.sync_focus_selection();
            let lines = step_lines(&state);
            assert!(!lines.is_empty(), "{step:?} renders an empty body");
        }
    }

    #[test]
    /// The focusable field count must match the number of focusable lines the
    /// body actually draws, or the cursor can land on nothing.
    fn field_count_matches_rendered_fields() {
        let mut state = CharacterCreationState::default();
        for step in [
            CreationStep::Identity,
            CreationStep::Appearance,
            CreationStep::Origin,
            CreationStep::GalaxySeed,
        ] {
            state.step = step;
            state.sync_focus_selection();
            let focusable = step_lines(&state)
                .iter()
                .filter(|l| matches!(l, Line::Field { .. } | Line::Choice { .. }))
                .count();
            assert_eq!(
                focusable,
                state.field_count(),
                "{step:?} draws {focusable} focusable lines but reports {}",
                state.field_count()
            );
        }
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
        assert!(state.galaxy_seed <= MAX_SEED);
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
