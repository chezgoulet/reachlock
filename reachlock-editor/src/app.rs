use std::collections::HashMap;

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
}

impl ContentType {
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
        ]
    }

    pub fn name(&self) -> &str {
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
        }
    }

    pub fn directory(&self) -> &str {
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
            ContentType::EnemyArchetype => "enemies",
        }
    }
}

pub trait Editor {
    fn title(&self) -> &str;
    fn content_type(&self) -> ContentType;
    fn has_unsaved_changes(&self) -> bool;
    fn load(&mut self, path: &std::path::Path) -> Result<(), String>;
    fn save(&self, path: &std::path::Path) -> Result<(), String>;
    fn validate(&self) -> Vec<String>;
    fn ui(&mut self, ctx: &egui::Context);
    fn generate_from_seed(&mut self, seed: u64);
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
    r.register(ContentType::HullFrame, crate::editors::hull::create_editor);
    r.register(ContentType::Station, crate::editors::station::create_editor);
    r.register(ContentType::Location, crate::editors::location::create_editor);
    r.register(ContentType::Soul, crate::editors::soul::create_editor);
    r.register(ContentType::Contract, || {
        crate::editors::dialogue::create_editor()
    });
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
    r
}
