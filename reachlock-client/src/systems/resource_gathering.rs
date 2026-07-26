use bevy::prelude::*;

use reachlock_core::economy::GoodId;

use crate::settings::{InputAction, Settings};
use crate::systems::inventory::PlayerInventory;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResourceType {
    #[default]
    Mineral,
    Organic,
    Gas,
    Water,
}

#[derive(Component, Clone, Debug)]
pub struct ResourceNode {
    pub resource_type: ResourceType,
    pub yield_amount: u32,
    pub difficulty: u32,
    pub depleted: bool,
}

#[derive(Resource)]
pub struct GatheringProgress {
    pub active: bool,
    pub target: Entity,
    pub elapsed_ticks: u32,
    pub total_ticks: u32,
    pub resource_type: ResourceType,
}

impl Default for GatheringProgress {
    fn default() -> Self {
        GatheringProgress {
            active: false,
            target: Entity::PLACEHOLDER,
            elapsed_ticks: 0,
            total_ticks: 0,
            resource_type: ResourceType::Mineral,
        }
    }
}

impl GatheringProgress {
    pub fn progress(&self) -> f32 {
        if self.total_ticks == 0 {
            1.0
        } else {
            self.elapsed_ticks as f32 / self.total_ticks as f32
        }
    }
}

#[derive(Component)]
pub struct GatherProgressBar;

#[derive(Component)]
pub struct GatherProgressFill;

pub fn interact_with_resource(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    avatar: Query<&Transform, With<crate::systems::mode::PlayerAvatar>>,
    nodes: Query<(Entity, &Transform, &ResourceNode)>,
    mut progress: ResMut<GatheringProgress>,
) {
    if progress.active {
        return;
    }
    let Ok(av) = avatar.single() else {
        return;
    };
    let av_pos = av.translation.truncate();
    let interact_key = settings.key(InputAction::Interact);
    if !keys.just_pressed(interact_key) && !keys.just_pressed(KeyCode::KeyE) {
        return;
    }
    let mut nearest: Option<(f32, Entity, u32, ResourceType)> = None;
    for (e, t, node) in &nodes {
        if node.depleted {
            continue;
        }
        let pos = t.translation.truncate();
        let d = pos.distance(av_pos);
        if d <= 48.0 {
            let better = match &nearest {
                None => true,
                Some(n) => d < n.0,
            };
            if better {
                nearest = Some((d, e, node.difficulty, node.resource_type));
            }
        }
    }
    if let Some((_, target, difficulty, resource_type)) = nearest {
        progress.active = true;
        progress.target = target;
        progress.elapsed_ticks = 0;
        progress.total_ticks = difficulty;
        progress.resource_type = resource_type;
    }
}

pub fn tick_gathering(
    mut progress: ResMut<GatheringProgress>,
    mut nodes: Query<&mut ResourceNode>,
    mut inventory: ResMut<PlayerInventory>,
) {
    if !progress.active {
        return;
    }
    progress.elapsed_ticks += 1;
    if progress.elapsed_ticks < progress.total_ticks {
        return;
    }
    if let Ok(mut node) = nodes.get_mut(progress.target) {
        node.depleted = true;
        let good_id = match progress.resource_type {
            ResourceType::Mineral => GoodId("mineral_ore".into()),
            ResourceType::Organic => GoodId("biomass".into()),
            ResourceType::Gas => GoodId("volatile_gas".into()),
            ResourceType::Water => GoodId("water_ice".into()),
        };
        *inventory.cargo.entry(good_id).or_insert(0) += node.yield_amount;
        info!(
            "gathered {} units of {:?}",
            node.yield_amount, progress.resource_type
        );
    }
    progress.active = false;
}

pub fn spawn_gathering_progress_bar(mut commands: Commands) {
    commands.spawn((
        GatherProgressBar,
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(80.0),
            left: Val::Percent(46.0),
            width: Val::Px(120.0),
            height: Val::Px(12.0),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BorderColor::all(Color::srgb(0.6, 0.6, 0.6)),
        BackgroundColor(Color::srgb(0.15, 0.15, 0.15)),
        Visibility::Hidden,
    ));
    commands.spawn((
        GatherProgressFill,
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(80.0),
            left: Val::Percent(46.0),
            width: Val::Px(0.0),
            height: Val::Px(12.0),
            ..default()
        },
        BackgroundColor(Color::srgb(0.2, 0.8, 0.3)),
        Visibility::Hidden,
    ));
}

pub fn render_gathering_progress(
    progress: Res<GatheringProgress>,
    mut bar_query: Query<&mut Visibility, (With<GatherProgressBar>, Without<GatherProgressFill>)>,
    mut fill_query: Query<(&mut Node, &mut Visibility), With<GatherProgressFill>>,
) {
    if progress.active {
        if let Ok(mut bar_vis) = bar_query.single_mut() {
            *bar_vis = Visibility::Visible;
        }
        if let Ok((mut node, mut fill_vis)) = fill_query.single_mut() {
            *fill_vis = Visibility::Visible;
            let p = progress.progress();
            node.width = Val::Px(120.0 * p);
        }
    } else {
        if let Ok(mut bar_vis) = bar_query.single_mut() {
            *bar_vis = Visibility::Hidden;
        }
        if let Ok((mut node, mut fill_vis)) = fill_query.single_mut() {
            *fill_vis = Visibility::Hidden;
            node.width = Val::Px(0.0);
        }
    }
}
