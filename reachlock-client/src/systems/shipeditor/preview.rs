use bevy::prelude::*;
use reachlock_core::editor::exterior::{HullFrame, SizeClass};
use reachlock_core::generator::hull::generate_hull_class;

pub struct HullPreview {
    pub outline: Vec<Vec2>,
    pub hardpoints: Vec<(Vec2, SizeClass)>,
}

pub fn hull_to_preview(frame: &HullFrame, seed: u64) -> HullPreview {
    let mesh = generate_hull_class(seed, frame.class);

    let outer = &mesh.vertices[1..];
    let min_x = outer.iter().map(|v| v.x.0).min().unwrap_or(0);
    let max_x = outer.iter().map(|v| v.x.0).max().unwrap_or(0);
    let min_y = outer.iter().map(|v| v.y.0).min().unwrap_or(0);
    let max_y = outer.iter().map(|v| v.y.0).max().unwrap_or(0);
    let span_x = (max_x - min_x).max(1) as f32;
    let span_y = (max_y - min_y).max(1) as f32;
    let scale = (160.0 / span_x).min(120.0 / span_y);
    let center = Vec2::new((min_x + max_x) as f32 / 2.0, (min_y + max_y) as f32 / 2.0);
    let offset = Vec2::new(200.0, 120.0);

    let outline: Vec<Vec2> = outer
        .iter()
        .map(|v| Vec2::new(v.x.0 as f32 - center.x, v.y.0 as f32 - center.y) * scale + offset)
        .collect();

    let hardpoints: Vec<(Vec2, SizeClass)> = frame
        .slots
        .iter()
        .map(|hp| {
            let pos = Vec2::new(
                hp.position.x.0 as f32 - center.x,
                hp.position.y.0 as f32 - center.y,
            ) * scale
                + offset;
            (pos, hp.size_class)
        })
        .collect();

    HullPreview {
        outline,
        hardpoints,
    }
}

pub fn draw_hull_preview(gizmos: &mut Gizmos, preview: &HullPreview) {
    for i in 0..preview.outline.len() {
        let a = preview.outline[i];
        let b = preview.outline[(i + 1) % preview.outline.len()];
        gizmos.line_2d(a, b, Color::WHITE);
    }

    for (pos, size) in &preview.hardpoints {
        let s = match size {
            SizeClass::Small => 6.0,
            SizeClass::Medium => 10.0,
            SizeClass::Large => 14.0,
        };
        let color = match size {
            SizeClass::Small => Color::srgb(0.3, 0.5, 1.0),
            SizeClass::Medium => Color::srgb(1.0, 1.0, 0.3),
            SizeClass::Large => Color::srgb(1.0, 0.3, 0.3),
        };
        let half = s / 2.0;
        let tl = Vec2::new(pos.x - half, pos.y + half);
        let tr = Vec2::new(pos.x + half, pos.y + half);
        let bl = Vec2::new(pos.x - half, pos.y - half);
        let br = Vec2::new(pos.x + half, pos.y - half);
        gizmos.line_2d(tl, tr, color);
        gizmos.line_2d(tr, br, color);
        gizmos.line_2d(br, bl, color);
        gizmos.line_2d(bl, tl, color);
    }
}
