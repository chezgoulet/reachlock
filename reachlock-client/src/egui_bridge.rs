use bevy::prelude::*;

use crate::systems::interaction::ActivePanel;
use crate::theme;

#[derive(Component)]
pub struct EguiManaged;

pub fn sync_egui_context(
    mut commands: Commands,
    panel: Res<ActivePanel>,
    query: Query<Entity, With<EguiManaged>>,
) {
    let needs_egui = matches!(
        *panel,
        ActivePanel::ContractWorkshop
            | ActivePanel::ContractLibrary
            | ActivePanel::ShipExterior
            | ActivePanel::ShipInterior
    );

    if needs_egui && query.is_empty() {
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(80.0),
                height: Val::Percent(80.0),
                left: Val::Percent(10.0),
                top: Val::Percent(10.0),
                ..default()
            },
            theme::surface("surface.sunk"),
            EguiManaged,
            Visibility::Visible,
            ZIndex(10),
        ));
    }

    if !needs_egui && !query.is_empty() {
        for e in &query {
            commands.entity(e).despawn();
        }
    }
}
