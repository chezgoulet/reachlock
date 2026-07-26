//! Vision: show the model what it authored (S101 P6).
//!
//! **No screen capture.** `reachlock_core::generator::sprite` returns
//! `GeneratedTexture` — plain RGBA bytes — so a preview is composited and PNG-
//! encoded in-process. That matters practically as much as architecturally:
//! screen capture is unavailable on this machine (Wayland; x11grab and the
//! GNOME portal both fail), so any screenshot-based design would not run at
//! all. It is also more deterministic than a screenshot, and works headless.
//!
//! **No new generator.** This composites the output of existing core
//! generators exactly as `editors/character_sprite.rs` already does for the
//! preview tab, so there is no new seeded pipeline and the determinism
//! manifest is untouched.
//!
//! **Degrades instead of failing.** A text-only model gets a written
//! description of the render rather than an error, so the same tool is usable
//! from a small local model.

use reachlock_core::generator::sprite::{
    generate_character_sprite, CharacterLookConfig, CharacterSprite,
};
use reachlock_core::generator::GeneratedTexture;
use reachlock_core::soul::types::Species;
use serde_json::{json, Value};

use super::{Mutability, Tool, ToolCtx, ToolOutcome};

/// Flatten the sprite's three layers, bottom-up, the same way the sprite
/// viewer tab does.
fn composite(sprite: &CharacterSprite) -> GeneratedTexture {
    let mut out = sprite.body_layer.clone();
    for layer in [&sprite.outfit_layer, &sprite.hair_layer] {
        for i in (0..out.pixels.len()).step_by(4) {
            if layer.pixels[i + 3] > 0 {
                out.pixels[i..i + 4].copy_from_slice(&layer.pixels[i..i + 4]);
            }
        }
    }
    out
}

/// RGBA8 to PNG bytes.
fn encode_png(tex: &GeneratedTexture) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut buf, tex.width, tex.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| format!("could not start PNG: {e}"))?;
        writer
            .write_image_data(&tex.pixels)
            .map_err(|e| format!("could not write PNG: {e}"))?;
    }
    Ok(buf)
}

/// What a text-only model gets instead of the image.
///
/// Deliberately about colour and coverage rather than "a 32x48 sprite": the
/// numbers a model can act on when adjusting a look are which colours dominate
/// and how much of the canvas the figure fills.
fn describe(tex: &GeneratedTexture, palette_key: &str) -> String {
    let mut opaque = 0usize;
    let (mut r, mut g, mut b) = (0u64, 0u64, 0u64);
    for px in tex.pixels.chunks_exact(4) {
        if px[3] > 0 {
            opaque += 1;
            r += px[0] as u64;
            g += px[1] as u64;
            b += px[2] as u64;
        }
    }
    let total = (tex.width * tex.height) as usize;
    if opaque == 0 {
        return format!(
            "{}x{} sprite, palette `{palette_key}` — the canvas is entirely transparent. \
             Nothing was drawn.",
            tex.width, tex.height
        );
    }
    format!(
        "{}x{} sprite, palette `{palette_key}`. The figure covers {}% of the canvas; \
         its average opaque colour is #{:02X}{:02X}{:02X}. \
         (This profile has no vision, so the image itself was not sent — tick \
         \"Supports images\" in AI Settings for a model that can see it.)",
        tex.width,
        tex.height,
        opaque * 100 / total.max(1),
        (r / opaque as u64) as u8,
        (g / opaque as u64) as u8,
        (b / opaque as u64) as u8,
    )
}

fn parse_species(label: &str) -> Option<Species> {
    match label {
        "human" => Some(Species::Human),
        "android" => Some(Species::Android),
        "robot" => Some(Species::Robot),
        "voidborn" => Some(Species::Voidborn),
        "xenotype" => Some(Species::Xenotype),
        _ => None,
    }
}

/// `[u8; 3]` from a `#RRGGBB` string. The model writes hex; the RON authored
/// form is a tuple, which is one of the traps worth keeping out of the
/// argument surface entirely.
fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let h = s.strip_prefix('#').unwrap_or(s);
    if h.len() != 6 {
        return None;
    }
    Some([
        u8::from_str_radix(&h[0..2], 16).ok()?,
        u8::from_str_radix(&h[2..4], 16).ok()?,
        u8::from_str_radix(&h[4..6], 16).ok()?,
    ])
}

fn color_arg(args: &Value, key: &str) -> Result<Option<[u8; 3]>, String> {
    match args.get(key).and_then(|v| v.as_str()) {
        None => Ok(None),
        Some(s) => parse_hex(s)
            .map(Some)
            .ok_or_else(|| format!("`{key}` must be a hex colour like #8FA060, got `{s}`")),
    }
}

pub fn tools() -> Vec<Tool> {
    vec![Tool {
        name: "render_character",
        description:
            "Render a character sprite from a look configuration and look at the result. Use \
             this after authoring or changing a soul's `look` block to check the appearance \
             reads at sprite size — whether the palette has enough contrast, whether the \
             figure is legible against its own outline. Colours are hex here; in the authored \
             RON they are tuples like (143, 160, 96).",
        input_schema: || {
            json!({
                "type": "object",
                "properties": {
                    "species": {
                        "type": "string",
                        "enum": ["human", "android", "robot", "voidborn", "xenotype"],
                    },
                    "seed": {
                        "type": "integer",
                        "minimum": 0,
                        "description": "Seed for anything the look leaves unset. Same seed plus same look always renders the same sprite.",
                    },
                    "hair_style": { "type": "integer", "minimum": 0, "description": "0 is bald." },
                    "skin_color": { "type": "string", "description": "#RRGGBB" },
                    "hair_color": { "type": "string", "description": "#RRGGBB" },
                    "shirt_color": { "type": "string", "description": "#RRGGBB" },
                    "pants_color": { "type": "string", "description": "#RRGGBB" },
                    "jacket_color": { "type": "string", "description": "#RRGGBB" },
                    "chassis_color": { "type": "string", "description": "#RRGGBB, robots only" },
                    "visor_color": { "type": "string", "description": "#RRGGBB, robots only" }
                },
                "required": ["species"],
                "additionalProperties": false,
            })
        },
        mutability: Mutability::ReadOnly,
        // Pure computation over core generators — no tabs, so this works in
        // the headless MCP server too.
        needs_session: false,
        run: run_render_character,
    }]
}

fn run_render_character(args: &Value, _ctx: &ToolCtx) -> ToolOutcome {
    let Some(species_label) = args.get("species").and_then(|v| v.as_str()) else {
        return ToolOutcome::error("`species` is required.");
    };
    let Some(species) = parse_species(species_label) else {
        return ToolOutcome::error(format!(
            "Unknown species `{species_label}`. One of: human, android, robot, voidborn, xenotype."
        ));
    };

    let mut config = CharacterLookConfig::seed_derived(species);
    // One explicit assignment per field. An earlier version walked a table of
    // raw pointers to shave repetition; unsafe code to save seven lines in a
    // content editor is a bad trade.
    for (key, value) in [
        ("skin_color", &mut config.skin_color),
        ("hair_color", &mut config.hair_color),
        ("shirt_color", &mut config.shirt_color),
        ("pants_color", &mut config.pants_color),
        ("chassis_color", &mut config.chassis_color),
        ("visor_color", &mut config.visor_color),
    ] {
        match color_arg(args, key) {
            Ok(parsed) => *value = parsed,
            Err(e) => return ToolOutcome::error(e),
        }
    }
    match color_arg(args, "jacket_color") {
        // A jacket colour with the jacket left disabled renders nothing, which
        // reads to the model as the colour being ignored.
        Ok(Some(c)) => {
            config.jacket_color = Some(c);
            config.jacket_enabled = Some(true);
        }
        Ok(None) => {}
        Err(e) => return ToolOutcome::error(e),
    }

    if let Some(style) = args.get("hair_style").and_then(|v| v.as_u64()) {
        config.hair_style = Some(style.min(u8::MAX as u64) as u8);
    }

    let seed = args.get("seed").and_then(|v| v.as_u64()).unwrap_or(42);
    let sprite = generate_character_sprite(seed, &config);
    let flat = composite(&sprite);

    let png = match encode_png(&flat) {
        Ok(p) => p,
        Err(e) => return ToolOutcome::error(e),
    };

    let mut outcome = ToolOutcome::ok(format!(
        "Rendered a {species_label} sprite at seed {seed} (palette `{}`, hair style {}).",
        sprite.palette_key, sprite.hair_style_index
    ));
    // The loop drops these for a provider without vision and substitutes the
    // description below, so both halves are always present.
    outcome.content.push_str("\n\n");
    outcome
        .content
        .push_str(&describe(&flat, &sprite.palette_key));
    outcome.images.push(("image/png".to_string(), png));
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::mode::Mode;
    use crate::agent::tools::ToolRegistry;

    fn render(args: Value) -> ToolOutcome {
        ToolRegistry::new().dispatch("render_character", &args, Mode::Plan, &ToolCtx::headless())
    }

    #[test]
    fn every_species_renders_a_png() {
        for species in ["human", "android", "robot", "voidborn", "xenotype"] {
            let out = render(json!({ "species": species }));
            assert!(!out.is_error, "{species}: {}", out.content);
            assert_eq!(out.images.len(), 1, "{species} produced no image");
            let (media_type, bytes) = &out.images[0];
            assert_eq!(media_type, "image/png");
            // PNG magic. A truncated or mis-encoded buffer is worse than an
            // error: the provider would send it and the model would see
            // nothing.
            assert_eq!(
                &bytes[..8],
                &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A],
                "{species} did not produce a PNG"
            );
        }
    }

    /// Same seed and same look must render the same bytes. The sprite pipeline
    /// is seed-deterministic and gated in CI; a render tool that drifted would
    /// be showing the model something the game will not draw.
    #[test]
    fn rendering_is_deterministic() {
        let args = json!({ "species": "xenotype", "seed": 7, "skin_color": "#80A060" });
        let a = render(args.clone());
        let b = render(args);
        assert_eq!(a.images[0].1, b.images[0].1);
        assert_eq!(a.content, b.content);
    }

    #[test]
    fn a_bad_hex_colour_names_the_field() {
        let out = render(json!({ "species": "human", "skin_color": "greenish" }));
        assert!(out.is_error);
        assert!(out.content.contains("skin_color"), "{}", out.content);
    }

    #[test]
    fn an_unknown_species_lists_the_valid_ones() {
        let out = render(json!({ "species": "elf" }));
        assert!(out.is_error);
        assert!(out.content.contains("voidborn"), "{}", out.content);
    }

    /// The written description is what a text-only model reads instead of the
    /// image, so it has to carry something actionable.
    #[test]
    fn the_description_reports_coverage_and_average_colour() {
        let out = render(json!({ "species": "human", "seed": 3 }));
        assert!(out.content.contains("% of the canvas"), "{}", out.content);
        assert!(out.content.contains('#'), "{}", out.content);
    }

    /// Dump a render to disk so a human can look at it. Ignored by default —
    /// this is an eyeball aid, not an assertion.
    #[test]
    #[ignore]
    fn dump_sprites_for_inspection() {
        for species in ["human", "android", "robot", "voidborn", "xenotype"] {
            let out = render(json!({ "species": species, "seed": 42 }));
            std::fs::write(format!("/tmp/rl-render/{species}.png"), &out.images[0].1).unwrap();
        }
    }

    #[test]
    fn hex_parsing_accepts_both_forms_and_rejects_junk() {
        assert_eq!(parse_hex("#8FA060"), Some([0x8F, 0xA0, 0x60]));
        assert_eq!(parse_hex("8FA060"), Some([0x8F, 0xA0, 0x60]));
        assert_eq!(parse_hex("#8FA0"), None);
        assert_eq!(parse_hex("zzzzzz"), None);
    }
}
