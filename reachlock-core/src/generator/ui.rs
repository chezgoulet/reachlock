//! UI panel generation (spec §5): seeded panel geometry. The theme of a
//! faction's consoles comes from its seed — same layout everywhere.

use super::{Door, GeneratedLayout, Room, RoomKind};
use crate::util::rng::SeededRng;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelType {
    Hud,
    StationServices,
    ShipLog,
}

/// UI layouts reuse `GeneratedLayout`: rooms are panel regions, doors are
/// unused. (Element metadata grows here when the client HUD needs it —
/// keeping one layout type until then.)
pub fn generate_ui_panel(seed: u64, panel: PanelType, width: i32, height: i32) -> GeneratedLayout {
    let mut rng = SeededRng::new(seed ^ 0x0117_A9E1);
    let regions: usize = match panel {
        PanelType::Hud => 3,
        PanelType::StationServices => 4,
        PanelType::ShipLog => 2,
    };

    // Split the panel into vertical bands with seeded proportions.
    // Weights are 1..=4; band height = weight share of total.
    let weights: Vec<i32> = (0..regions).map(|_| 1 + rng.next_below(4) as i32).collect();
    let total: i32 = weights.iter().sum();

    let mut rooms = Vec::with_capacity(regions);
    let mut y = 0;
    for (i, w) in weights.iter().enumerate() {
        let h = if i == regions - 1 {
            height - y // last band absorbs rounding
        } else {
            height * w / total
        };
        rooms.push(Room {
            kind: RoomKind::Corridor, // region kind is cosmetic for panels
            x: 0,
            y,
            width,
            height: h,
        });
        y += h;
    }

    GeneratedLayout {
        rooms,
        doors: Vec::<Door>::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        assert_eq!(
            generate_ui_panel(4, PanelType::Hud, 320, 240),
            generate_ui_panel(4, PanelType::Hud, 320, 240)
        );
    }

    #[test]
    fn bands_tile_exactly() {
        let layout = generate_ui_panel(11, PanelType::StationServices, 320, 240);
        let sum: i32 = layout.rooms.iter().map(|r| r.height).sum();
        assert_eq!(sum, 240, "bands must cover the panel with no gap");
    }
}
