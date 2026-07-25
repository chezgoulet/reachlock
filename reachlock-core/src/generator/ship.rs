//! Authored ship interiors (docs/SHIPS.md). Unlike station interiors, a
//! ship's layout is not seeded — it's the deck plan of a specific hull
//! class, laid out to make sense within the ship's footprint. The
//! Loup-Garou (docs/LORE.md §IV) is the first and the design anchor: two
//! decks joined by a ladder, zero-g Upstairs where the ship works, gravity
//! Downstairs where the crew lives.
//!
//! Grid units match the station generator (the client scales them the same
//! way), fore is +y.

use serde::{Deserialize, Serialize};

use super::GeneratedLayout;

/// One deck of a ship interior: a layout plus its gravity profile and the
/// grid-unit point where the inter-deck ladder stands. Ladder points are
/// vertically aligned across decks so climbing keeps your position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShipDeck {
    pub name: String,
    /// Zero-g deck: humans move slow (mag boots), robots move fast.
    pub zero_g: bool,
    pub layout: GeneratedLayout,
    /// Grid-unit position of the ladder between decks.
    pub ladder: (i32, i32),
}

/// A whole ship interior. `decks[0]` is where boarding puts you (the deck
/// with the airlock).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShipInterior {
    pub decks: Vec<ShipDeck>,
}

/// Load a `ShipInterior` from the template catalog.
/// Returns `None` if the id is not found (caller falls back to a default).
pub fn load_ship_template(
    id: &str,
    templates: &[crate::crew::ShipTemplate],
) -> Option<ShipInterior> {
    templates
        .iter()
        .find(|t| t.id == id)
        .map(|t| t.interior.clone())
}
