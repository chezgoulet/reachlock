//! `ContentFile`: the envelope every authored `.ron` asset deserializes
//! into (spec §10, "Freeze first"). Field names here are the compatibility
//! surface for every file under `content/` — don't rename without a
//! migration plan.

use serde::{Deserialize, Serialize};

use crate::contract::types::Contract;
use crate::editor::exterior::HullFrame;
use crate::editor::interior::RoomTemplate;
use crate::generator::{Ecosystem, GeneratedLayout, GeneratedMesh};
use crate::soul::types::SoulFile;
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
    /// S39: a full ecosystem override (spec §5/§17).
    Ecosystem,
    /// S13: an NPC soul (spec §15) — the pipeline's fourth content type.
    Soul,
    /// S17: an exterior hull frame (spec §19) — slot layout, engine mount,
    /// plating zones. Protocol revision: adding this variant extended the
    /// envelope's wire vocabulary (iron rule #4, noted in the S17 PR).
    HullFrame,
    /// S18: a room template set (spec §19) — one file carries the whole
    /// authored list (`content/hulls/room_templates.ron`), since templates
    /// only mean anything as a set the interior editor picks from. Protocol
    /// revision noted in the S18 PR.
    RoomTemplates,
}

/// A non-player character placed in a station interior. `room_index` points
/// into the station's `GeneratedLayout::rooms` so the renderer/loader can
/// drop the figure in the right room. `dialogue` is the authored line list
/// the talk verb surfaces (S07; S13/S16 swap the *source*, not the panel).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NpcSpawn {
    pub room_index: usize,
    pub name: String,
    #[serde(default)]
    pub dialogue: Vec<String>,
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
        /// Authored NPCs (S07). Default-empty so generated/legacy station
        /// payloads still deserialize.
        #[serde(default)]
        npc_spawns: Vec<NpcSpawn>,
    },
    Contract(Contract),
    /// S39: an authored ecosystem (spec §5). Boxed because ecosystems carry
    /// a full species list and food web for each biome.
    Ecosystem(Box<Ecosystem>),
    /// S13: who an NPC is (spec §15). Souls are data; the contract engine
    /// decides how they act, S16 decides what they say. Boxed: a soul is an
    /// order of magnitude bigger than the other variants, and serde treats
    /// the box as transparent.
    Soul(Box<SoulFile>),
    /// S17: a hull frame's structural constants (spec §19). The exterior
    /// editor composes a `HullConfiguration` against exactly this data.
    HullFrame(HullFrame),
    /// S18: the authored room template set (spec §19). The interior editor
    /// places these; `editor::interior::realize` turns placements into the
    /// walkable layout.
    RoomTemplates(Vec<RoomTemplate>),
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

    /// Station payload with NPCs — locks the serialized form so a silent
    /// schema change (renaming `npc_spawns`, `room_index`, `dialogue`, …)
    /// is caught. Iron rule #4: content schemas have tests that lock their
    /// serialized form.
    #[test]
    fn station_with_npcs_serialized_form_is_locked() {
        let file = ContentFile {
            id: "sorrow_station".into(),
            display_name: "Sorrow Station".into(),
            asset_type: AssetType::Station,
            seed: 4218130448322139,
            universe: "all".into(),
            priority: Priority::Curated,
            expires_at: None,
            payload: ContentPayload::Station {
                exterior: GeneratedMesh {
                    vertices: vec![FixedVec2 {
                        x: Fixed(0),
                        y: Fixed(0),
                    }],
                    indices: vec![],
                },
                layout: GeneratedLayout {
                    rooms: vec![crate::generator::Room {
                        kind: crate::generator::RoomKind::Bar,
                        x: 0,
                        y: 0,
                        width: 32,
                        height: 32,
                    }],
                    doors: vec![],
                },
                npc_spawns: vec![NpcSpawn {
                    room_index: 0,
                    name: "Mara".into(),
                    dialogue: vec!["Hello, traveler.".into()],
                }],
            },
        };
        let text = ron::to_string(&file).unwrap();
        assert!(
            text.contains("npc_spawns"),
            "serialized form must keep field name: {text}"
        );
        assert!(text.contains("room_index"));
        // Defaulted field stays round-trippable with the same bytes.
        let back: ContentFile = ron::from_str(&text).unwrap();
        assert_eq!(file, back);
    }

    /// S17: hull-frame payloads lock their serialized form the same way —
    /// `content/hulls/*_frame.ron` files depend on these field names.
    #[test]
    fn hull_frame_serialized_form_is_locked() {
        use crate::editor::exterior::HullFrame;
        use crate::generator::hull::HullClass;

        let file = ContentFile {
            id: "frame_corvette".into(),
            display_name: "Corvette Frame".into(),
            asset_type: AssetType::HullFrame,
            seed: 7_681_152_800_107_288,
            universe: "all".into(),
            priority: Priority::Curated,
            expires_at: None,
            payload: ContentPayload::HullFrame(HullFrame::reference(HullClass::Corvette)),
        };
        let text = ron::to_string(&file).unwrap();
        for field in [
            "hull_frame",
            "slots",
            "engine_mount",
            "zones",
            "decal_slots",
            "size_class",
            // S18: interior placement area — an additive frame revision.
            "grid_bounds",
        ] {
            assert!(text.contains(field), "missing {field} in: {text}");
        }
        let back: ContentFile = ron::from_str(&text).unwrap();
        assert_eq!(file, back);
    }

    /// S39: ecosystem payloads lock their serialized form —
    /// `content/ecosystems/*.ron` depends on these field names.
    #[test]
    fn ecosystem_serialized_form_is_locked() {
        use crate::generator::ecosystem::{
            BiomeEcosystem, EcologicalRole, Ecosystem, EcosystemComplexity, FoodWeb, Species,
            SpeciesVisual, Taxonomy,
        };
        use crate::generator::ecosystem::{BodyPlan, Edibility};
        use crate::item::types::Rarity;
        use crate::seed::types::Biome;
        use crate::util::color::ColorRgba8;
        use crate::util::Fixed;

        let species = Species {
            id: "test-0".into(),
            taxonomy: Taxonomy {
                kingdom: "A".into(),
                phylum: "B".into(),
                class: "C".into(),
                order: "D".into(),
                family: "E".into(),
                genus: "F".into(),
                species: "g".into(),
            },
            common_name: "test lurker".into(),
            scientific_name: "F g".into(),
            ecological_role: EcologicalRole::PrimaryProducer,
            size_class: crate::editor::exterior::SizeClass::Small,
            habitat: "test".into(),
            rarity: Rarity::Common,
            visual: SpeciesVisual {
                silhouette: 0,
                primary_color: ColorRgba8 {
                    r: 10,
                    g: 20,
                    b: 30,
                    a: 255,
                },
                secondary_color: ColorRgba8 {
                    r: 40,
                    g: 50,
                    b: 60,
                    a: 255,
                },
                body_plan: BodyPlan::Radial,
                size_hint: "fist".into(),
            },
            discoverable: true,
            research_value: 10,
            edibility: Edibility::Inedible,
            medicinal_potential: 0,
            danger_level: 0,
        };
        let eco = Ecosystem {
            planet_seed: 999,
            biomes: vec![BiomeEcosystem {
                biome: Biome::Frontier,
                species: vec![species],
                food_web: FoodWeb { edges: vec![] },
                keystone_species: vec![],
            }],
            global_species_count: 1,
            endemic_species_count: 1,
            ecological_complexity: EcosystemComplexity::Simple,
            baseline_recorded: false,
        };
        let file = ContentFile {
            id: "test_eco".into(),
            display_name: "Test Eco".into(),
            asset_type: AssetType::Ecosystem,
            seed: 999,
            universe: "all".into(),
            priority: Priority::Authoritative,
            expires_at: None,
            payload: ContentPayload::Ecosystem(Box::new(eco)),
        };
        let text = ron::to_string(&file).unwrap();
        for field in ["ecosystem", "common_name", "scientific_name", "food_web", "keystone_species"] {
            assert!(text.contains(field), "missing {field} in: {text}");
        }
        let back: ContentFile = ron::from_str(&text).unwrap();
        assert_eq!(file, back);
    }

    /// S18: room-template payloads lock their serialized form the same way
    /// — `content/hulls/room_templates.ron` depends on these field names.
    #[test]
    fn room_templates_serialized_form_is_locked() {
        use crate::editor::interior::RoomTemplate;

        let file = ContentFile {
            id: "room_templates".into(),
            display_name: "Room Templates".into(),
            asset_type: AssetType::RoomTemplates,
            seed: 4_912_338_771_002_441,
            universe: "all".into(),
            priority: Priority::Curated,
            expires_at: None,
            payload: ContentPayload::RoomTemplates(RoomTemplate::reference_set()),
        };
        let text = ron::to_string(&file).unwrap();
        for field in [
            "room_templates",
            "kind",
            "label",
            "width",
            "height",
            "required_systems",
            "furniture_slots",
            "adjacent_pairs",
        ] {
            assert!(text.contains(field), "missing {field} in: {text}");
        }
        let back: ContentFile = ron::from_str(&text).unwrap();
        assert_eq!(file, back);
    }
}
