use bevy::prelude::*;

use crate::settings::{InputAction, Settings};
use crate::theme;

#[derive(Clone, Debug)]
pub struct SignatureRequest {
    pub contract_id: String,
    pub action_description: String,
    pub evaluated_by: String,
}

#[derive(Clone, Debug)]
pub struct PendingSignature {
    pub contract_id: String,
    pub action_description: String,
    pub evaluated_by: String,
    pub signed: bool,
    pub rejected: bool,
}

#[derive(Resource, Default)]
pub struct SignatureCollector {
    pub pending: Vec<PendingSignature>,
    pub incoming: Vec<SignatureRequest>,
}

#[derive(Resource, Default)]
pub struct SignatureCollectorVisible(pub bool);

#[derive(Component)]
pub struct SignaturePanel;

pub fn spawn_signature_panel(mut commands: Commands) {
    commands.spawn((
        SignaturePanel,
        Text::new(""),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        theme::fg("text.warn"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(200.0),
            right: Val::Px(16.0),
            max_width: Val::Px(360.0),
            ..default()
        },
        Visibility::Hidden,
    ));
}

pub fn render_signature_panel(
    visible: Res<SignatureCollectorVisible>,
    collector: Res<SignatureCollector>,
    mut query: Query<(&mut Text, &mut Visibility), With<SignaturePanel>>,
) {
    if let Ok((mut text, mut vis)) = query.single_mut() {
        if !visible.0 {
            *vis = Visibility::Hidden;
            **text = String::new();
            return;
        }
        *vis = Visibility::Visible;
        let mut lines = vec!["── SIGNATURES ──".to_string()];
        if collector.pending.is_empty() {
            lines.push("  No pending signatures.".into());
        } else {
            for (i, sig) in collector.pending.iter().enumerate() {
                let status = if sig.signed {
                    "signed"
                } else if sig.rejected {
                    "rejected"
                } else {
                    "pending"
                };
                lines.push(format!(
                    "  {}. {} — {} [{}] ({})",
                    i + 1,
                    sig.contract_id,
                    sig.action_description,
                    sig.evaluated_by,
                    status,
                ));
            }
            lines.push(String::new());
            lines.push("  [Enter] sign selected  [R] reject  [G] close".into());
        }
        **text = lines.join("\n");
    }
}

pub fn collect_signature(
    mut collector: ResMut<SignatureCollector>,
    mut visible: ResMut<SignatureCollectorVisible>,
) {
    let requests: Vec<_> = collector.incoming.drain(..).collect();
    for req in requests {
        collector.pending.push(PendingSignature {
            contract_id: req.contract_id,
            action_description: req.action_description,
            evaluated_by: req.evaluated_by,
            signed: false,
            rejected: false,
        });
        visible.0 = true;
    }
}

pub fn toggle_signature_panel(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    mut visible: ResMut<SignatureCollectorVisible>,
) {
    if keys.just_pressed(KeyCode::KeyG)
        || keys.just_pressed(settings.key(InputAction::OpenMissionBoard))
    {
        visible.0 = !visible.0;
    }
}
