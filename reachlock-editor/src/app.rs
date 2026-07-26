use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

/// Process-wide content root, set from the Preferences `content_root`
/// value at startup and whenever that preference changes. Every editor's
/// `new()` reads this so the directory it scans honors the configured
/// root — not just the browser tree (the previous behaviour left editors
/// hardcoding `mods/reachlock/` and silently ignoring the preference).
static CONTENT_ROOT: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Serialises the tests that move the process-wide content root.
///
/// [`CONTENT_ROOT`] is global, so two tests that repoint it race: one asserts
/// against a root the other has already replaced. `browser.rs` had a lock for
/// exactly this, but it was private to that one test — and `soul.rs` moves the
/// root too, without taking it. A lock only one participant holds is not a
/// lock, and the result was a workspace test run that failed roughly one time
/// in eight and passed on every rerun.
///
/// Every test that calls [`set_content_root`] must take this guard.
#[cfg(test)]
pub static ROOT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Set the content root (called from the app shell at startup and on
/// preference change). Pass `None` to fall back to the default.
pub fn set_content_root(root: Option<PathBuf>) {
    if let Ok(mut g) = CONTENT_ROOT.write() {
        *g = root;
    }
}

/// Resolve the configured content root, falling back to `mods/reachlock`.
pub fn content_root() -> PathBuf {
    if let Ok(g) = CONTENT_ROOT.read() {
        if let Some(r) = g.as_ref() {
            return r.clone();
        }
    }
    PathBuf::from("mods/reachlock")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContentType {
    HullFrame,
    Station,
    Location,
    Soul,
    Contract,
    Faction,
    EconomyGoods,
    Storyline,
    Item,
    EnemyArchetype,
    ChartedSystem,
    HullMesh,
    RoomTemplates,
    GateNetwork,
    /// Phase 2 previewer — nothing persisted; browses generated items live.
    ItemBrowser,
    /// Phase 2 previewer — character look explorer over the sprite generator.
    SpriteViewer,
    // Wave 9 — Living Galaxy content types (Phase B editors).
    Career,
    Ecosystem,
    PlanetCulture,
    Theme,
    Trope,
    ScriptedEncounter,
    // S55 — Last 4 editors.
    Dialogue,
    Dungeon,
    Event,
    Recipe,
    // S79 — Origin starting packages.
    Origin,
    // S120 — authored crew packages. The editor file existed since S120 but
    // had no `mod` line and no ContentType, so nothing could reach it.
    CrewPackage,
}

/// The `File > New` menu, as data. The menu bar renders this and
/// `new_menu_covers_every_type` asserts it covers every `ContentType`, so a
/// new content type cannot ship without an entry point the way `Origin` and
/// the ten Wave-9 types did.
pub const NEW_MENU_GROUPS: &[(&str, &[ContentType])] = &[
    (
        "Systems",
        &[ContentType::ChartedSystem, ContentType::GateNetwork],
    ),
    (
        "Ships",
        &[
            ContentType::HullFrame,
            ContentType::HullMesh,
            ContentType::Station,
            ContentType::RoomTemplates,
        ],
    ),
    (
        "Characters",
        &[
            ContentType::Soul,
            ContentType::EnemyArchetype,
            ContentType::Career,
            ContentType::Origin,
            ContentType::CrewPackage,
        ],
    ),
    (
        "World",
        &[
            ContentType::Faction,
            ContentType::Location,
            ContentType::Ecosystem,
            ContentType::PlanetCulture,
            ContentType::Dungeon,
        ],
    ),
    (
        "Narrative",
        &[
            ContentType::Storyline,
            ContentType::Dialogue,
            ContentType::Event,
            ContentType::Trope,
            ContentType::ScriptedEncounter,
            ContentType::Contract,
        ],
    ),
    (
        "Economy",
        &[
            ContentType::EconomyGoods,
            ContentType::Item,
            ContentType::Recipe,
        ],
    ),
    ("Audio", &[ContentType::Theme]),
    (
        "Preview",
        &[ContentType::ItemBrowser, ContentType::SpriteViewer],
    ),
];

impl ContentType {
    /// Previewers hold no file and are excluded from the content browser tree.
    pub fn is_previewer(&self) -> bool {
        matches!(self, ContentType::ItemBrowser | ContentType::SpriteViewer)
    }

    pub fn all() -> &'static [ContentType] {
        &[
            ContentType::HullFrame,
            ContentType::Station,
            ContentType::Location,
            ContentType::Soul,
            ContentType::Contract,
            ContentType::Faction,
            ContentType::EconomyGoods,
            ContentType::Storyline,
            ContentType::Item,
            ContentType::EnemyArchetype,
            ContentType::ChartedSystem,
            ContentType::HullMesh,
            ContentType::RoomTemplates,
            ContentType::GateNetwork,
            ContentType::ItemBrowser,
            ContentType::SpriteViewer,
            ContentType::Career,
            ContentType::Ecosystem,
            ContentType::PlanetCulture,
            ContentType::Theme,
            ContentType::Trope,
            ContentType::ScriptedEncounter,
            ContentType::Dialogue,
            ContentType::Dungeon,
            ContentType::Event,
            ContentType::Recipe,
            ContentType::Origin,
            ContentType::CrewPackage,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            ContentType::HullFrame => "Hull Frame",
            ContentType::Station => "Station",
            ContentType::Location => "Location",
            ContentType::Soul => "Soul",
            ContentType::Contract => "Contract",
            ContentType::Faction => "Faction",
            ContentType::EconomyGoods => "Economy Goods",
            ContentType::Storyline => "Storyline",
            ContentType::Item => "Item",
            ContentType::EnemyArchetype => "Enemy Archetype",
            ContentType::ChartedSystem => "Charted System",
            ContentType::HullMesh => "Hull Mesh",
            ContentType::RoomTemplates => "Room Templates",
            ContentType::GateNetwork => "Gate Network",
            ContentType::ItemBrowser => "Item Browser",
            ContentType::SpriteViewer => "Sprite Viewer",
            ContentType::Career => "Career Path",
            ContentType::Ecosystem => "Ecosystem",
            ContentType::PlanetCulture => "Planet Culture",
            ContentType::Theme => "Music Theme",
            ContentType::Trope => "Trope Template",
            ContentType::ScriptedEncounter => "Scripted Encounter",
            ContentType::Dialogue => "Dialogue Tree",
            ContentType::Dungeon => "Dungeon Layout",
            ContentType::Event => "Scripted Event",
            ContentType::Recipe => "Crafting Recipe",
            ContentType::Origin => "Origin",
            ContentType::CrewPackage => "Crew Package",
        }
    }

    pub fn directory(&self) -> &'static str {
        match self {
            ContentType::HullFrame => "hulls",
            ContentType::Station => "stations",
            ContentType::Location => "locations",
            ContentType::Soul => "souls",
            ContentType::Contract => "contracts",
            ContentType::Faction => "factions",
            ContentType::EconomyGoods => "economy",
            ContentType::Storyline => "storylines",
            ContentType::Item => "items",
            // The filesystem directory is `combat/` (see `mods/reachlock/`),
            // not `enemies/`. The exporter and content loader agree on
            // `combat/`, so this is the single source of truth.
            ContentType::EnemyArchetype => "combat",
            ContentType::ChartedSystem => "systems",
            ContentType::HullMesh => "hulls",
            ContentType::RoomTemplates => "hulls",
            ContentType::GateNetwork => "gate_network",
            ContentType::ItemBrowser => "items",
            ContentType::SpriteViewer => "souls",
            ContentType::Career => "careers",
            ContentType::Ecosystem => "ecosystems",
            ContentType::PlanetCulture => "cultures",
            ContentType::Theme => "themes",
            ContentType::Trope => "tropes",
            ContentType::ScriptedEncounter => "encounters",
            ContentType::Dialogue => "dialogues",
            ContentType::Dungeon => "dungeons",
            ContentType::Event => "events",
            ContentType::Recipe => "recipes",
            ContentType::Origin => "origins",
            ContentType::CrewPackage => "crews",
        }
    }

    /// Reverse of [`ContentType::directory`]. Maps a content directory name
    /// back to its `ContentType`. Returns `None` for unknown dirs
    /// (e.g. `schemas/`, `assets/`) and `mods/reachlock` itself.
    ///
    /// `hulls/` is shared by three types; use
    /// [`crate::browser::classify_hull_file`] to disambiguate those.
    pub fn from_directory(dir: &str) -> Option<ContentType> {
        match dir {
            "systems" => Some(ContentType::ChartedSystem),
            "gate_network" => Some(ContentType::GateNetwork),
            "hulls" => None, // shared — disambiguate via classify_hull_file
            "stations" => Some(ContentType::Station),
            "souls" => Some(ContentType::Soul),
            "combat" => Some(ContentType::EnemyArchetype),
            "factions" => Some(ContentType::Faction),
            "storylines" => Some(ContentType::Storyline),
            "locations" => Some(ContentType::Location),
            "economy" => Some(ContentType::EconomyGoods),
            "items" => Some(ContentType::Item),
            "contracts" => Some(ContentType::Contract),
            "careers" => Some(ContentType::Career),
            "ecosystems" => Some(ContentType::Ecosystem),
            "cultures" => Some(ContentType::PlanetCulture),
            "themes" => Some(ContentType::Theme),
            "tropes" => Some(ContentType::Trope),
            "encounters" => Some(ContentType::ScriptedEncounter),
            "dialogues" => Some(ContentType::Dialogue),
            "dungeons" => Some(ContentType::Dungeon),
            "events" => Some(ContentType::Event),
            "recipes" => Some(ContentType::Recipe),
            "origins" => Some(ContentType::Origin),
            _ => None,
        }
    }
}

pub trait Editor {
    fn title(&self) -> &str;
    fn content_type(&self) -> ContentType;
    fn has_unsaved_changes(&self) -> bool;

    /// Mark the editor as having unsaved changes and flag the currently
    /// selected entry dirty. Multi-entry editors override this so that
    /// [`Editor::save_all`] writes back only the entries that actually
    /// changed, preventing silent cross-entry data loss.
    fn touch(&mut self) {}
    fn load(&mut self, path: &std::path::Path) -> Result<(), String>;
    fn save(&self, path: &std::path::Path) -> Result<(), String>;

    /// Save every dirty entry to its own path. Multi-entry editors
    /// (soul, station, enemy, …) override this to write each
    /// loaded entry back to the file it was read from, instead of
    /// collapsing all entries onto the single tab path. The default
    /// impl delegates to [`Editor::save`], so single-entry editors
    /// keep their existing behaviour.
    ///
    /// Takes `&mut self` so that newly-created entries (with no path yet)
    /// can record the path they were written to, avoiding duplicate files
    /// on the next save.
    fn save_all(&mut self) -> Result<bool, String> {
        // Single-entry editors have no per-entry paths to fan out to.
        Ok(false)
    }

    fn validate(&self) -> Vec<String>;

    /// The content ids this document defines.
    ///
    /// Defaults to the tab title, which single-document editors set to their
    /// payload's id (`&self.theme.id`, `&self.origin.id`, …). Editors whose
    /// title is a fixed label rather than an id return nothing useful here and
    /// so contribute no cross-reference findings — silent is the right failure
    /// for a hint: a wrong id would report references that are actually fine.
    /// Multi-document editors override to list every loaded entry.
    fn document_ids(&self) -> Vec<String> {
        vec![self.title().to_string()]
    }

    /// Outgoing references from this document that point at an id nothing in
    /// the content tree defines, as `(field_path, message)`.
    ///
    /// Provided, not required: it reads [`Editor::document_ids`] against the
    /// index the shell built, so a tab gets reference checking without
    /// implementing anything.
    fn validate_cross_refs(
        &self,
        index: &crate::cross_ref::CrossReferenceIndex,
    ) -> Vec<(String, String)> {
        let ids = self.document_ids();
        index
            .broken_references()
            .into_iter()
            .filter(|(source_id, _, _)| ids.iter().any(|id| id == source_id))
            .map(|(_, target_id, field_path)| {
                (
                    field_path.clone(),
                    format!("{field_path} → `{target_id}` is not defined anywhere"),
                )
            })
            .collect()
    }

    /// Draw the editor into the region the shell hands it.
    ///
    /// Takes a `Ui`, not a `Context`: when this took `&egui::Context` every
    /// editor opened its own `CentralPanel`, while the shell had already
    /// opened one to call it — two panels with the same id every frame. Panels
    /// nest via `show_inside(ui, …)`, so the shell owns layout and an editor
    /// cannot fight it.
    fn ui(&mut self, ui: &mut egui::Ui);
    fn generate_from_seed(&mut self, seed: u64);

    /// Populate editor fields from AI-generated JSON (handoff §Phase 2.5).
    /// Editors that store their data as a plain serde core struct implement
    /// this; editors wrapping data in a `ContentFile` envelope fall back to
    /// this default and report that direct AI population isn't wired yet.
    fn apply_ai_json(&mut self, _value: &serde_json::Value) -> Result<(), String> {
        Err(
            "This editor stores data in a content envelope — AI population isn't wired yet. \
             Use procedural generation instead."
                .into(),
        )
    }

    /// Serialize the editor's full document state (entries, selection) to a
    /// RON string for snapshot-based undo. `None` means this editor doesn't
    /// support undo (previewers).
    fn snapshot(&self) -> Option<String> {
        None
    }

    /// Restore state previously captured by [`Editor::snapshot`].
    fn restore_snapshot(&mut self, _ron: &str) -> Result<(), String> {
        Err("undo is not supported by this editor".into())
    }

    /// The app shell calls this after a successful save so the dirty flag
    /// (and the tab asterisk) clears.
    fn mark_saved(&mut self) {}

    /// Files this editor tried to load at construction and could not parse.
    ///
    /// Empty for editors that do not scan a directory. Drained into the
    /// status bar and the warning panel when the tab opens, so an
    /// unparseable file is visible instead of silently absent.
    fn load_warnings(&self) -> &[String] {
        &[]
    }

    /// Whether the seed panel's "Reroll All" should reseed this editor.
    /// Editors whose content is purely authored (gate networks) or replaced
    /// by a reference set (room templates) opt out.
    fn accept_seed_reroll(&self) -> bool {
        true
    }

    /// Seed panel entry point — defaults to procedural generation.
    fn apply_seed(&mut self, seed: u64) {
        self.generate_from_seed(seed);
    }

    /// Name of the currently selected entry, shown in the Delete shortcut's
    /// confirmation dialog. `None` disables the shortcut for this editor.
    fn selected_entry_name(&self) -> Option<String> {
        None
    }

    /// Remove the selected entry. Returns false when nothing was removed.
    fn delete_selected(&mut self) -> bool {
        false
    }

    /// Compact summary card rendered in the right-hand preview panel.
    fn preview_ui(&self, ui: &mut egui::Ui) {
        ui.label(self.content_type().name());
        ui.weak("No preview for this editor.");
    }
}

pub struct EditorRegistry(HashMap<ContentType, fn() -> Box<dyn Editor>>);

impl EditorRegistry {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn register(&mut self, ct: ContentType, factory: fn() -> Box<dyn Editor>) {
        self.0.insert(ct, factory);
    }

    pub fn create(&self, ct: ContentType) -> Option<Box<dyn Editor>> {
        self.0.get(&ct).map(|f| f())
    }
}

pub fn build_default_registry() -> EditorRegistry {
    let mut r = EditorRegistry::new();
    r.register(ContentType::Career, crate::editors::career::create_editor);
    r.register(
        ContentType::Ecosystem,
        crate::editors::ecosystem::create_editor,
    );
    r.register(
        ContentType::PlanetCulture,
        crate::editors::planet_culture::create_editor,
    );
    r.register(ContentType::Theme, crate::editors::theme::create_editor);
    r.register(ContentType::Trope, crate::editors::trope::create_editor);
    r.register(
        ContentType::ScriptedEncounter,
        crate::editors::scripted_encounter::create_editor,
    );
    r.register(
        ContentType::HullFrame,
        crate::editors::hull_frame::create_editor,
    );
    r.register(ContentType::Station, crate::editors::station::create_editor);
    r.register(
        ContentType::Location,
        crate::editors::location::create_editor,
    );
    r.register(ContentType::Soul, crate::editors::soul::create_editor);
    r.register(
        ContentType::Contract,
        crate::editors::contract::create_editor,
    );
    r.register(ContentType::Faction, crate::editors::faction::create_editor);
    r.register(
        ContentType::EconomyGoods,
        crate::editors::economy::create_editor,
    );
    r.register(
        ContentType::Storyline,
        crate::editors::storyline::create_editor,
    );
    r.register(ContentType::Item, crate::editors::item::create_editor);
    r.register(
        ContentType::EnemyArchetype,
        crate::editors::enemy::create_editor,
    );
    r.register(
        ContentType::ChartedSystem,
        crate::editors::charted_system::create_editor,
    );
    r.register(
        ContentType::HullMesh,
        crate::editors::hull_mesh::create_editor,
    );
    r.register(
        ContentType::RoomTemplates,
        crate::editors::room_templates::create_editor,
    );
    r.register(
        ContentType::GateNetwork,
        crate::editors::gate_network::create_editor,
    );
    r.register(
        ContentType::ItemBrowser,
        crate::editors::item_browser::create_editor,
    );
    r.register(
        ContentType::SpriteViewer,
        crate::editors::character_sprite::create_editor,
    );
    r.register(
        ContentType::Dialogue,
        crate::editors::dialogue::create_editor,
    );
    r.register(ContentType::Dungeon, crate::editors::dungeon::create_editor);
    r.register(ContentType::Event, crate::editors::event::create_editor);
    r.register(ContentType::Recipe, crate::editors::recipe::create_editor);
    r.register(ContentType::Origin, crate::editors::origin::create_editor);
    r.register(
        ContentType::CrewPackage,
        crate::editors::crew_package::create_editor,
    );
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `directory()` must be the inverse of `from_directory()` for every
    /// content type that owns a unique directory. The `hulls/` directory is
    /// shared by three types (disambiguated via `classify_hull_file`), and the
    /// two previewers persist nothing, so they are excluded here.
    #[test]
    fn directory_round_trips_for_unique_dirs() {
        let unique = [
            ContentType::Station,
            ContentType::Location,
            ContentType::Soul,
            ContentType::Contract,
            ContentType::Faction,
            ContentType::EconomyGoods,
            ContentType::Storyline,
            ContentType::Item,
            ContentType::EnemyArchetype,
            ContentType::ChartedSystem,
            ContentType::GateNetwork,
        ];
        for ct in unique {
            let dir = ct.directory();
            assert_eq!(
                ContentType::from_directory(dir),
                Some(ct),
                "{ct:?} directory {dir} did not round-trip"
            );
        }
    }

    /// Enemy archetypes live under `combat/`, not `enemies/` — the exporter and
    /// content loader both read from `combat/`. Lock that mapping.
    #[test]
    fn enemy_archetype_maps_to_combat_dir() {
        assert_eq!(ContentType::EnemyArchetype.directory(), "combat");
        assert_eq!(
            ContentType::from_directory("combat"),
            Some(ContentType::EnemyArchetype)
        );
    }

    /// Unknown directories resolve to `None` (e.g. `schemas/`, `assets/`).
    #[test]
    fn unknown_directory_resolves_to_none() {
        assert_eq!(ContentType::from_directory("schemas"), None);
        assert_eq!(ContentType::from_directory("assets"), None);
        assert_eq!(ContentType::from_directory("hulls"), None);
    }
}
