//! `ContentFile`: the envelope every authored `.ron` asset deserializes
//! into (spec §10, "Freeze first"). Field names here are the compatibility
//! surface for every file under `content/` — don't rename without a
//! migration plan.

use serde::{Deserialize, Serialize};

use crate::contract::types::Contract;
use crate::generator::{GeneratedLayout, GeneratedMesh};
use crate::universe::tier::UniverseTier;

use super::priority::Priority;

/// What kind of authored asset a `ContentFile` carries. Mirrors the
/// generator primitives it can replace (spec §10, Content Types table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetType {
    Hull,
    Station,
    Contract,
}

/// The authored payload — the exact same plain-data structs the generators
/// emit (spec §10: "the bridge doesn't know the difference"). One variant
/// per `AssetType`; keep the two in sync.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentPayload {
    Hull(GeneratedMesh),
    Station {
        exterior: GeneratedMesh,
        layout: GeneratedLayout,
    },
    Contract(Contract),
}

/// The content envelope (spec §10, "Freeze first" list: id, display_name,
/// asset_type, seed, universe, priority, payload).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentFile {
    pub id: String,
    pub display_name: String,
    pub asset_type: AssetType,
    /// Canonical seed (spec §10, Seed Integration):
    /// `hash("content_override", system_id, object_id)` — see
    /// [`super::seed::content_seed`]. Authored files pin the value
    /// explicitly so a stray edit is diffable against a recomputation.
    pub seed: u64,
    /// `"all"`, or a [`UniverseTier`] name (`"classic"`, `"fair_play"`,
    /// `"spectrum"`, `"byok"`). A plain string (not the tier enum) because
    /// "all universes" has no tier value of its own — see
    /// `content_overrides.universe` in spec §11, which is nullable for the
    /// same reason.
    pub universe: String,
    pub priority: Priority,
    /// Only meaningful when `priority == Priority::Event` (spec §10,
    /// Content Lifecycle: "Event content auto-removes").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
    pub payload: ContentPayload,
}

impl ContentFile {
    /// True if this file's `universe` field applies to `tier`.
    pub fn matches_universe(&self, tier: UniverseTier) -> bool {
        self.universe == "all" || self.universe == tier.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generator::FixedVec2;
    use crate::util::rng::Fixed;

    fn hull_file() -> ContentFile {
        ContentFile {
            id: "loup_garou".into(),
            display_name: "Loup-Garou".into(),
            asset_type: AssetType::Hull,
            seed: 12345,
            universe: "all".into(),
            priority: Priority::Authoritative,
            expires_at: None,
            payload: ContentPayload::Hull(GeneratedMesh {
                vertices: vec![FixedVec2 {
                    x: Fixed(0),
                    y: Fixed(0),
                }],
                indices: vec![],
            }),
        }
    }

    #[test]
    fn matches_universe_all_matches_every_tier() {
        let file = hull_file();
        for tier in UniverseTier::ALL {
            assert!(file.matches_universe(tier));
        }
    }

    #[test]
    fn matches_universe_specific_tier_only() {
        let mut file = hull_file();
        file.universe = "classic".into();
        assert!(file.matches_universe(UniverseTier::Classic));
        assert!(!file.matches_universe(UniverseTier::Spectrum));
    }

    /// Round-trips through RON — the actual authoring format — not just
    /// JSON, since RON's enum-variant syntax is where authors will
    /// actually hit typos (spec §10 gotcha).
    #[test]
    fn ron_round_trip() {
        let file = hull_file();
        let text = ron::to_string(&file).unwrap();
        let back: ContentFile = ron::from_str(&text).unwrap();
        assert_eq!(file, back);
    }
}
