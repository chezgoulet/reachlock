//! Cross-platform determinism manifest (spec §5, adversarial finding #3).
//!
//! `manifest()` runs every generator over a canonical seed set and hashes
//! the outputs. CI builds this on x86_64, aarch64, and wasm32 and compares
//! the manifests bit-for-bit — any divergence fails the merge.

use serde::{Deserialize, Serialize};

use crate::generator;
use crate::item;
use crate::seed::types::Biome;
use crate::util::{color, noise};

/// The canonical seed battery. Edge values on purpose.
pub const CANONICAL_SEEDS: [u64; 6] = [
    0,
    1,
    42,
    0xDEAD_BEEF,
    Seed53_MAX,
    7_928_794_229_254_937, // the seed-resolver golden
];
#[allow(non_upper_case_globals)]
const Seed53_MAX: u64 = (1 << 53) - 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub generator: String,
    pub seed: u64,
    pub checksum: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Format version: bump when adding generators so old manifests don't
    /// false-negative against new binaries.
    pub version: u32,
    pub entries: Vec<Entry>,
}

/// FNV-1a running hasher for output canonicalization.
struct Hasher(u64);

impl Hasher {
    fn new() -> Self {
        Hasher(0xCBF2_9CE4_8422_2325)
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01B3);
        }
    }
    fn write_i64(&mut self, v: i64) {
        self.write(&v.to_le_bytes());
    }
    fn finish(self) -> u64 {
        self.0
    }
}

/// Hash any serializable generator output by its canonical JSON encoding.
/// Deterministic because these generators use only integer/fixed-point
/// values (no floats) and BTreeMap-backed maps, so serde_json emits
/// byte-identical output on every target.
fn hash_serde<T: Serialize>(value: &T) -> u64 {
    let bytes = serde_json::to_vec(value).expect("generator output serializes");
    let mut h = Hasher::new();
    h.write(&bytes);
    h.finish()
}

fn hash_mesh(mesh: &generator::GeneratedMesh) -> u64 {
    let mut h = Hasher::new();
    for v in &mesh.vertices {
        h.write_i64(v.x.0);
        h.write_i64(v.y.0);
    }
    for &i in &mesh.indices {
        h.write(&i.to_le_bytes());
    }
    h.finish()
}

fn hash_layout(layout: &generator::GeneratedLayout) -> u64 {
    let mut h = Hasher::new();
    for room in &layout.rooms {
        h.write(&[room.kind as u8]);
        for v in [room.x, room.y, room.width, room.height] {
            h.write(&v.to_le_bytes());
        }
    }
    for door in &layout.doors {
        h.write(&door.from.to_le_bytes());
        h.write(&door.to.to_le_bytes());
        h.write(&door.x.to_le_bytes());
        h.write(&door.y.to_le_bytes());
    }
    h.finish()
}

pub fn manifest() -> Manifest {
    let mut entries = Vec::new();

    for &seed in &CANONICAL_SEEDS {
        entries.push(Entry {
            generator: "hull".into(),
            seed,
            checksum: hash_mesh(&generator::generate_hull(seed)),
        });

        let station = generator::generate_station(seed, generator::station::StationKind::Trade, 2);
        let mut h = Hasher::new();
        h.write_i64(hash_mesh(&station.exterior) as i64);
        h.write_i64(hash_layout(&station.layout) as i64);
        entries.push(Entry {
            generator: "station".into(),
            seed,
            checksum: h.finish(),
        });

        // S06 — ship interior layout (the On-Board scene's one source of
        // truth). Corvette is the player's default hull class.
        entries.push(Entry {
            generator: "hull_interior".into(),
            seed,
            checksum: hash_layout(&generator::station::generate_hull_interior(
                seed,
                generator::hull::HullClass::Corvette,
            )),
        });

        let planet = generator::generate_planet(seed, 100, Biome::Frontier);
        let mut h = Hasher::new();
        h.write_i64(hash_mesh(&planet.disc) as i64);
        h.write(&planet.surface.pixels);
        entries.push(Entry {
            generator: "planet".into(),
            seed,
            checksum: h.finish(),
        });

        let audio = generator::generate_music(seed, generator::Mood::Tense, 1);
        let mut h = Hasher::new();
        for s in &audio.samples {
            h.write(&s.to_le_bytes());
        }
        entries.push(Entry {
            generator: "music".into(),
            seed,
            checksum: h.finish(),
        });

        entries.push(Entry {
            generator: "ui_panel".into(),
            seed,
            checksum: hash_layout(&generator::generate_ui_panel(
                seed,
                generator::ui::PanelType::StationServices,
                320,
                240,
            )),
        });

        let mut h = Hasher::new();
        for i in 0..64i64 {
            h.write(&noise::fbm(seed, i * 97, i * 61, 4).to_le_bytes());
        }
        entries.push(Entry {
            generator: "noise".into(),
            seed,
            checksum: h.finish(),
        });

        let palette = color::generate_palette(seed);
        let mut h = Hasher::new();
        for c in [palette.primary, palette.accent, palette.structure] {
            h.write(&[c.r, c.g, c.b, c.a]);
        }
        entries.push(Entry {
            generator: "palette".into(),
            seed,
            checksum: h.finish(),
        });

        // S04 — whole-system generator, both fidelities.
        entries.push(Entry {
            generator: "system_full".into(),
            seed,
            checksum: hash_serde(&generator::system::generate_system(
                seed,
                Biome::Frontier,
                generator::system::Fidelity::Full,
            )),
        });
        entries.push(Entry {
            generator: "system_sparse".into(),
            seed,
            checksum: hash_serde(&generator::system::generate_system(
                seed,
                Biome::DeepSpace,
                generator::system::Fidelity::Sparse,
            )),
        });

        // S05 — item generator (representative family; the icon texture is
        // part of the hashed output).
        entries.push(Entry {
            generator: "item_kinetic".into(),
            seed,
            checksum: hash_serde(&item::generate_item(&item::ItemSeed {
                seed,
                item_type: item::ItemFamily::KineticWeapon.representative_item_type(),
                tier: 4,
                faction: "compact".into(),
                biome: "frontier".into(),
            })),
        });
        // S17 — exterior composition over a fixture config: reference
        // corvette frame, one kinetic hardpoint, a tier-4 engine, plated
        // nose. Hashes the composed mesh, the resolved paint, and the
        // derived handling so drift in any of the three is caught.
        {
            use crate::editor::exterior;
            let frame = exterior::HullFrame::reference(generator::hull::HullClass::Corvette);
            let item_ref = |item_type: item::ItemType, tier: u8| {
                exterior::ItemRef(item::ItemSeed {
                    seed: seed ^ 0x17,
                    item_type,
                    tier,
                    faction: "compact".into(),
                    biome: "frontier".into(),
                })
            };
            let config = exterior::HullConfiguration {
                hull_id: "frame_corvette".into(),
                seed,
                hardpoints: vec![exterior::Hardpoint {
                    slot_id: "nose".into(),
                    item: item_ref(
                        item::ItemFamily::KineticWeapon.representative_item_type(),
                        3,
                    ),
                    size_class: exterior::SizeClass::Small,
                }],
                engine: item_ref(item::ItemType::Equipment(item::EquipmentKind::Engine), 4),
                plating: vec![exterior::ArmorSegment {
                    zone_id: "nose".into(),
                    mass: 8 * 1024,
                }],
                paint: exterior::PaintScheme::default(),
                decals: vec![],
            };
            let composed = exterior::compose_hull(&config, &frame);
            let h = exterior::handling(&config, &frame);
            let mut hasher = Hasher::new();
            hasher.write_i64(hash_mesh(&composed.mesh) as i64);
            for c in [
                composed.paint.primary,
                composed.paint.secondary,
                composed.paint.accent,
            ] {
                hasher.write(&[c.r, c.g, c.b, c.a]);
            }
            for v in [
                h.mass,
                h.thrust,
                h.turn_rate,
                h.drift_damping,
                h.boost_mult,
                h.fuel_burn,
            ] {
                hasher.write_i64(v);
            }
            entries.push(Entry {
                generator: "hull_config".into(),
                seed,
                checksum: hasher.finish(),
            });
        }

        // S18 — interior realization over a fixture placement: the six-room
        // corvette layout with one corridor, realized through the reference
        // template set. Hashes the walkable GeneratedLayout so drift in the
        // door pass, corridor legs, or cell math is caught cross-platform.
        {
            use crate::editor::interior;
            let place = |template_id: &str, x: u8, y: u8| interior::PlacedRoom {
                template_id: template_id.into(),
                position: (x, y),
                rotation: 0,
            };
            let layout = interior::ShipInteriorLayout {
                hull_id: "frame_corvette".into(),
                rooms: vec![
                    place("airlock", 0, 0),
                    place("cockpit", 0, 2),
                    place("galley", 2, 0),
                    place("quarters", 3, 2),
                    place("engineering", 0, 6),
                    place("cargo_hold", 4, 6),
                ],
                corridors: vec![interior::Corridor {
                    from: (1, 4),
                    to: (1, 5),
                }],
                furniture: vec![interior::PlacedFurniture {
                    slot_id: "galley".into(),
                    room_idx: 2,
                    kind: interior::FurnitureKind::GalleyUnit,
                }],
                seed,
            };
            let bounds =
                crate::editor::exterior::HullFrame::reference(generator::hull::HullClass::Corvette)
                    .grid_bounds;
            let realized =
                interior::realize(&layout, &interior::RoomTemplate::reference_set(), bounds)
                    .expect("fixture layout realizes");
            entries.push(Entry {
                generator: "ship_interior".into(),
                seed,
                checksum: hash_layout(&realized),
            });
        }

        // S10 — economy engine. Hash the starter catalogue plus a seeded,
        // ticked `EconomyState` so any drift in price/tick math is caught
        // cross-platform (iron rule #3: new generator ⇒ golden entry).
        let catalog = crate::economy::starter_catalog();
        entries.push(Entry {
            generator: "economy_catalog".into(),
            seed,
            checksum: hash_serde(&catalog),
        });
        let station_seeds = vec![
            (
                "hub-1".to_string(),
                seed ^ 0x111,
                crate::economy::StationKind::Hub,
                None,
            ),
            (
                "ref-1".to_string(),
                seed ^ 0x222,
                crate::economy::StationKind::Refinery,
                None,
            ),
            (
                "bm-1".to_string(),
                seed ^ 0x333,
                crate::economy::StationKind::BlackMarket,
                None,
            ),
        ];
        let mut state = crate::economy::EconomyState::new(catalog, &station_seeds);
        for step in 0..8 {
            state.tick(seed.wrapping_add(step));
        }
        entries.push(Entry {
            generator: "economy_state".into(),
            seed,
            checksum: hash_serde(&state),
        });

        // S11 — faction engine. Hash a canonical catalog ticked forward (drift
        // + diplomacy) and a representative tariff quote so any change to the
        // faction/tariff math is caught cross-platform (iron rule #3).
        let catalog = crate::faction::FactionCatalog {
            version: 1,
            factions: vec![
                crate::faction::Faction {
                    id: crate::faction::FactionId("compact".into()),
                    name: "Compact".into(),
                    territory: vec![],
                    resources: crate::faction::FactionResources {
                        stock: std::collections::BTreeMap::new(),
                    },
                    relationships: {
                        let mut m = std::collections::BTreeMap::new();
                        m.insert(
                            crate::faction::FactionId("isc".into()),
                            crate::faction::DiplomaticStanding {
                                affinity: 100,
                                status_snapshot: crate::faction::RelationStatus::Allied,
                                treaty: None,
                                war_goal: None,
                            },
                        );
                        m
                    },
                    goals: vec![],
                    internal_divisions: vec![],
                    doctrine: crate::faction::Doctrine::Diplomatic,
                    tariff_policy: crate::faction::TariffPolicy::Regulated {
                        foreign_mult: 1229,
                        own_mult: 871,
                    },
                    produces: vec![],
                    color: [0x88, 0x88, 0x88, 0xFF],
                },
                crate::faction::Faction {
                    id: crate::faction::FactionId("isc".into()),
                    name: "ISC".into(),
                    territory: vec![],
                    resources: crate::faction::FactionResources {
                        stock: std::collections::BTreeMap::new(),
                    },
                    relationships: std::collections::BTreeMap::new(),
                    goals: vec![],
                    internal_divisions: vec![],
                    doctrine: crate::faction::Doctrine::Economic,
                    tariff_policy: crate::faction::TariffPolicy::Flat { mult: 1075 },
                    produces: vec![],
                    color: [0x88, 0x88, 0x88, 0xFF],
                },
            ],
        };
        let mut fstate = crate::faction::FactionState::new(catalog);
        for _ in 0..8 {
            fstate = crate::faction::tick_factions(fstate).0;
        }
        entries.push(Entry {
            generator: "faction_state".into(),
            seed,
            checksum: hash_serde(&fstate),
        });
        let tariff = crate::faction::tariff(
            &fstate.catalog.factions[0],
            crate::economy::GoodCategory::Consumable,
            50 * crate::economy::TARIFF_ONE,
            1024,
        );
        entries.push(Entry {
            generator: "faction_tariff".into(),
            seed,
            checksum: tariff as u64,
        });
        // evaluate_storylines golden: hash the fired chapter IDs for a
        // canonical storyline (Compact arc) at a fixed tick.
        let canonical_stories = vec![crate::faction::Storyline {
            faction: crate::faction::FactionId("compact".into()),
            chapters: vec![
                crate::faction::Chapter {
                    id: "arc1".into(),
                    trigger: Some(crate::faction::ChapterTrigger::TickAfter(2)),
                    narration: "The Compact mobilizes.".into(),
                    events: vec![],
                },
                crate::faction::Chapter {
                    id: "arc2".into(),
                    trigger: Some(crate::faction::ChapterTrigger::ChapterComplete(
                        "arc1".into(),
                    )),
                    narration: "First contact established.".into(),
                    events: vec![],
                },
                crate::faction::Chapter {
                    id: "arc3".into(),
                    trigger: Some(crate::faction::ChapterTrigger::PlayerReputation {
                        faction: crate::faction::FactionId("compact".into()),
                        trust: 50 * crate::faction::REP_ONE,
                    }),
                    narration: "Trust earned.".into(),
                    events: vec![],
                },
            ],
        }];
        let fired = crate::faction::evaluate_storylines(&fstate, &canonical_stories);
        let mut h = crate::determinism::Hasher::new();
        for s in &fired {
            h.write(s.as_bytes());
        }
        entries.push(Entry {
            generator: "faction_storylines".into(),
            seed,
            checksum: h.finish(),
        });

        // S19 — combat. Encounters for the canonical Frontier system, the
        // per-class weapon rolls, and a scripted apply_hit exchange, so any
        // drift in spawn/damage math is caught cross-platform.
        let sys = generator::system::generate_system(
            seed,
            Biome::Frontier,
            generator::system::Fidelity::Full,
        );
        entries.push(Entry {
            generator: "combat_encounters".into(),
            seed,
            checksum: hash_serde(&crate::combat::generate_encounters(seed, &sys)),
        });
        let mut vessel = crate::combat::EnemyClass::Bomber.vessel(6);
        let gun = crate::combat::EnemyClass::Interceptor.weapon(seed, 6);
        let mut results = Vec::new();
        for target in [
            None,
            Some(crate::combat::SubsystemKind::Engines),
            Some(crate::combat::SubsystemKind::Drive),
        ] {
            for _ in 0..4 {
                results.push(crate::combat::apply_hit(&mut vessel, &gun, target));
            }
        }
        let mut h = Hasher::new();
        h.write_i64(hash_serde(&gun) as i64);
        h.write_i64(hash_serde(&vessel) as i64);
        h.write_i64(hash_serde(&results) as i64);
        entries.push(Entry {
            generator: "combat_damage".into(),
            seed,
            checksum: h.finish(),
        });

        // S21: deep_space_seed is frozen protocol (joining derive_seed).
        // Test three distinct coords per seed so any FNV-1a derivation drift
        // across platforms is caught.
        for coord in &[
            crate::galaxy::GalaxyCoord { x: 0, y: 0, z: 0 },
            crate::galaxy::GalaxyCoord { x: 1000, y: 2000, z: 3000 },
            crate::galaxy::GalaxyCoord { x: -500, y: 8000, z: -12000 },
        ] {
            entries.push(Entry {
                generator: format!("deep_space_seed_{}_{}_{}", coord.x, coord.y, coord.z),
                seed,
                checksum: crate::galaxy::deep_space_seed(
                    *coord,
                    crate::universe::tier::UniverseTier::Classic,
                )
                .value(),
            });
        }

        // S20 — landed (humanoid) combat. Drive `humanoid_step` through a
        // fixed engagement (Idle → Chase → repeated swings) with a
        // seed-derived patrol, capturing (state, intent) at ticks 0/10/20/
        // 50/100 so any drift in the state machine or its tick math is caught
        // cross-platform. Same SeededRng pattern the behavior tree uses.
        {
            use crate::combat::humanoid::{
                humanoid_step, AttackWindow, BlockWindow, DodgeWindow, HostileArchetype,
                HumanoidSenses, HumanoidState,
            };
            let mut rng = crate::util::rng::SeededRng::new(seed ^ 0x5A5A_1234);
            let mut waypoints = [(0i64, 0i64); 4];
            for wp in &mut waypoints {
                *wp = (
                    (rng.next_below(20_000) as i64) - 10_000,
                    (rng.next_below(20_000) as i64) - 10_000,
                );
            }
            let win = |s: u32, a: u32, r: u32, d: i64, rng: i64| AttackWindow {
                startup_ticks: s,
                active_ticks: a,
                recovery_ticks: r,
                damage: d,
                range: rng,
            };
            let arch = HostileArchetype {
                id: "manifest_raider".into(),
                display_name: "Manifest Raider".into(),
                hp: 8192,
                speed: 256,
                light_attack: win(8, 4, 12, 1024, 2048),
                heavy_attack: win(16, 6, 20, 2048, 2560),
                block: BlockWindow {
                    active_ticks: 20,
                    cooldown_ticks: 30,
                    parry_ticks: 4,
                },
                dodge: DodgeWindow {
                    i_frame_ticks: 8,
                    recovery_ticks: 12,
                    distance: 3072,
                },
                chase_radius: 8192,
                disengage_radius: 16000,
                flee_hp_frac: 256,
            };
            let senses = HumanoidSenses {
                to_target: crate::generator::FixedVec2 {
                    x: crate::util::rng::Fixed(3 * 1024),
                    y: crate::util::rng::Fixed(2 * 1024),
                },
                dist_to_target: 3600,
                hp_frac: 1024,
                weapon_ready: true,
                target_in_range: true,
                target_telegraphing: false,
                under_attack: false,
                ally_count: 1,
                patrol_waypoints: waypoints,
                waypoint_index: (seed % 4) as u32,
            };
            let mut state = HumanoidState::Idle;
            let mut timer = 0u32;
            let mut captures = Vec::new();
            for tick in 0..=100u32 {
                let intent = humanoid_step(&mut state, &mut timer, &senses, &arch);
                if matches!(tick, 0 | 10 | 20 | 50 | 100) {
                    captures.push((state, intent));
                }
            }
            entries.push(Entry {
                generator: "combat_humanoid".into(),
                seed,
                checksum: hash_serde(&captures),
            });
        }
    }

    Manifest {
        // v3: added S06 hull_interior (ship interior layout) generator.
        // v4: added S10 economy engine golden entries.
        // v5: added S11 faction engine golden entries.
        // v6: added S19 combat golden entries (encounters + damage model).
        // v7: added S17 hull_config (exterior composition) golden entry.
        //     (S17 and S19 both bumped 5->6 on their branches; the merge
        //     carries both entry sets, so the merged manifest is v7.)
        // v8: added S18 ship_interior (interior realization) golden entry.
        // v9: added S21 deep_space_seed (frozen protocol) and S20 combat_humanoid.
        // v10: added S20 hostile_locations to GeneratedSystem (system POIs).
        version: 10,
        entries,
    }
}

/// Compare two manifests, returning human-readable mismatch lines.
pub fn diff(ours: &Manifest, theirs: &Manifest) -> Vec<String> {
    let mut problems = Vec::new();
    if ours.version != theirs.version {
        problems.push(format!(
            "manifest version mismatch: {} vs {}",
            ours.version, theirs.version
        ));
        return problems;
    }
    if ours.entries.len() != theirs.entries.len() {
        problems.push(format!(
            "entry count mismatch: {} vs {}",
            ours.entries.len(),
            theirs.entries.len()
        ));
    }
    for (a, b) in ours.entries.iter().zip(&theirs.entries) {
        if a != b {
            problems.push(format!(
                "{}(seed {:#x}): {:#018x} vs {:#018x}",
                a.generator, a.seed, a.checksum, b.checksum
            ));
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_is_stable_within_a_run() {
        assert_eq!(manifest(), manifest());
    }

    #[test]
    fn diff_reports_divergence() {
        let a = manifest();
        let mut b = manifest();
        // Flip a stable generator (music sits at a fixed index regardless of
        // generators added before it).
        let music_idx = b
            .entries
            .iter()
            .position(|e| e.generator == "music")
            .expect("music entry present");
        b.entries[music_idx].checksum ^= 1;
        let problems = diff(&a, &b);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("music"));
    }
}
