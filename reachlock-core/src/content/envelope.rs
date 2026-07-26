//! `ContentFile`: the envelope every authored `.ron` asset deserializes
//! into (spec §10, "Freeze first"). Field names here are the compatibility
//! surface for every file under `content/` — don't rename without a
//! migration plan.

use serde::{Deserialize, Serialize};

use crate::career::CareerPath;
use crate::contract::types::Contract;
use crate::editor::exterior::HullFrame;
use crate::editor::interior::RoomTemplate;
use crate::faction::Storyline;
use crate::generator::culture::PlanetCulture;
use crate::generator::music::Theme;
use crate::generator::scripted_encounter::ScriptedEncounter;
use crate::generator::trope::TropeTemplate;
use crate::generator::{Ecosystem, GeneratedLayout, GeneratedMesh};
use crate::soul::types::SoulFile;
use crate::universe::tier::UniverseTier;

use super::dialogue::Dialogue;
use super::dungeon::Dungeon;
use super::event::Event;
use super::recipe::Recipe;

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
    /// S42: a career path definition (spec §14/§21). One file per path.
    Career,
    /// S47: a planet culture override (spec §20).
    PlanetCulture,
    /// S55: a crafting recipe (spec §?).
    Recipe,
    /// S48: a music theme for the procedural audio engine.
    Theme,
    /// S40: a trope template for procedural narrative encounters.
    Trope,
    /// S41: a fully authored, multi-scene scripted encounter.
    ScriptedEncounter,
    /// S55: a branching dialogue tree (spec §?).
    Dialogue,
    /// S55: a dungeon layout (spec §?).
    Dungeon,
    /// S55: a scripted event (spec §?).
    Event,
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
    /// S79: an origin starting package (spec §?).
    Origin,
    /// S120: an authored crew package.
    CrewPackage,
    /// S120: authored soul mutation arcs.
    SoulMutations,
    /// S115: a faction storyline arc.
    Storyline,
}

impl AssetType {
    /// Every variant. Paired with [`AssetType::ordinal`], whose exhaustive
    /// match makes adding a variant a compile error until it is listed here —
    /// which is what keeps the client's `ContentPayloadVariant` mirror and its
    /// dispatch-coverage gate honest.
    pub const ALL: [AssetType; 20] = [
        AssetType::Hull,
        AssetType::Station,
        AssetType::Contract,
        AssetType::Ecosystem,
        AssetType::Career,
        AssetType::PlanetCulture,
        AssetType::Recipe,
        AssetType::Theme,
        AssetType::Trope,
        AssetType::ScriptedEncounter,
        AssetType::Dialogue,
        AssetType::Dungeon,
        AssetType::Event,
        AssetType::Soul,
        AssetType::HullFrame,
        AssetType::RoomTemplates,
        AssetType::Origin,
        AssetType::CrewPackage,
        AssetType::SoulMutations,
        AssetType::Storyline,
    ];

    /// Dense index for this variant. The match is exhaustive on purpose.
    pub fn ordinal(self) -> usize {
        match self {
            AssetType::Hull => 0,
            AssetType::Station => 1,
            AssetType::Contract => 2,
            AssetType::Ecosystem => 3,
            AssetType::Career => 4,
            AssetType::PlanetCulture => 5,
            AssetType::Recipe => 6,
            AssetType::Theme => 7,
            AssetType::Trope => 8,
            AssetType::ScriptedEncounter => 9,
            AssetType::Dialogue => 10,
            AssetType::Dungeon => 11,
            AssetType::Event => 12,
            AssetType::Soul => 13,
            AssetType::HullFrame => 14,
            AssetType::RoomTemplates => 15,
            AssetType::Origin => 16,
            AssetType::CrewPackage => 17,
            AssetType::SoulMutations => 18,
            AssetType::Storyline => 19,
        }
    }
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
    /// S42: a career path definition (spec §14/§21). Boxed to keep the
    /// envelope enum small (career paths carry a vec of ranks and perks).
    Career(Box<CareerPath>),
    /// S47: an authored planet culture override (spec §20).
    PlanetCulture(Box<PlanetCulture>),
    /// S48: a music theme (spec §5/§10).
    Theme(Box<Theme>),
    /// S40: a trope template (spec §10).
    Trope(Box<TropeTemplate>),
    /// S41: a scripted encounter (spec §10). Boxed to keep the envelope slim.
    ScriptedEncounter(Box<ScriptedEncounter>),
    /// S55: a branching dialogue tree.
    Dialogue(Box<Dialogue>),
    /// S55: a dungeon layout.
    Dungeon(Box<Dungeon>),
    /// S55: a scripted event.
    Event(Box<Event>),
    /// S55: a crafting recipe.
    Recipe(Box<Recipe>),
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
    /// S79: an origin starting package.
    Origin(super::origin::Origin),
    /// S120: an authored crew package.
    CrewPackage(crate::crew::CrewPackage),
    /// S120: authored soul mutation arcs.
    SoulMutations(Vec<crate::soul::SoulMutation>),
    /// S115: an authored faction storyline arc.
    Storylines(Vec<Storyline>),
}

/// The bridge between a bare payload struct and the [`ContentPayload`] variant
/// that carries it.
///
/// Tools that edit one content type at a time — the content editor above all —
/// want to work with a plain `Theme` or `SoulFile`, not with an enum they have
/// to match on. Without this trait each of them re-derived the mapping by hand,
/// which is how eight editor tabs ended up reading and writing the bare type
/// while the files on disk were envelopes.
///
/// The `Box`ing is asymmetric across [`ContentPayload`] — some variants box
/// their payload to keep the enum small, some don't — and this trait is the one
/// place that asymmetry is handled.
pub trait Enveloped: Sized {
    /// The `asset_type` an envelope carrying this payload declares.
    const ASSET_TYPE: AssetType;
    /// Wrap into the payload enum.
    fn into_payload(self) -> ContentPayload;
    /// Unwrap from the payload enum. `None` when the variant doesn't match.
    fn from_payload(payload: ContentPayload) -> Option<Self>;
}

/// Implement [`Enveloped`] for a payload type. `boxed` marks variants whose
/// payload is `Box`ed in [`ContentPayload`].
macro_rules! impl_enveloped {
    ($ty:ty, $variant:ident, $asset:ident, boxed) => {
        impl Enveloped for $ty {
            const ASSET_TYPE: AssetType = AssetType::$asset;
            fn into_payload(self) -> ContentPayload {
                ContentPayload::$variant(Box::new(self))
            }
            fn from_payload(payload: ContentPayload) -> Option<Self> {
                match payload {
                    ContentPayload::$variant(inner) => Some(*inner),
                    _ => None,
                }
            }
        }
    };
    ($ty:ty, $variant:ident, $asset:ident, plain) => {
        impl Enveloped for $ty {
            const ASSET_TYPE: AssetType = AssetType::$asset;
            fn into_payload(self) -> ContentPayload {
                ContentPayload::$variant(self)
            }
            fn from_payload(payload: ContentPayload) -> Option<Self> {
                match payload {
                    ContentPayload::$variant(inner) => Some(inner),
                    _ => None,
                }
            }
        }
    };
}

impl_enveloped!(Theme, Theme, Theme, boxed);
impl_enveloped!(SoulFile, Soul, Soul, boxed);
impl_enveloped!(CareerPath, Career, Career, boxed);
impl_enveloped!(PlanetCulture, PlanetCulture, PlanetCulture, boxed);
impl_enveloped!(Ecosystem, Ecosystem, Ecosystem, boxed);
impl_enveloped!(TropeTemplate, Trope, Trope, boxed);
impl_enveloped!(
    ScriptedEncounter,
    ScriptedEncounter,
    ScriptedEncounter,
    boxed
);
impl_enveloped!(Dialogue, Dialogue, Dialogue, boxed);
impl_enveloped!(Dungeon, Dungeon, Dungeon, boxed);
impl_enveloped!(Event, Event, Event, boxed);
impl_enveloped!(Recipe, Recipe, Recipe, boxed);
impl_enveloped!(Contract, Contract, Contract, plain);
impl_enveloped!(super::origin::Origin, Origin, Origin, plain);
impl_enveloped!(crate::crew::CrewPackage, CrewPackage, CrewPackage, plain);
impl_enveloped!(HullFrame, HullFrame, HullFrame, plain);
impl_enveloped!(Vec<Storyline>, Storylines, Storyline, plain);
impl_enveloped!(Vec<RoomTemplate>, RoomTemplates, RoomTemplates, plain);

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

    /// Take the inner payload as `T`, or `None` if this envelope carries a
    /// different kind. Consumes the envelope; use it when the metadata has
    /// already been read off.
    pub fn into_inner<T: Enveloped>(self) -> Option<T> {
        T::from_payload(self.payload)
    }

    /// Build an envelope around `inner`, filling `asset_type` from the payload
    /// type so the two can't disagree.
    pub fn wrap<T: Enveloped>(
        id: impl Into<String>,
        display_name: impl Into<String>,
        seed: u64,
        universe: impl Into<String>,
        priority: Priority,
        inner: T,
    ) -> Self {
        ContentFile {
            id: id.into(),
            display_name: display_name.into(),
            asset_type: T::ASSET_TYPE,
            seed,
            universe: universe.into(),
            priority,
            expires_at: None,
            payload: inner.into_payload(),
        }
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

    /// Wrap → RON → parse → unwrap must return exactly what went in, and the
    /// envelope's `asset_type` must come from the payload type rather than
    /// from whatever the caller typed.
    fn round_trips_through_envelope<T>(inner: T)
    where
        T: Enveloped + Clone + PartialEq + std::fmt::Debug,
    {
        let file = ContentFile::wrap(
            "test_id",
            "Test",
            42,
            "all",
            Priority::Authoritative,
            inner.clone(),
        );
        assert_eq!(file.asset_type, T::ASSET_TYPE);
        let text = ron::to_string(&file).expect("serialize");
        let back: ContentFile = ron::from_str(&text).expect("deserialize");
        assert_eq!(back.asset_type, T::ASSET_TYPE);
        let out: T = back.into_inner().expect("payload variant matches");
        assert_eq!(out, inner);
    }

    #[test]
    fn theme_round_trips_through_envelope() {
        use crate::generator::music::{NoteEvent, Scale, Theme, VariationMask};
        round_trips_through_envelope(Theme {
            id: "calm".into(),
            notes: vec![NoteEvent {
                degree: 3,
                octave: 0,
                velocity: 80,
                start_tick: 0,
                duration_ticks: 12,
            }],
            scale: Scale::MinorPentatonic,
            bpm_range: (60, 80),
            allowed_variations: VariationMask(511),
        });
    }

    #[test]
    fn room_templates_round_trip_through_envelope() {
        round_trips_through_envelope(RoomTemplate::reference_set());
    }

    #[test]
    fn hull_frame_round_trips_through_envelope() {
        use crate::generator::hull::HullClass;
        round_trips_through_envelope(HullFrame::reference(HullClass::Corvette));
    }

    #[test]
    fn storylines_round_trip_through_envelope() {
        round_trips_through_envelope(Vec::<Storyline>::new());
    }

    /// Unwrapping to the wrong type is `None`, not a panic and not a silent
    /// default — an editor tab pointed at the wrong file must fail visibly.
    #[test]
    fn unwrapping_the_wrong_payload_type_is_none() {
        use crate::generator::music::{Scale, Theme, VariationMask};
        let file = ContentFile::wrap(
            "calm",
            "Calm",
            1,
            "all",
            Priority::Authoritative,
            Theme {
                id: "calm".into(),
                notes: vec![],
                scale: Scale::MinorPentatonic,
                bpm_range: (60, 80),
                allowed_variations: VariationMask(0),
            },
        );
        assert!(file.clone().into_inner::<SoulFile>().is_none());
        assert!(file.into_inner::<Theme>().is_some());
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

    /// S47: planet-culture payloads lock their serialized form —
    /// `content/cultures/*.ron` depends on these field names.
    #[test]
    fn planet_culture_serialized_form_is_locked() {
        use crate::faction::FactionId;
        use crate::generator::culture::{
            ArchitecturalStyle, ClothingStyle, ColorPreference, ColorScheme, CulturalValue, Custom,
            CustomType, LanguageProfile, OutsiderAttitude, PlanetCulture, SocialStructure,
        };
        use crate::util::color::ColorRgba8;

        let culture = PlanetCulture {
            cultural_id: "test".into(),
            language: LanguageProfile {
                base_language: "Test".into(),
                drift_intensity: 10,
                accent_name: "flat".into(),
                unique_terms: vec!["a".into()],
                greeting: "hi".into(),
                farewell: "bye".into(),
            },
            customs: vec![Custom {
                custom_type: CustomType::Greeting,
                description: "fist".into(),
                trigger: "meet".into(),
            }],
            social_structure: SocialStructure::Egalitarian,
            architecture: ArchitecturalStyle {
                style_name: "test".into(),
                materials: vec!["stone".into()],
                dominant_shape: "dome".into(),
                color_palette: ColorScheme {
                    primary: ColorRgba8 {
                        r: 10,
                        g: 20,
                        b: 30,
                        a: 255,
                    },
                    secondary: ColorRgba8 {
                        r: 40,
                        g: 50,
                        b: 60,
                        a: 255,
                    },
                    accent: ColorRgba8 {
                        r: 70,
                        g: 80,
                        b: 90,
                        a: 255,
                    },
                    preference: ColorPreference::Warm,
                },
                adapted_to: vec![],
            },
            clothing: ClothingStyle {
                style_name: "test".into(),
                primary_material: "synth".into(),
                dominant_colors: ColorScheme {
                    primary: ColorRgba8 {
                        r: 1,
                        g: 2,
                        b: 3,
                        a: 255,
                    },
                    secondary: ColorRgba8 {
                        r: 4,
                        g: 5,
                        b: 6,
                        a: 255,
                    },
                    accent: ColorRgba8 {
                        r: 7,
                        g: 8,
                        b: 9,
                        a: 255,
                    },
                    preference: ColorPreference::Cool,
                },
                practicality_level: 50,
                adapted_to: vec![],
            },
            attitude_toward_outsiders: OutsiderAttitude::Curious,
            faction_allegiance: crate::generator::culture::FactionAllegiance::Loyal {
                faction_id: FactionId("compact".into()),
                intensity: 120,
            },
            dominant_values: vec![CulturalValue::Honor],
            cultural_quirk: "test quirk".into(),
        };
        let file = ContentFile {
            id: "test_culture".into(),
            display_name: "Test Culture".into(),
            asset_type: AssetType::PlanetCulture,
            seed: 4242,
            universe: "all".into(),
            priority: Priority::Authoritative,
            expires_at: None,
            payload: ContentPayload::PlanetCulture(Box::new(culture)),
        };
        let text = ron::to_string(&file).unwrap();
        for field in [
            "planet_culture",
            "base_language",
            "greeting",
            "farewell",
            "social_structure",
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
        for field in [
            "ecosystem",
            "common_name",
            "scientific_name",
            "food_web",
            "keystone_species",
        ] {
            assert!(text.contains(field), "missing {field} in: {text}");
        }
        let back: ContentFile = ron::from_str(&text).unwrap();
        assert_eq!(file, back);
    }

    /// S42: career-path payloads lock their serialized form —
    /// `content/careers/*.ron` depends on these field names.
    #[test]
    fn career_serialized_form_is_locked() {
        use crate::career::{
            CareerPath, CareerPerk, CareerRank, PathType, PerkType, ProgressionCriterion,
            ProgressionCriterionType, ProgressionRequirement,
        };
        use crate::util::Fixed;

        let file = ContentFile {
            id: "compact_navy".into(),
            display_name: "Compact Navy".into(),
            asset_type: AssetType::Career,
            seed: 42,
            universe: "all".into(),
            priority: Priority::Authoritative,
            expires_at: None,
            payload: ContentPayload::Career(Box::new(CareerPath {
                id: "compact_navy".into(),
                path_type: PathType::Military,
                name: "Compact Navy".into(),
                description: "Serve in the Compact Fleet.".into(),
                faction_id: Some("compact".into()),
                ranks: vec![CareerRank {
                    rank: 1,
                    title: "Ensign".into(),
                    required_criteria: vec![ProgressionRequirement {
                        criterion_type: ProgressionCriterionType::CombatVictories,
                        threshold: 3,
                    }],
                    rank_perks: vec!["nav_boost".into()],
                    faction_standing_bonus: 10,
                }],
                progression_criteria: vec![ProgressionCriterion {
                    criterion_type: ProgressionCriterionType::CombatVictories,
                    target: "*".into(),
                    threshold: 10,
                    weight: Fixed::from_int(1),
                }],
                perks: vec![CareerPerk {
                    id: "nav_boost".into(),
                    name: "Nav Boost".into(),
                    description: "Combat bonus".into(),
                    perk_type: PerkType::CombatBonus {
                        damage_type: "kinetic".into(),
                        pct: Fixed::from_int(5),
                    },
                    magnitude: Fixed::from_int(5),
                }],
                conflicting_paths: vec!["reach_pirates".into()],
            })),
        };
        let text = ron::to_string(&file).unwrap();
        for field in [
            "career",
            "path_type",
            "progression_criteria",
            "conflicting_paths",
            "combat_bonus",
        ] {
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

    /// S79: origin payloads lock their serialized form — the wire shape is
    /// pinned (iron rule #4). This test proves a full Loup-Garou veteran
    /// origin round-trips through RON.
    #[test]
    fn origin_envelope_round_trip() {
        use crate::content::origin::{
            CrewAssignment, FactionStandingDelta, ItemStack, LogEntryDraft, Origin,
        };
        use crate::seed::Seed;

        let origin = Origin {
            id: "loup_garou_veteran".into(),
            name: "Loup-Garou Veteran".into(),
            description: "Captain of the Loup-Garou, a crew of misfits.".into(),
            icon: "compact_military".into(),
            starting_career: "freelance".into(),
            starting_rank: 1,
            faction_deltas: vec![FactionStandingDelta {
                faction_id: "compact".into(),
                delta: 10,
            }],
            starting_credits: 5000,
            ship_template: Some("loup_garou".into()),
            ship_seed: Some(Seed::new(16843009)),
            starting_gear: vec![ItemStack {
                item_id: "pistol".into(),
                count: 1,
            }],
            starting_crew: vec![CrewAssignment::Authored {
                soul_id: "tove".into(),
                role: "pilot".into(),
            }],
            known_systems: vec![],
            start_system: Seed::new(16843009),
            start_location: "Aethon Station".into(),
            opening_log_entries: vec![LogEntryDraft {
                title: "Last Job".into(),
                body: "It was a simple cargo run.".into(),
                tick_offset: 0,
            }],
        };
        let file = ContentFile {
            id: "loup_garou_veteran".into(),
            display_name: "Loup-Garou Veteran".into(),
            asset_type: AssetType::Origin,
            seed: 16843009,
            universe: "all".into(),
            priority: Priority::Authoritative,
            expires_at: None,
            payload: ContentPayload::Origin(origin.clone()),
        };
        let text = ron::ser::to_string(&file).unwrap();
        assert!(
            text.contains("origin("),
            "serialized form must include origin variant: {text}"
        );
        let back: ContentFile = ron::de::from_str(&text).unwrap();
        assert_eq!(file, back);
    }
}
