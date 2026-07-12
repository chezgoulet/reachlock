//! `reachlock gen …` — run generators from the CLI, print a summary, and
//! optionally export previewable files (SVG for meshes/layouts, WAV for
//! music, PPM for textures). No Bevy dependency: these are the core's plain
//! data structures serialized into ancient, dependency-free formats.

use clap::Subcommand;
use reachlock_core::generator::{
    self, hull::HullClass, station::StationKind, ui::PanelType, GeneratedLayout, GeneratedMesh,
    Mood,
};
use reachlock_core::seed::types::Biome;
use std::fmt::Write as _;

#[derive(Subcommand)]
pub enum GenCommand {
    /// Generate a ship hull.
    Hull {
        #[arg(long)]
        seed: u64,
        #[arg(long, default_value = "corvette")]
        class: String,
        /// Write an SVG preview to this path.
        #[arg(long)]
        svg: Option<std::path::PathBuf>,
    },
    /// Generate a station (exterior + interior layout).
    Station {
        #[arg(long)]
        seed: u64,
        #[arg(long, default_value = "trade")]
        kind: String,
        #[arg(long, default_value_t = 2)]
        size: u32,
        /// Write an SVG floor-plan preview to this path.
        #[arg(long)]
        svg: Option<std::path::PathBuf>,
    },
    /// Generate a planet surface texture.
    Planet {
        #[arg(long)]
        seed: u64,
        #[arg(long, default_value = "frontier")]
        biome: String,
        /// Write a PPM image of the surface to this path.
        #[arg(long)]
        ppm: Option<std::path::PathBuf>,
    },
    /// Generate a music phrase.
    Music {
        #[arg(long)]
        seed: u64,
        #[arg(long, default_value = "calm")]
        mood: String,
        #[arg(long, default_value_t = 4)]
        seconds: u32,
        /// Write a WAV file to this path.
        #[arg(long)]
        wav: Option<std::path::PathBuf>,
    },
    /// Generate a UI panel layout.
    UiPanel {
        #[arg(long)]
        seed: u64,
    },
    /// Generate a whole star system (star, orbits, stations, gate).
    System {
        #[arg(long)]
        seed: u64,
        #[arg(long, default_value = "frontier")]
        biome: String,
        /// Fidelity: `full` or `sparse` (deep-space trim, spec §17).
        #[arg(long, default_value = "full")]
        fidelity: String,
        /// Write an SVG map of the system to this path.
        #[arg(long)]
        svg: Option<std::path::PathBuf>,
    },
    /// Generate a named, stat-banded item with a procedural icon.
    Item {
        #[arg(long)]
        seed: u64,
        /// Item family token, e.g. `kinetic_weapon`, `shield`, `engine`.
        #[arg(long, default_value = "kinetic_weapon")]
        family: String,
        #[arg(long, default_value_t = 4)]
        tier: u8,
        #[arg(long, default_value = "compact")]
        faction: String,
        #[arg(long, default_value = "frontier")]
        biome: String,
        /// Write a PPM image of the icon to this path.
        #[arg(long)]
        ppm: Option<std::path::PathBuf>,
    },
}

pub fn run(cmd: GenCommand) -> Result<(), String> {
    match cmd {
        GenCommand::Hull { seed, class, svg } => {
            let class = parse_class(&class)?;
            let mesh = generator::hull::generate_hull_class(seed, class);
            println!(
                "hull seed={seed:#x} class={class:?}: {} vertices, {} triangles",
                mesh.vertices.len(),
                mesh.indices.len() / 3
            );
            if let Some(path) = svg {
                write(&path, mesh_svg(&mesh))?;
                println!("wrote {}", path.display());
            }
            Ok(())
        }
        GenCommand::Station {
            seed,
            kind,
            size,
            svg,
        } => {
            let kind = parse_station_kind(&kind)?;
            let station = generator::generate_station(seed, kind, size);
            println!(
                "station seed={seed:#x} kind={kind:?} size={size}: {} rooms, {} doors",
                station.layout.rooms.len(),
                station.layout.doors.len()
            );
            for (i, room) in station.layout.rooms.iter().enumerate() {
                println!(
                    "  [{i}] {:?} at ({}, {}) {}x{}",
                    room.kind, room.x, room.y, room.width, room.height
                );
            }
            if let Some(path) = svg {
                write(&path, layout_svg(&station.layout))?;
                println!("wrote {}", path.display());
            }
            Ok(())
        }
        GenCommand::Planet { seed, biome, ppm } => {
            let biome = parse_biome(&biome)?;
            let planet = generator::generate_planet(seed, 100, biome);
            println!(
                "planet seed={seed:#x} biome={biome:?}: {}x{} surface",
                planet.surface.width, planet.surface.height
            );
            if let Some(path) = ppm {
                write(&path, texture_ppm(&planet.surface))?;
                println!("wrote {}", path.display());
            }
            Ok(())
        }
        GenCommand::Music {
            seed,
            mood,
            seconds,
            wav,
        } => {
            let mood = parse_mood(&mood)?;
            let audio = generator::generate_music(seed, mood, seconds);
            println!(
                "music seed={seed:#x} mood={mood:?}: {} samples at {} Hz",
                audio.samples.len(),
                audio.sample_rate
            );
            if let Some(path) = wav {
                std::fs::write(&path, generator::music::to_wav_bytes(&audio))
                    .map_err(|e| format!("writing {}: {e}", path.display()))?;
                println!("wrote {}", path.display());
            }
            Ok(())
        }
        GenCommand::UiPanel { seed } => {
            let layout = generator::generate_ui_panel(seed, PanelType::StationServices, 320, 240);
            println!("ui_panel seed={seed:#x}: {} regions", layout.rooms.len());
            for room in &layout.rooms {
                println!("  band at y={} height={}", room.y, room.height);
            }
            Ok(())
        }
        GenCommand::System {
            seed,
            biome,
            fidelity,
            svg,
        } => {
            let biome = parse_biome(&biome)?;
            let fidelity = parse_fidelity(&fidelity)?;
            let system = generator::system::generate_system(seed, biome, fidelity);
            println!(
                "system seed={seed:#x} biome={biome:?} fidelity={fidelity:?}: \
                 star {:?}, {} orbits, {} asteroid fields, {} stations, threat {}",
                system.star.class,
                system.orbits.len(),
                system.asteroid_fields.len(),
                system.station_slots.len(),
                system.threat_level,
            );
            for (i, orbit) in system.orbits.iter().enumerate() {
                println!(
                    "  orbit[{i}] r={} body={} {:?} seed={:#x}",
                    orbit.radius, orbit.planet_radius, orbit.biome, orbit.seed
                );
            }
            for (i, slot) in system.station_slots.iter().enumerate() {
                println!(
                    "  station[{i}] {:?} at ({}, {})",
                    slot.kind, slot.position.x.0, slot.position.y.0
                );
            }
            println!(
                "  gate at ({}, {})",
                system.gate_position.x.0, system.gate_position.y.0
            );
            if let Some(path) = svg {
                write(&path, system_svg(&system))?;
                println!("wrote {}", path.display());
            }
            Ok(())
        }
        GenCommand::Item {
            seed,
            family,
            tier,
            faction,
            biome,
            ppm,
        } => {
            let family = parse_item_family(&family)?;
            let item_seed = reachlock_core::item::ItemSeed {
                seed,
                item_type: family.representative_item_type(),
                tier,
                faction,
                biome,
            };
            let item = reachlock_core::item::generate_item(&item_seed);
            println!(
                "item seed={seed:#x} family={} tier={tier}: {:?} \"{}\"",
                family.token(),
                item.rarity,
                item.display_name
            );
            println!("  id={}", item.id);
            println!("  {}", item.description);
            for (key, value) in &item.stats.0 {
                // Stats are fixed-point (1 unit = 1/1024); show the human value.
                println!(
                    "  {key:?}: {}.{:03}",
                    value / 1024,
                    (value.abs() % 1024) * 1000 / 1024
                );
            }
            if let Some(path) = ppm {
                write(&path, texture_ppm(&item.icon))?;
                println!("wrote {}", path.display());
            }
            Ok(())
        }
    }
}

fn parse_class(s: &str) -> Result<HullClass, String> {
    match s {
        "shuttle" => Ok(HullClass::Shuttle),
        "freighter" => Ok(HullClass::Freighter),
        "corvette" => Ok(HullClass::Corvette),
        "station" => Ok(HullClass::Station),
        other => Err(format!("unknown hull class: {other}")),
    }
}

fn parse_station_kind(s: &str) -> Result<StationKind, String> {
    match s {
        "trade" => Ok(StationKind::Trade),
        "mining" => Ok(StationKind::Mining),
        "military" => Ok(StationKind::Military),
        other => Err(format!("unknown station kind: {other}")),
    }
}

fn parse_biome(s: &str) -> Result<Biome, String> {
    match s {
        "core" => Ok(Biome::Core),
        "frontier" => Ok(Biome::Frontier),
        "nebula" => Ok(Biome::Nebula),
        "derelict" => Ok(Biome::Derelict),
        "deep_space" => Ok(Biome::DeepSpace),
        other => Err(format!("unknown biome: {other}")),
    }
}

fn parse_mood(s: &str) -> Result<Mood, String> {
    match s {
        "calm" => Ok(Mood::Calm),
        "tense" => Ok(Mood::Tense),
        "derelict" => Ok(Mood::Derelict),
        other => Err(format!("unknown mood: {other}")),
    }
}

fn parse_fidelity(s: &str) -> Result<generator::system::Fidelity, String> {
    match s {
        "full" => Ok(generator::system::Fidelity::Full),
        "sparse" => Ok(generator::system::Fidelity::Sparse),
        other => Err(format!("unknown fidelity: {other} (want full|sparse)")),
    }
}

fn parse_item_family(s: &str) -> Result<reachlock_core::item::ItemFamily, String> {
    reachlock_core::item::ItemFamily::ALL
        .into_iter()
        .find(|f| f.token() == s)
        .ok_or_else(|| format!("unknown item family: {s}"))
}

fn write(path: &std::path::Path, content: String) -> Result<(), String> {
    std::fs::write(path, content).map_err(|e| format!("writing {}: {e}", path.display()))
}

pub(crate) fn mesh_svg(mesh: &GeneratedMesh) -> String {
    let mut min = (i64::MAX, i64::MAX);
    let mut max = (i64::MIN, i64::MIN);
    for v in &mesh.vertices {
        min = (min.0.min(v.x.0), min.1.min(v.y.0));
        max = (max.0.max(v.x.0), max.1.max(v.y.0));
    }
    let pad = 2048;
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{} {} {} {}">"#,
        min.0 - pad,
        min.1 - pad,
        max.0 - min.0 + 2 * pad,
        max.1 - min.1 + 2 * pad
    );
    // Outline: vertices 1.. are the rim (0 is the fan center).
    let mut d = String::from("M");
    for v in &mesh.vertices[1..] {
        let _ = write!(d, " {} {}", v.x.0, v.y.0);
    }
    let _ = write!(
        svg,
        r##"<path d="{d} Z" fill="#345" stroke="#9cf" stroke-width="512"/></svg>"##
    );
    svg
}

pub(crate) fn layout_svg(layout: &GeneratedLayout) -> String {
    let mut min = (i32::MAX, i32::MAX);
    let mut max = (i32::MIN, i32::MIN);
    for r in &layout.rooms {
        min = (min.0.min(r.x), min.1.min(r.y));
        max = (max.0.max(r.x + r.width), max.1.max(r.y + r.height));
    }
    let pad = 8;
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{} {} {} {}">"#,
        min.0 - pad,
        min.1 - pad,
        max.0 - min.0 + 2 * pad,
        max.1 - min.1 + 2 * pad
    );
    for room in &layout.rooms {
        let _ = write!(
            svg,
            r##"<rect x="{}" y="{}" width="{}" height="{}" fill="#234" stroke="#8ac" stroke-width="1"/>"##,
            room.x, room.y, room.width, room.height
        );
        let _ = write!(
            svg,
            r##"<text x="{}" y="{}" font-size="6" fill="#cde">{:?}</text>"##,
            room.x + 2,
            room.y + 8,
            room.kind
        );
    }
    for door in &layout.doors {
        let _ = write!(
            svg,
            r##"<circle cx="{}" cy="{}" r="2" fill="#fc6"/>"##,
            door.x, door.y
        );
    }
    svg.push_str("</svg>");
    svg
}

/// A top-down SVG map of a system. The star sits at the origin; positions
/// are fixed-point (1 unit = 1/1024 world units), orbit radii are whole
/// world units, so radii are scaled by 1024 to share the coordinate space.
fn system_svg(system: &reachlock_core::generator::system::GeneratedSystem) -> String {
    const FIXED: i64 = 1024;
    let mut reach = FIXED; // never zero, so the viewBox is always valid
    for o in &system.orbits {
        reach = reach.max(o.position.x.0.abs()).max(o.position.y.0.abs());
        reach = reach.max(o.radius * FIXED);
    }
    for s in &system.station_slots {
        reach = reach.max(s.position.x.0.abs()).max(s.position.y.0.abs());
    }
    for f in &system.asteroid_fields {
        reach = reach.max(f.center.x.0.abs()).max(f.center.y.0.abs());
    }
    reach = reach
        .max(system.gate_position.x.0.abs())
        .max(system.gate_position.y.0.abs());

    let span = reach * 2 + 2 * FIXED;
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{} {} {span} {span}">"#,
        -reach - FIXED,
        -reach - FIXED,
    );
    // Orbit rings.
    for o in &system.orbits {
        let _ = write!(
            svg,
            r##"<circle cx="0" cy="0" r="{}" fill="none" stroke="#234" stroke-width="20"/>"##,
            o.radius * FIXED
        );
    }
    // Asteroid fields.
    for f in &system.asteroid_fields {
        let _ = write!(
            svg,
            r##"<circle cx="{}" cy="{}" r="{}" fill="#3a3320" fill-opacity="0.4" stroke="#665" stroke-width="12"/>"##,
            f.center.x.0,
            f.center.y.0,
            f.radius * FIXED
        );
    }
    // Planets.
    for o in &system.orbits {
        let _ = write!(
            svg,
            r##"<circle cx="{}" cy="{}" r="{}" fill="#69c"/>"##,
            o.position.x.0,
            o.position.y.0,
            (o.planet_radius * FIXED).max(120)
        );
    }
    // Stations (diamonds).
    for s in &system.station_slots {
        let (x, y) = (s.position.x.0, s.position.y.0);
        let _ = write!(
            svg,
            r##"<rect x="{}" y="{}" width="240" height="240" transform="rotate(45 {x} {y})" fill="#fc6"/>"##,
            x - 120,
            y - 120,
        );
    }
    // Gate (magenta ring) and star (center).
    let _ = write!(
        svg,
        r##"<circle cx="{}" cy="{}" r="180" fill="none" stroke="#c6f" stroke-width="40"/>"##,
        system.gate_position.x.0, system.gate_position.y.0
    );
    let _ = write!(
        svg,
        r##"<circle cx="0" cy="0" r="360" fill="#ffd"/></svg>"##
    );
    svg
}

pub(crate) fn texture_ppm(tex: &reachlock_core::generator::GeneratedTexture) -> String {
    let mut ppm = format!("P3\n{} {}\n255\n", tex.width, tex.height);
    for px in tex.pixels.chunks_exact(4) {
        let _ = writeln!(ppm, "{} {} {}", px[0], px[1], px[2]);
    }
    ppm
}
