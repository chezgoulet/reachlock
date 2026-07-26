use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use reachlock_core::career::CareerPath;
use reachlock_core::content::dialogue::Dialogue;
use reachlock_core::content::dungeon::Dungeon;
use reachlock_core::content::envelope::{AssetType, ContentFile, ContentPayload};
use reachlock_core::content::event::Event;
use reachlock_core::content::origin::Origin;
use reachlock_core::content::recipe::Recipe;
use reachlock_core::contract::types::Contract;
use reachlock_core::generator::culture::PlanetCulture;
use reachlock_core::generator::ecosystem::Ecosystem;
use reachlock_core::generator::music::Theme as MusicTheme;
use reachlock_core::generator::scripted_encounter::ScriptedEncounter;
use reachlock_core::generator::trope::TropeTemplate;

use crate::systems::content_index::ContentIndex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContentPayloadVariant {
    Hull,
    Station,
    Contract,
    Ecosystem,
    Career,
    PlanetCulture,
    Theme,
    Trope,
    ScriptedEncounter,
    Dialogue,
    Dungeon,
    Event,
    Recipe,
    Soul,
    HullFrame,
    RoomTemplates,
    Origin,
    CrewPackage,
    SoulMutations,
    Storyline,
}

impl ContentPayloadVariant {
    pub fn from_asset_type(at: AssetType) -> Option<ContentPayloadVariant> {
        match at {
            AssetType::Hull => Some(ContentPayloadVariant::Hull),
            AssetType::Station => Some(ContentPayloadVariant::Station),
            AssetType::Contract => Some(ContentPayloadVariant::Contract),
            AssetType::Ecosystem => Some(ContentPayloadVariant::Ecosystem),
            AssetType::Career => Some(ContentPayloadVariant::Career),
            AssetType::PlanetCulture => Some(ContentPayloadVariant::PlanetCulture),
            AssetType::Theme => Some(ContentPayloadVariant::Theme),
            AssetType::Trope => Some(ContentPayloadVariant::Trope),
            AssetType::ScriptedEncounter => Some(ContentPayloadVariant::ScriptedEncounter),
            AssetType::Dialogue => Some(ContentPayloadVariant::Dialogue),
            AssetType::Dungeon => Some(ContentPayloadVariant::Dungeon),
            AssetType::Event => Some(ContentPayloadVariant::Event),
            AssetType::Recipe => Some(ContentPayloadVariant::Recipe),
            AssetType::Soul => Some(ContentPayloadVariant::Soul),
            AssetType::HullFrame => Some(ContentPayloadVariant::HullFrame),
            AssetType::RoomTemplates => Some(ContentPayloadVariant::RoomTemplates),
            AssetType::Origin => Some(ContentPayloadVariant::Origin),
            AssetType::CrewPackage => Some(ContentPayloadVariant::CrewPackage),
            AssetType::SoulMutations => Some(ContentPayloadVariant::SoulMutations),
            AssetType::Storyline => Some(ContentPayloadVariant::Storyline),
        }
    }

    #[cfg_attr(not(test), expect(dead_code))]
    pub fn all() -> &'static [ContentPayloadVariant] {
        &[
            ContentPayloadVariant::Hull,
            ContentPayloadVariant::Station,
            ContentPayloadVariant::Contract,
            ContentPayloadVariant::Ecosystem,
            ContentPayloadVariant::Career,
            ContentPayloadVariant::PlanetCulture,
            ContentPayloadVariant::Theme,
            ContentPayloadVariant::Trope,
            ContentPayloadVariant::ScriptedEncounter,
            ContentPayloadVariant::Dialogue,
            ContentPayloadVariant::Dungeon,
            ContentPayloadVariant::Event,
            ContentPayloadVariant::Recipe,
            ContentPayloadVariant::Soul,
            ContentPayloadVariant::HullFrame,
            ContentPayloadVariant::RoomTemplates,
            ContentPayloadVariant::Origin,
            ContentPayloadVariant::CrewPackage,
            ContentPayloadVariant::SoulMutations,
            ContentPayloadVariant::Storyline,
        ]
    }
}

/// A consumer receives a slice of content files for its variant and integrates
/// them into the game world. Returns `(file_id, error_message)` for each file
/// that failed — non-fatal, one bad file doesn't block the rest.
pub type ContentConsumer = fn(&[ContentFile]) -> Vec<(String, String)>;

/// Registry of consumers, one per `ContentPayloadVariant`. Every variant must
/// have exactly one registered consumer; the coverage test enforces this.
pub struct ContentDispatcher {
    consumers: HashMap<ContentPayloadVariant, ContentConsumer>,
}

impl ContentDispatcher {
    pub fn new() -> Self {
        ContentDispatcher {
            consumers: HashMap::new(),
        }
    }

    pub fn register(&mut self, payload: ContentPayloadVariant, consumer: ContentConsumer) {
        self.consumers.insert(payload, consumer);
    }

    /// Route every file in `index.files` to its registered consumer. Returns
    /// aggregated errors from every consumer.
    pub fn dispatch_all(&self, index: &ContentIndex) -> Vec<(String, String)> {
        let mut errors = Vec::new();
        // Group files by asset type.
        let mut by_variant: HashMap<ContentPayloadVariant, Vec<&ContentFile>> = HashMap::new();
        for file in &index.files {
            if let Some(variant) = ContentPayloadVariant::from_asset_type(file.asset_type) {
                by_variant.entry(variant).or_default().push(file);
            }
        }
        // Dispatch each group.
        for (variant, files) in &by_variant {
            if let Some(consumer) = self.consumers.get(variant) {
                let owned: Vec<ContentFile> = files.iter().map(|f| (*f).clone()).collect();
                errors.extend(consumer(&owned));
            }
        }
        errors
    }

    #[cfg_attr(not(test), expect(dead_code))]
    pub fn registered_variants(&self) -> HashSet<ContentPayloadVariant> {
        self.consumers.keys().copied().collect()
    }

    /// Build the default dispatcher with all registered consumers. Called once
    /// at startup.
    pub fn build_default() -> Self {
        let mut d = ContentDispatcher::new();
        register_all_consumers(&mut d);
        d
    }
}

// ---------------------------------------------------------------------------
// Default consumer registry
// ---------------------------------------------------------------------------

fn register_all_consumers(d: &mut ContentDispatcher) {
    use ContentPayloadVariant::*;

    d.register(Soul, consume_souls);
    d.register(Hull, consume_hulls);
    d.register(Station, consume_stations);
    d.register(HullFrame, consume_hull_frames);
    d.register(RoomTemplates, consume_room_templates);
    d.register(Contract, consume_contracts);
    d.register(Career, consume_careers);
    d.register(Theme, consume_themes);
    d.register(Trope, consume_tropes);
    d.register(ScriptedEncounter, consume_scripted_encounters);
    d.register(Dialogue, consume_dialogues);
    d.register(Dungeon, consume_dungeons);
    d.register(Ecosystem, consume_ecosystems);
    d.register(Event, consume_events);
    d.register(PlanetCulture, consume_planet_cultures);
    d.register(Recipe, consume_recipes);
    d.register(Origin, consume_origins);
    d.register(CrewPackage, consume_crew_packages);
    d.register(SoulMutations, consume_soul_mutations);
    d.register(Storyline, consume_storylines);
}

// ---------------------------------------------------------------------------
// Existing consumers (unchanged behaviour, just routed through dispatcher)
// ---------------------------------------------------------------------------

fn consume_souls(files: &[ContentFile]) -> Vec<(String, String)> {
    let mut errors = Vec::new();
    for file in files {
        if let ContentPayload::Soul(_) = &file.payload {
            // Souls are loaded by soul::init_souls which reads index.files
            // directly. The dispatcher hands them off; the actual loading
            // happens in soul.rs via the existing path.
        } else {
            errors.push((file.id.clone(), "Soul payload expected".into()));
        }
    }
    errors
}

fn consume_hulls(_files: &[ContentFile]) -> Vec<(String, String)> {
    // Hulls are resolved on demand via resolve() in setup.rs — no load-time
    // action needed.
    Vec::new()
}

fn consume_stations(_files: &[ContentFile]) -> Vec<(String, String)> {
    // Stations are resolved on demand by seed lookup.
    Vec::new()
}

fn consume_hull_frames(_files: &[ContentFile]) -> Vec<(String, String)> {
    // Hull frames are loaded by frame_for() in shipeditor on demand.
    Vec::new()
}

fn consume_room_templates(_files: &[ContentFile]) -> Vec<(String, String)> {
    // Room templates are loaded by the interior editor on demand.
    Vec::new()
}

// ---------------------------------------------------------------------------
// Contract consumer
// ---------------------------------------------------------------------------

fn consume_contracts(files: &[ContentFile]) -> Vec<(String, String)> {
    let mut errors = Vec::new();
    let contracts: Vec<Contract> = files
        .iter()
        .filter_map(|f| match &f.payload {
            ContentPayload::Contract(c) => Some(c.clone()),
            _ => {
                errors.push((f.id.clone(), "Contract payload expected".into()));
                None
            }
        })
        .collect();
    // Contracts are installed into ContractRuntime via the dispatcher
    // after the index is loaded. The runtime is a Resource, which we
    // can't access from a pure fn consumer. Instead, the contracts are
    // stored in a temporary ContractCatalog resource that the runtime
    // reads during its own initialization.
    if !contracts.is_empty() {
        // We use a OnceLock to stash contracts for ContractRuntime to pick up,
        // since ContentConsumer is a plain fn and can't access Bevy commands.
        crate::systems::contract::push_authored_contracts(contracts);
    }
    errors
}

// ---------------------------------------------------------------------------
// Stub consumers (registries created for future narrative/living-world sprints)
// ---------------------------------------------------------------------------

fn consume_careers(files: &[ContentFile]) -> Vec<(String, String)> {
    let mut careers: Vec<CareerPath> = Vec::new();
    for file in files {
        if let ContentPayload::Career(c) = &file.payload {
            careers.push((**c).clone());
        }
    }
    if !careers.is_empty() {
        crate::systems::dispatch::stash::set_careers(careers);
    }
    Vec::new()
}

fn consume_themes(files: &[ContentFile]) -> Vec<(String, String)> {
    let mut themes: Vec<MusicTheme> = Vec::new();
    for file in files {
        if let ContentPayload::Theme(t) = &file.payload {
            themes.push((**t).clone());
        }
    }
    if !themes.is_empty() {
        crate::systems::dispatch::stash::set_themes(themes);
    }
    Vec::new()
}

fn consume_tropes(files: &[ContentFile]) -> Vec<(String, String)> {
    let mut tropes: Vec<TropeTemplate> = Vec::new();
    for file in files {
        if let ContentPayload::Trope(t) = &file.payload {
            tropes.push((**t).clone());
        }
    }
    if !tropes.is_empty() {
        crate::systems::dispatch::stash::set_tropes(tropes);
    }
    Vec::new()
}

fn consume_scripted_encounters(files: &[ContentFile]) -> Vec<(String, String)> {
    let mut encounters: Vec<ScriptedEncounter> = Vec::new();
    for file in files {
        if let ContentPayload::ScriptedEncounter(e) = &file.payload {
            encounters.push((**e).clone());
        }
    }
    if !encounters.is_empty() {
        crate::systems::dispatch::stash::set_encounters(encounters);
    }
    Vec::new()
}

fn consume_dialogues(files: &[ContentFile]) -> Vec<(String, String)> {
    let mut dialogues: Vec<Dialogue> = Vec::new();
    for file in files {
        if let ContentPayload::Dialogue(d) = &file.payload {
            dialogues.push((**d).clone());
        }
    }
    if !dialogues.is_empty() {
        crate::systems::dispatch::stash::set_dialogues(dialogues);
    }
    Vec::new()
}

fn consume_dungeons(files: &[ContentFile]) -> Vec<(String, String)> {
    let mut dungeons: Vec<Dungeon> = Vec::new();
    for file in files {
        if let ContentPayload::Dungeon(d) = &file.payload {
            dungeons.push((**d).clone());
        }
    }
    if !dungeons.is_empty() {
        crate::systems::dispatch::stash::set_dungeons(dungeons);
    }
    Vec::new()
}

fn consume_ecosystems(files: &[ContentFile]) -> Vec<(String, String)> {
    let mut ecosystems: Vec<(String, Ecosystem)> = Vec::new();
    for file in files {
        if let ContentPayload::Ecosystem(e) = &file.payload {
            ecosystems.push((file.id.clone(), (**e).clone()));
        }
    }
    if !ecosystems.is_empty() {
        crate::systems::dispatch::stash::set_ecosystems(ecosystems);
    }
    Vec::new()
}

fn consume_events(files: &[ContentFile]) -> Vec<(String, String)> {
    let mut events: Vec<Event> = Vec::new();
    for file in files {
        if let ContentPayload::Event(e) = &file.payload {
            events.push((**e).clone());
        }
    }
    if !events.is_empty() {
        crate::systems::dispatch::stash::set_events(events);
    }
    Vec::new()
}

fn consume_planet_cultures(files: &[ContentFile]) -> Vec<(String, String)> {
    let mut cultures: Vec<(String, PlanetCulture)> = Vec::new();
    for file in files {
        if let ContentPayload::PlanetCulture(c) = &file.payload {
            cultures.push((file.id.clone(), (**c).clone()));
        }
    }
    if !cultures.is_empty() {
        crate::systems::dispatch::stash::set_cultures(cultures);
    }
    Vec::new()
}

fn consume_recipes(files: &[ContentFile]) -> Vec<(String, String)> {
    let mut recipes: Vec<Recipe> = Vec::new();
    for file in files {
        if let ContentPayload::Recipe(r) = &file.payload {
            recipes.push((**r).clone());
        }
    }
    if !recipes.is_empty() {
        crate::systems::dispatch::stash::set_recipes(recipes);
    }
    Vec::new()
}

fn consume_crew_packages(_files: &[ContentFile]) -> Vec<(String, String)> {
    // Crew packages are loaded by crew::CrewRoster::load_from_content
    // which reads from the content index directly.
    Vec::new()
}

fn consume_soul_mutations(_files: &[ContentFile]) -> Vec<(String, String)> {
    // Soul mutations are loaded by soul::init_souls which reads
    // from the content index directly.
    Vec::new()
}

fn consume_storylines(_files: &[ContentFile]) -> Vec<(String, String)> {
    // Storylines are loaded through the faction loader.
    Vec::new()
}

// ---------------------------------------------------------------------------
// Stash — temporary holding for data that consumers produce, picked up by
// Bevy systems in startup order (after load_content_index, before load_save).
// The stash avoids needing ResMut access from the pure fn consumers.
// ---------------------------------------------------------------------------

pub mod stash {
    use std::sync::Mutex;

    use reachlock_core::career::CareerPath;
    use reachlock_core::content::dialogue::Dialogue;
    use reachlock_core::content::dungeon::Dungeon;
    use reachlock_core::content::event::Event;
    use reachlock_core::content::origin::Origin;
    use reachlock_core::content::recipe::Recipe;
    use reachlock_core::contract::types::Contract;
    use reachlock_core::generator::culture::PlanetCulture;
    use reachlock_core::generator::ecosystem::Ecosystem;
    use reachlock_core::generator::music::Theme;
    use reachlock_core::generator::scripted_encounter::ScriptedEncounter;
    use reachlock_core::generator::trope::TropeTemplate;

    static STASH: Mutex<StashInner> = Mutex::new(StashInner {
        contracts: Vec::new(),
        careers: Vec::new(),
        themes: Vec::new(),
        tropes: Vec::new(),
        encounters: Vec::new(),
        dialogues: Vec::new(),
        dungeons: Vec::new(),
        ecosystems: Vec::new(),
        events: Vec::new(),
        cultures: Vec::new(),
        recipes: Vec::new(),
        origins: Vec::new(),
    });

    struct StashInner {
        contracts: Vec<Contract>,
        careers: Vec<CareerPath>,
        themes: Vec<Theme>,
        tropes: Vec<TropeTemplate>,
        encounters: Vec<ScriptedEncounter>,
        dialogues: Vec<Dialogue>,
        dungeons: Vec<Dungeon>,
        ecosystems: Vec<(String, Ecosystem)>,
        events: Vec<Event>,
        cultures: Vec<(String, PlanetCulture)>,
        recipes: Vec<Recipe>,
        origins: Vec<Origin>,
    }

    pub fn set_contracts(v: Vec<Contract>) {
        STASH.lock().unwrap().contracts = v;
    }
    pub fn take_contracts() -> Vec<Contract> {
        std::mem::take(&mut STASH.lock().unwrap().contracts)
    }

    pub fn set_careers(v: Vec<CareerPath>) {
        STASH.lock().unwrap().careers = v;
    }
    #[expect(dead_code)]
    pub fn take_careers() -> Vec<CareerPath> {
        std::mem::take(&mut STASH.lock().unwrap().careers)
    }

    pub fn set_themes(v: Vec<Theme>) {
        STASH.lock().unwrap().themes = v;
    }
    pub fn take_themes() -> Vec<Theme> {
        std::mem::take(&mut STASH.lock().unwrap().themes)
    }

    pub fn set_tropes(v: Vec<TropeTemplate>) {
        STASH.lock().unwrap().tropes = v;
    }
    pub fn take_tropes() -> Vec<TropeTemplate> {
        std::mem::take(&mut STASH.lock().unwrap().tropes)
    }

    pub fn set_encounters(v: Vec<ScriptedEncounter>) {
        STASH.lock().unwrap().encounters = v;
    }
    pub fn take_encounters() -> Vec<ScriptedEncounter> {
        std::mem::take(&mut STASH.lock().unwrap().encounters)
    }

    pub fn set_dialogues(v: Vec<Dialogue>) {
        STASH.lock().unwrap().dialogues = v;
    }
    #[expect(dead_code)]
    pub fn take_dialogues() -> Vec<Dialogue> {
        std::mem::take(&mut STASH.lock().unwrap().dialogues)
    }

    pub fn set_dungeons(v: Vec<Dungeon>) {
        STASH.lock().unwrap().dungeons = v;
    }
    #[expect(dead_code)]
    pub fn take_dungeons() -> Vec<Dungeon> {
        std::mem::take(&mut STASH.lock().unwrap().dungeons)
    }

    pub fn set_ecosystems(v: Vec<(String, Ecosystem)>) {
        STASH.lock().unwrap().ecosystems = v;
    }
    pub fn take_ecosystems() -> Vec<(String, Ecosystem)> {
        std::mem::take(&mut STASH.lock().unwrap().ecosystems)
    }

    pub fn set_events(v: Vec<Event>) {
        STASH.lock().unwrap().events = v;
    }
    #[expect(dead_code)]
    pub fn take_events() -> Vec<Event> {
        std::mem::take(&mut STASH.lock().unwrap().events)
    }

    pub fn set_cultures(v: Vec<(String, PlanetCulture)>) {
        STASH.lock().unwrap().cultures = v;
    }
    pub fn take_cultures() -> Vec<(String, PlanetCulture)> {
        std::mem::take(&mut STASH.lock().unwrap().cultures)
    }

    pub fn set_recipes(v: Vec<Recipe>) {
        STASH.lock().unwrap().recipes = v;
    }
    #[expect(dead_code)]
    pub fn take_recipes() -> Vec<Recipe> {
        std::mem::take(&mut STASH.lock().unwrap().recipes)
    }

    pub fn set_origins(v: Vec<Origin>) {
        STASH.lock().unwrap().origins = v;
    }
    pub fn take_origins() -> Vec<Origin> {
        std::mem::take(&mut STASH.lock().unwrap().origins)
    }

    #[allow(dead_code)]
    pub fn clear_all() {
        let mut s = STASH.lock().unwrap();
        s.contracts.clear();
        s.careers.clear();
        s.themes.clear();
        s.tropes.clear();
        s.encounters.clear();
        s.dialogues.clear();
        s.dungeons.clear();
        s.ecosystems.clear();
        s.events.clear();
        s.cultures.clear();
        s.recipes.clear();
        s.origins.clear();
    }
}

fn consume_origins(files: &[ContentFile]) -> Vec<(String, String)> {
    let mut origins: Vec<Origin> = Vec::new();
    for file in files {
        if let ContentPayload::Origin(o) = &file.payload {
            origins.push(o.clone());
        }
    }
    if !origins.is_empty() {
        crate::systems::dispatch::stash::set_origins(origins);
    }
    Vec::new()
}

/// S79: the registry of all authored origins, keyed by origin id.
/// Populated during content dispatch from `ContentPayload::Origin` files.
#[derive(Resource, Default)]
pub struct OriginRegistry {
    pub origins: std::collections::HashMap<String, Origin>,
}

impl OriginRegistry {
    pub fn get(&self, id: &str) -> Option<&Origin> {
        self.origins.get(id)
    }
    pub fn all(&self) -> impl Iterator<Item = &Origin> {
        self.origins.values()
    }
}

/// Authored ecosystem overrides keyed by planet id. Populated during startup
/// from `ContentPayload::Ecosystem` files, read by the ecosystem event system
/// on system arrival.
#[derive(Resource, Default)]
pub struct EcosystemOverrideRegistry(pub HashMap<String, Ecosystem>);

/// Authored planet culture overrides keyed by planet id. Populated during
/// startup from `ContentPayload::PlanetCulture` files, read by the culture
/// panel on system arrival or interaction.
#[derive(Resource, Default)]
pub struct CultureOverrideRegistry(pub HashMap<String, PlanetCulture>);

/// Bevy startup system: runs after `dispatch_content`, flushes the stash into
/// the override registries so gameplay systems can read them.
pub fn flush_content_registries(
    mut origin_registry: ResMut<OriginRegistry>,
    mut eco_registry: ResMut<EcosystemOverrideRegistry>,
    mut culture_registry: ResMut<CultureOverrideRegistry>,
) {
    for origin in stash::take_origins() {
        origin_registry.origins.insert(origin.id.clone(), origin);
    }
    for (id, eco) in stash::take_ecosystems() {
        eco_registry.0.insert(id, eco);
    }
    for (id, culture) in stash::take_cultures() {
        culture_registry.0.insert(id, culture);
    }
}

/// Bevy startup system: runs after `load_content_index`, dispatches every
/// content file to its registered consumer, and installs authored contracts
/// into the runtime.
pub fn dispatch_content(
    index: Res<ContentIndex>,
    mut runtime: ResMut<crate::systems::contract::ContractRuntime>,
) {
    let dispatcher = ContentDispatcher::build_default();
    let errors = dispatcher.dispatch_all(&index);
    for (id, msg) in &errors {
        warn!("content dispatch: {id}: {msg}");
    }
    // Install authored contracts from the stash.
    let contracts = stash::take_contracts();
    for c in contracts {
        runtime.install(c);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Every ContentPayloadVariant must have a registered consumer in the
    /// default dispatcher. Adding a new variant without a consumer is a
    /// compile-time reminder via this test.
    #[test]
    fn consumer_coverage_test() {
        let d = ContentDispatcher::build_default();
        let registered = d.registered_variants();
        for variant in ContentPayloadVariant::all() {
            assert!(
                registered.contains(variant),
                "ContentPayloadVariant::{variant:?} has no registered consumer"
            );
        }
    }

    /// `ContentPayloadVariant` is a hand-maintained mirror of core's
    /// `AssetType`, and `all()` is a hand-written list. `from_asset_type` is an
    /// exhaustive match, so a new `AssetType` breaks the build — but nothing
    /// stopped a developer from adding the variant to the enum and the match
    /// while forgetting `all()`, which would leave the coverage gate above
    /// silently testing a short list. This closes that hole from the core side.
    #[test]
    fn every_asset_type_maps_into_the_variant_list() {
        use reachlock_core::content::envelope::AssetType;
        let listed = ContentPayloadVariant::all();
        for at in AssetType::ALL {
            let variant = ContentPayloadVariant::from_asset_type(at).unwrap_or_else(|| {
                panic!("AssetType::{at:?} has no ContentPayloadVariant mapping")
            });
            assert!(
                listed.contains(&variant),
                "AssetType::{at:?} maps to {variant:?}, which is missing from \
                 ContentPayloadVariant::all() — the dispatch coverage gate \
                 would not check it"
            );
        }
        assert_eq!(
            listed.len(),
            AssetType::ALL.len(),
            "ContentPayloadVariant::all() and AssetType::ALL have diverged"
        );
    }

    /// The stash round-trips cleanly: set → take returns the same data.
    #[test]
    fn stash_round_trip() {
        use reachlock_core::contract::types::{Contract, Trigger};
        let c = Contract {
            id: "test".into(),
            label: "test".into(),
            trigger: Trigger::Manual,
            rules: vec![],
            llm_authority: None,
        };
        stash::set_contracts(vec![c.clone()]);
        let taken = stash::take_contracts();
        assert_eq!(taken.len(), 1);
        assert_eq!(taken[0].id, "test");
    }
}
