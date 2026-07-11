//! Cross-platform determinism manifest (spec §5, adversarial finding #3).
//!
//! `manifest()` runs every generator over a canonical seed set and hashes
//! the outputs. CI builds this on x86_64, aarch64, and wasm32 and compares
//! the manifests bit-for-bit — any divergence fails the merge.

use serde::{Deserialize, Serialize};

use crate::generator;
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
    }

    Manifest {
        version: 1,
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
        b.entries[3].checksum ^= 1;
        let problems = diff(&a, &b);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("music") || problems[0].contains("hull"));
    }
}
