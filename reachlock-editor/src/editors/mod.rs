pub mod dialogue;
pub mod economy;
pub mod enemy;
pub mod faction;
pub mod hull;
pub mod item;
pub mod location;
pub mod soul;
pub mod station;
pub mod storyline;

pub fn register_all(registry: &mut super::app::EditorRegistry) {
    registry.register(super::app::ContentType::HullFrame, hull::create_editor);
    registry.register(super::app::ContentType::Station, station::create_editor);
    registry.register(super::app::ContentType::Location, location::create_editor);
    registry.register(super::app::ContentType::Soul, soul::create_editor);
    registry.register(super::app::ContentType::Contract, dialogue::create_editor);
    registry.register(super::app::ContentType::Faction, faction::create_editor);
    registry.register(super::app::ContentType::EconomyGoods, economy::create_editor);
    registry.register(super::app::ContentType::Storyline, storyline::create_editor);
    registry.register(super::app::ContentType::Item, item::create_editor);
    registry.register(super::app::ContentType::EnemyArchetype, enemy::create_editor);
}
