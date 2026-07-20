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
    ChartedSystem,
    HullMesh,
    RoomTemplates,
    GateNetwork,
    /// Phase 2 previewer — nothing persisted; browses generated items live.
    ItemBrowser,
    /// Phase 2 previewer — character look explorer over the sprite generator.
    SpriteViewer,
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
            ContentType::ChartedSystem,
            ContentType::HullMesh,
            ContentType::RoomTemplates,
            ContentType::GateNetwork,
            ContentType::ItemBrowser,
            ContentType::SpriteViewer,
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
            ContentType::EnemyArchetype => "enemies",
            ContentType::ChartedSystem => "systems",
            ContentType::HullMesh => "hulls",
            ContentType::RoomTemplates => "hulls",
            ContentType::GateNetwork => "gate_network",
            ContentType::ItemBrowser => "items",
            ContentType::SpriteViewer => "souls",
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
    r.register(
        ContentType::HullFrame,
        crate::editors::hull_frame::create_editor,
    );
    r.register(ContentType::Station, crate::editors::station::create_editor);
    r.register(ContentType::Location, crate::editors::location::create_editor);
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
    r
}
