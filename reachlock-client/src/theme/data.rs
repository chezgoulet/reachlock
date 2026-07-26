//! Stylesheet parsing and resolution.
//!
//! Three layers resolve into one another: `palette` holds raw colors,
//! `roles` give them semantic names, and `classes` are style rules written
//! against roles. UI code names only classes, so a re-skin edits this file
//! and nothing else.

use std::collections::{BTreeMap, HashMap};

use bevy::prelude::*;
use serde::Deserialize;

/// The on-disk stylesheet, straight from RON.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeFile {
    pub id: String,
    pub name: String,
    pub fonts: FontPaths,
    #[serde(default)]
    pub palette: BTreeMap<String, String>,
    #[serde(default)]
    pub roles: BTreeMap<String, String>,
    #[serde(default)]
    pub text_sizes: BTreeMap<String, f32>,
    #[serde(default)]
    pub spacing: BTreeMap<String, f32>,
    #[serde(default)]
    pub classes: BTreeMap<String, ClassRule>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FontPaths {
    pub regular: String,
    pub bold: String,
}

/// One style rule. Every field is optional: an omitted property leaves that
/// aspect of the entity untouched, so a class can style text only, layout
/// only, or both.
///
/// `deny_unknown_fields` is deliberate — a misspelled property in a
/// stylesheet must fail loudly rather than silently doing nothing.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ClassRule {
    // Paint. Values name a role ("accent") or a palette entry ("amber").
    pub fg: Option<String>,
    pub bg: Option<String>,
    pub border_color: Option<String>,

    // Text. `size` names a `text_sizes` key; `weight` is "bold" or "regular".
    pub size: Option<String>,
    pub weight: Option<String>,
    /// Extra space between characters, in "spaces" (0 = none). Bevy 0.18 has
    /// no letter-spacing, so this is applied as a text transform — see
    /// [`super::apply_tracking`].
    pub letter_spacing: Option<f32>,

    // Box. Widths are pixels; padding/margin/gap name a `spacing` key.
    pub border: Option<f32>,
    pub border_top: Option<f32>,
    pub border_bottom: Option<f32>,
    pub border_left: Option<f32>,
    pub border_right: Option<f32>,
    pub padding: Option<String>,
    pub padding_x: Option<String>,
    pub padding_y: Option<String>,
    pub margin: Option<String>,
    pub row_gap: Option<String>,
    pub column_gap: Option<String>,
    pub width: Option<f32>,
    pub height: Option<f32>,
}

/// A class rule with every reference resolved to a concrete value. This is
/// what the resolver system writes onto entities.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Style {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub border_color: Option<Color>,
    pub font_size: Option<f32>,
    pub bold: bool,
    pub letter_spacing: Option<f32>,
    pub border: Option<UiRect>,
    pub padding: Option<UiRect>,
    pub margin: Option<UiRect>,
    pub row_gap: Option<f32>,
    pub column_gap: Option<f32>,
    pub width: Option<f32>,
    pub height: Option<f32>,
}

/// Parse a stylesheet.
///
/// Every property on a class rule is an `Option`, and plain RON would make an
/// author write `fg: Some("accent")` on every line. `IMPLICIT_SOME` lets the
/// stylesheet read like a stylesheet — `fg: "accent"` — without each file
/// needing an `#![enable(...)]` header.
pub fn parse_theme(text: &str) -> Result<ThemeFile, ron::error::SpannedError> {
    ron::Options::default()
        .with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME)
        .from_str(text)
}

impl ThemeFile {
    /// Resolve every class. Returns the resolved table plus a list of
    /// problems — a bad reference disables that one property rather than
    /// failing the whole theme, so a typo never leaves the game unstyled.
    pub fn resolve(&self) -> (HashMap<String, Style>, Vec<String>) {
        let mut problems = Vec::new();
        let mut out = HashMap::with_capacity(self.classes.len());

        for (name, rule) in &self.classes {
            let mut style = Style::default();
            let mut color = |key: &Option<String>, prop: &str| -> Option<Color> {
                let key = key.as_ref()?;
                match self.color(key) {
                    Ok(c) => Some(c),
                    Err(e) => {
                        problems.push(format!("class '{name}' property '{prop}': {e}"));
                        None
                    }
                }
            };
            style.fg = color(&rule.fg, "fg");
            style.bg = color(&rule.bg, "bg");
            style.border_color = color(&rule.border_color, "border_color");

            if let Some(key) = &rule.size {
                match self.text_sizes.get(key) {
                    Some(v) => style.font_size = Some(*v),
                    None => problems.push(format!("class '{name}': unknown text size '{key}'")),
                }
            }
            match rule.weight.as_deref() {
                None | Some("regular") => {}
                Some("bold") => style.bold = true,
                Some(other) => {
                    problems.push(format!(
                        "class '{name}': weight '{other}' is not 'regular' or 'bold'"
                    ));
                }
            }
            style.letter_spacing = rule.letter_spacing;

            // Border widths are pixels, so no scale lookup.
            if rule.border.is_some()
                || rule.border_top.is_some()
                || rule.border_bottom.is_some()
                || rule.border_left.is_some()
                || rule.border_right.is_some()
            {
                let all = rule.border.unwrap_or(0.0);
                style.border = Some(UiRect {
                    top: Val::Px(rule.border_top.unwrap_or(all)),
                    bottom: Val::Px(rule.border_bottom.unwrap_or(all)),
                    left: Val::Px(rule.border_left.unwrap_or(all)),
                    right: Val::Px(rule.border_right.unwrap_or(all)),
                });
            }

            let mut space = |key: &Option<String>, prop: &str| -> Option<f32> {
                let key = key.as_ref()?;
                match self.spacing.get(key) {
                    Some(v) => Some(*v),
                    None => {
                        problems.push(format!(
                            "class '{name}' property '{prop}': unknown spacing '{key}'"
                        ));
                        None
                    }
                }
            };
            let pad_all = space(&rule.padding, "padding");
            let pad_x = space(&rule.padding_x, "padding_x");
            let pad_y = space(&rule.padding_y, "padding_y");
            if pad_all.is_some() || pad_x.is_some() || pad_y.is_some() {
                let base = pad_all.unwrap_or(0.0);
                style.padding = Some(UiRect {
                    left: Val::Px(pad_x.unwrap_or(base)),
                    right: Val::Px(pad_x.unwrap_or(base)),
                    top: Val::Px(pad_y.unwrap_or(base)),
                    bottom: Val::Px(pad_y.unwrap_or(base)),
                });
            }
            if let Some(m) = space(&rule.margin, "margin") {
                style.margin = Some(UiRect::all(Val::Px(m)));
            }
            style.row_gap = space(&rule.row_gap, "row_gap");
            style.column_gap = space(&rule.column_gap, "column_gap");
            style.width = rule.width;
            style.height = rule.height;

            out.insert(name.clone(), style);
        }
        (out, problems)
    }

    /// Look a color up by role name first, then by palette name, so classes
    /// may reference either. Roles point at the palette, one level only.
    fn color(&self, key: &str) -> Result<Color, String> {
        let hex = if let Some(target) = self.roles.get(key) {
            self.palette
                .get(target)
                .ok_or_else(|| format!("role '{key}' points at unknown palette entry '{target}'"))?
        } else {
            self.palette
                .get(key)
                .ok_or_else(|| format!("'{key}' is not a role or a palette entry"))?
        };
        parse_hex(hex).ok_or_else(|| format!("'{key}' has malformed color '{hex}'"))
    }
}

/// `#rrggbb` or `#rrggbbaa`.
pub fn parse_hex(s: &str) -> Option<Color> {
    let s = s.strip_prefix('#')?;
    if s.len() != 6 && s.len() != 8 {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(s.get(i..i + 2)?, 16).ok();
    Some(Color::srgba_u8(
        byte(0)?,
        byte(2)?,
        byte(4)?,
        if s.len() == 8 { byte(6)? } else { 255 },
    ))
}

/// Insert `count` spaces between characters. Bevy 0.18 has no letter-spacing
/// property, so wide-tracked headings are produced by transforming the text.
pub fn apply_tracking(source: &str, spacing: f32) -> String {
    let count = spacing.round().max(0.0) as usize;
    if count == 0 {
        return source.to_string();
    }
    let pad = " ".repeat(count);
    source
        .chars()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(&pad)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHEET: &str = r##"(
        id: "t", name: "T",
        fonts: (regular: "a.ttf", bold: "b.ttf"),
        palette: { "ink": "#d7e3ea", "amber": "#ffb454", "clear": "#00000000" },
        roles: { "text": "ink", "accent": "amber" },
        text_sizes: { "body": 15.0 },
        spacing: { "md": 16.0 },
        classes: {
            "body": ( fg: "text", size: "body" ),
            "direct": ( fg: "amber" ),
            "boxed": ( bg: "clear", border: 1.0, padding: "md", border_bottom: 3.0 ),
        },
    )"##;

    fn sheet() -> ThemeFile {
        parse_theme(SHEET).expect("stylesheet parses")
    }

    #[test]
    fn resolves_roles_and_direct_palette_refs() {
        let (classes, problems) = sheet().resolve();
        assert!(problems.is_empty(), "unexpected problems: {problems:?}");
        assert_eq!(classes["body"].fg, parse_hex("#d7e3ea"));
        assert_eq!(classes["body"].font_size, Some(15.0));
        // A class may name the palette directly, bypassing the role layer.
        assert_eq!(classes["direct"].fg, parse_hex("#ffb454"));
    }

    #[test]
    fn per_side_border_overrides_the_shorthand() {
        let (classes, _) = sheet().resolve();
        let border = classes["boxed"].border.expect("border set");
        assert_eq!(border.top, Val::Px(1.0));
        assert_eq!(border.bottom, Val::Px(3.0), "per-side must win");
        assert_eq!(classes["boxed"].padding, Some(UiRect::all(Val::Px(16.0))));
    }

    #[test]
    fn parses_hex_with_and_without_alpha() {
        assert_eq!(parse_hex("#00000000"), Some(Color::srgba_u8(0, 0, 0, 0)));
        assert_eq!(
            parse_hex("#ffb454"),
            Some(Color::srgba_u8(255, 180, 84, 255))
        );
        assert_eq!(parse_hex("ffb454"), None, "missing # is malformed");
        assert_eq!(parse_hex("#fff"), None, "3-digit shorthand unsupported");
        assert_eq!(parse_hex("#gggggg"), None, "non-hex digits");
    }

    #[test]
    /// A bad reference must disable one property and report it, not poison
    /// the whole stylesheet — otherwise one typo leaves the game unstyled.
    fn bad_references_are_reported_not_fatal() {
        let src = r##"(
            id: "t", name: "T",
            fonts: (regular: "a.ttf", bold: "b.ttf"),
            palette: { "ink": "#d7e3ea" },
            roles: { "text": "ink", "broken": "nonexistent" },
            text_sizes: { "body": 15.0 },
            spacing: {},
            classes: {
                "ok":       ( fg: "text", size: "body" ),
                "bad-role": ( fg: "broken" ),
                "bad-size": ( fg: "text", size: "nope" ),
                "bad-wght": ( fg: "text", weight: "heavy" ),
            },
        )"##;
        let file: ThemeFile = parse_theme(src).unwrap();
        let (classes, problems) = file.resolve();
        assert_eq!(
            problems.len(),
            3,
            "each bad ref reported once: {problems:?}"
        );
        assert_eq!(classes["ok"].fg, parse_hex("#d7e3ea"));
        assert_eq!(
            classes["bad-role"].fg, None,
            "bad ref disables the property"
        );
        assert_eq!(classes["bad-size"].font_size, None);
        assert_eq!(
            classes["bad-size"].fg,
            parse_hex("#d7e3ea"),
            "a bad property must not discard the good ones alongside it"
        );
        assert!(!classes["bad-wght"].bold);
    }

    #[test]
    /// A misspelled property is a stylesheet bug. Silently ignoring it is how
    /// a theme ends up half-applied with nothing to point at.
    fn unknown_properties_are_rejected() {
        let src = r##"(
            id: "t", name: "T",
            fonts: (regular: "a.ttf", bold: "b.ttf"),
            palette: {}, roles: {}, text_sizes: {}, spacing: {},
            classes: { "x": ( colour: "text" ) },
        )"##;
        assert!(
            parse_theme(src).is_err(),
            "'colour' is not a property and must not parse"
        );
    }

    #[test]
    fn tracking_spaces_characters() {
        assert_eq!(apply_tracking("ABC", 0.0), "ABC");
        assert_eq!(apply_tracking("ABC", 1.0), "A B C");
        assert_eq!(apply_tracking("ABC", 2.0), "A  B  C");
        assert_eq!(apply_tracking("", 3.0), "");
    }

    #[test]
    /// Tracking runs from an untransformed source every time, so a live
    /// theme reload cannot compound spacing on already-spaced text.
    fn tracking_is_idempotent_from_source() {
        let source = "REACHLOCK";
        let once = apply_tracking(source, 2.0);
        let twice = apply_tracking(source, 2.0);
        assert_eq!(once, twice);
    }
}
