pub mod career;
pub mod character_sprite;
pub mod charted_system;
pub mod contract;
pub mod dialogue;
pub mod dungeon;
pub mod economy;
pub mod ecosystem;
pub mod enemy;
pub mod event;
pub mod faction;
pub mod gate_network;
pub mod origin;
pub mod planet_culture;
// `hull` was a reference implementation for `HullConfiguration` editing,
// superseded in the registry by `hull_frame`. Removed from mod.rs (not the
// file system) because it added compile time for a module that was
// `#[allow(dead_code)]` and not registered in any dispatch path. The file
// remains on disk as a study reference.
pub mod hull_frame;
pub mod hull_mesh;
pub mod item;
pub mod item_browser;
pub mod location;
pub mod recipe;
pub mod room_templates;
pub mod scripted_encounter;
pub mod soul;
pub mod station;
pub mod storyline;
pub mod theme;
pub mod trope;
pub mod widgets;

pub fn register_all(registry: &mut super::app::EditorRegistry) {
    registry.register(super::app::ContentType::Career, career::create_editor);
    registry.register(super::app::ContentType::Ecosystem, ecosystem::create_editor);
    registry.register(super::app::ContentType::Theme, theme::create_editor);
    registry.register(super::app::ContentType::Trope, trope::create_editor);
    registry.register(
        super::app::ContentType::PlanetCulture,
        planet_culture::create_editor,
    );
    registry.register(
        super::app::ContentType::HullFrame,
        hull_frame::create_editor,
    );
    registry.register(super::app::ContentType::Station, station::create_editor);
    registry.register(super::app::ContentType::Location, location::create_editor);
    registry.register(super::app::ContentType::Soul, soul::create_editor);
    registry.register(super::app::ContentType::Contract, contract::create_editor);
    registry.register(super::app::ContentType::Faction, faction::create_editor);
    registry.register(
        super::app::ContentType::EconomyGoods,
        economy::create_editor,
    );
    registry.register(super::app::ContentType::Storyline, storyline::create_editor);
    registry.register(super::app::ContentType::Item, item::create_editor);
    registry.register(
        super::app::ContentType::EnemyArchetype,
        enemy::create_editor,
    );
    registry.register(
        super::app::ContentType::ChartedSystem,
        charted_system::create_editor,
    );
    registry.register(super::app::ContentType::HullMesh, hull_mesh::create_editor);
    registry.register(
        super::app::ContentType::RoomTemplates,
        room_templates::create_editor,
    );
    registry.register(
        super::app::ContentType::GateNetwork,
        gate_network::create_editor,
    );
    registry.register(
        super::app::ContentType::ItemBrowser,
        item_browser::create_editor,
    );
    registry.register(
        super::app::ContentType::SpriteViewer,
        character_sprite::create_editor,
    );
    registry.register(super::app::ContentType::Dungeon, dungeon::create_editor);
    registry.register(super::app::ContentType::Event, event::create_editor);
    registry.register(super::app::ContentType::Dialogue, dialogue::create_editor);
    registry.register(super::app::ContentType::Recipe, recipe::create_editor);
    registry.register(super::app::ContentType::Origin, origin::create_editor);
}
