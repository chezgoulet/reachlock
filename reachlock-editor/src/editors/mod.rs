pub mod charted_system;
pub mod contract;
pub mod economy;
pub mod enemy;
pub mod faction;
pub mod gate_network;
// Reference implementation for `HullConfiguration` editing; superseded in the
// registry by `hull_frame` (the authored-frame editor) but kept as the
// pattern exemplar the handoff cites.
#[allow(dead_code)]
pub mod hull;
pub mod hull_frame;
pub mod hull_mesh;
pub mod item;
pub mod room_templates;
pub mod location;
pub mod soul;
pub mod station;
pub mod storyline;
pub mod widgets;

pub fn register_all(registry: &mut super::app::EditorRegistry) {
    registry.register(
        super::app::ContentType::HullFrame,
        hull_frame::create_editor,
    );
    registry.register(super::app::ContentType::Station, station::create_editor);
    registry.register(super::app::ContentType::Location, location::create_editor);
    registry.register(super::app::ContentType::Soul, soul::create_editor);
    registry.register(super::app::ContentType::Contract, contract::create_editor);
    registry.register(super::app::ContentType::Faction, faction::create_editor);
    registry.register(super::app::ContentType::EconomyGoods, economy::create_editor);
    registry.register(super::app::ContentType::Storyline, storyline::create_editor);
    registry.register(super::app::ContentType::Item, item::create_editor);
    registry.register(super::app::ContentType::EnemyArchetype, enemy::create_editor);
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
}
