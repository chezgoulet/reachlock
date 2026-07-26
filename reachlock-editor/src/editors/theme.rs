//! Music Theme editor (S48, Phase B): seed note sequence + scale + variation mask.
//!
//! This tab used to render four read-only labels — id, scale, note count, bpm
//! range — so a theme could be opened but not changed, and never heard. Every
//! field is editable now, and the preview panel plays the theme through the
//! same note-to-pitch mapping the game uses.

use reachlock_core::generator::music::{NoteEvent, Scale, Theme, VariationMask};

/// The mask's named bits, in the order the generator applies them.
///
/// Editing this as a raw number is how `allowed_variations: (65535)` gets
/// hand-written wrong — a newtype in RON needs its parens, and a file that
/// misses them is skipped silently by every loader.
const VARIATION_BITS: &[(u16, &str)] = &[
    (VariationMask::TRANSPOSE, "Transpose"),
    (VariationMask::PASSING_TONES, "Passing tones"),
    (VariationMask::RHYTHMIC_SHIFT, "Rhythmic shift"),
    (VariationMask::REPETITION, "Repetition"),
    (VariationMask::ARTICULATION, "Articulation"),
    (VariationMask::PHRASE_SWAP, "Phrase swap"),
    (VariationMask::REST_INSERTION, "Rest insertion"),
    (VariationMask::SUBSTITUTION, "Substitution"),
    (VariationMask::ORNAMENTATION, "Ornamentation"),
];

use super::super::app::{ContentType, Editor};
use crate::io::EnvelopeMeta;

pub struct ThemeEditor {
    path: Option<std::path::PathBuf>,
    theme: Theme,
    /// Envelope fields the UI doesn't edit but the file must keep. Themes live
    /// on disk as `ContentFile` envelopes; this tab used to read and write the
    /// bare `Theme`, so it could not open a single authored theme.
    meta: EnvelopeMeta,
    has_changes: bool,
    /// Held for as long as the preview is audible — dropping it stops the
    /// sound, which is also how Stop works.
    playing: Option<crate::audio::Playing>,
    preview_status: String,
}

impl ThemeEditor {
    /// A genuinely new document.
    ///
    /// This used to adopt the first `.ron` in the content directory, so
    /// `File > New` silently bound to an existing file and the first save
    /// overwrote it.
    fn new() -> Self {
        ThemeEditor {
            path: None,
            theme: Theme {
                id: "new_theme".into(),
                notes: vec![],
                scale: reachlock_core::generator::music::Scale::MinorPentatonic,
                bpm_range: (60, 80),
                allowed_variations: reachlock_core::generator::music::VariationMask(511),
            },
            meta: EnvelopeMeta::new_for("new_theme"),
            has_changes: false,
            playing: None,
            preview_status: String::new(),
        }
    }
}

impl ThemeEditor {
    /// Render at the theme's own low tempo bound and a fixed seed, so
    /// repeated presses sound the same and A/B-ing an edit is meaningful.
    fn preview_intent(&self) -> reachlock_core::generator::music::MusicIntent {
        use reachlock_core::generator::music::{generate_themed_music, Mood};
        generate_themed_music(self.meta.seed, Mood::Calm, &self.theme, 8, 4)
    }

    fn play_preview(&mut self) -> String {
        if self.theme.notes.is_empty() {
            // The generator would still emit a drone, which sounds like a
            // failure rather than an empty theme.
            return "No notes to play — add some first.".into();
        }
        let audio = crate::audio::render(&self.preview_intent());
        // Replace rather than layer: pressing Play twice should restart, not
        // play the theme over itself.
        self.playing = None;
        match crate::audio::play(&audio) {
            Ok(handle) => {
                self.playing = Some(handle);
                let secs = audio.samples.len() as f32 / audio.sample_rate as f32;
                format!("playing {secs:.1}s")
            }
            Err(e) => format!("cannot play: {e}"),
        }
    }

    fn save_wav(&self) -> String {
        use reachlock_core::generator::music::to_wav_bytes;
        let audio = crate::audio::render(&self.preview_intent());
        let stem = crate::io::file_stem_for_id(&self.theme.id, "theme");
        let path = std::path::PathBuf::from(format!("save/{stem}.wav"));
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::write(&path, to_wav_bytes(&audio)) {
            Ok(()) => format!("wrote {}", path.display()),
            Err(e) => format!("could not write {}: {e}", path.display()),
        }
    }
}

impl Editor for ThemeEditor {
    fn title(&self) -> &str {
        &self.theme.id
    }
    fn content_type(&self) -> ContentType {
        ContentType::Theme
    }
    fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }
    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        let (meta, theme) =
            crate::io::read_enveloped::<Theme>(path).map_err(|e| format!("reading theme: {e}"))?;
        self.theme = theme;
        self.meta = meta;
        self.path = Some(path.to_path_buf());
        self.has_changes = false;
        Ok(())
    }
    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        crate::io::write_enveloped(path, &self.meta, self.theme.clone())
            .map_err(|e| format!("saving theme: {e}"))
    }
    fn save_all(&mut self) -> Result<bool, String> {
        // Only write when dirty, and never invent a filename: the old
        // fallback name meant two new documents overwrote each other.
        // Returning Ok(false) with no path lets the shell run Save As.
        if !self.has_changes {
            return Ok(self.path.is_some());
        }
        let Some(path) = self.path.clone() else {
            return Ok(false);
        };
        self.save(&path)?;
        self.has_changes = false;
        Ok(true)
    }
    fn generate_from_seed(&mut self, seed: u64) {
        let intent = reachlock_core::generator::generate_music_intent(
            seed,
            reachlock_core::generator::music::Mood::Calm,
            8,
        );
        self.theme.notes = intent
            .notes
            .iter()
            .map(|n| reachlock_core::generator::music::NoteEvent {
                degree: n.degree,
                octave: n.octave,
                velocity: n.velocity,
                start_tick: n.start_tick,
                duration_ticks: n.duration_ticks,
            })
            .collect();
        self.theme.id = format!("theme_{:#x}", seed);
        // The envelope's id is what the content tree indexes, so it has to
        // follow the payload's rename or the file defines an id nothing
        // references.
        self.meta.id = self.theme.id.clone();
        self.meta.display_name = self.theme.id.clone();
        self.meta.seed = seed;
        self.has_changes = true;
    }
    fn validate(&self) -> Vec<String> {
        let mut errors = vec![];
        if self.theme.id.is_empty() {
            errors.push("id is empty".into());
        }
        errors
    }
    fn ui(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show_inside(ui, |ui| {
            let before = self.theme.clone();

            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Id:");
                    ui.text_edit_singleline(&mut self.theme.id);
                });

                ui.horizontal(|ui| {
                    if ui
                        .button("▶ Play")
                        .on_hover_text(
                            "Render this theme and play it. Uses the same note-to-pitch \
                             mapping as the game; the game's own synth adds filtering this \
                             preview does not.",
                        )
                        .clicked()
                    {
                        self.preview_status = self.play_preview();
                    }
                    if ui
                        .add_enabled(self.playing.is_some(), egui::Button::new("■ Stop"))
                        .clicked()
                    {
                        self.playing = None;
                        self.preview_status.clear();
                    }
                    if ui
                        .button("Save WAV")
                        .on_hover_text(
                            "Write the rendered preview next to the content root, for \
                             listening outside the editor.",
                        )
                        .clicked()
                    {
                        self.preview_status = self.save_wav();
                    }
                    if !self.preview_status.is_empty() {
                        ui.weak(self.preview_status.clone());
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Scale:");
                    egui::ComboBox::from_id_salt("theme_scale")
                        .selected_text(format!("{:?}", self.theme.scale))
                        .show_ui(ui, |ui| {
                            for scale in [
                                Scale::MinorPentatonic,
                                Scale::MajorPentatonic,
                                Scale::Dorian,
                                Scale::Octatonic,
                            ] {
                                ui.selectable_value(
                                    &mut self.theme.scale,
                                    scale,
                                    format!("{scale:?}"),
                                );
                            }
                        });
                });

                ui.horizontal(|ui| {
                    ui.label("BPM:");
                    ui.add(egui::DragValue::new(&mut self.theme.bpm_range.0).range(20..=300));
                    ui.label("to");
                    ui.add(egui::DragValue::new(&mut self.theme.bpm_range.1).range(20..=300));
                });
                // A backwards range silently yields no tempo at all rather
                // than erroring, so keep it ordered as it is edited.
                if self.theme.bpm_range.1 < self.theme.bpm_range.0 {
                    self.theme.bpm_range.1 = self.theme.bpm_range.0;
                }

                ui.separator();
                ui.strong("Allowed variations");
                ui.label(
                    egui::RichText::new(
                        "Which operators the generator may apply when it varies this theme.",
                    )
                    .weak(),
                );
                // The mask is a u16 of named bits. Editing it as a number is
                // how `allowed_variations: (65535)` gets hand-written wrong;
                // checkboxes make the RON the editor's problem, not the
                // author's.
                for (bit, name) in VARIATION_BITS {
                    let mut on = self.theme.allowed_variations.0 & bit != 0;
                    if ui.checkbox(&mut on, *name).changed() {
                        if on {
                            self.theme.allowed_variations.0 |= bit;
                        } else {
                            self.theme.allowed_variations.0 &= !bit;
                        }
                    }
                }

                ui.separator();
                ui.horizontal(|ui| {
                    ui.strong(format!("Notes ({})", self.theme.notes.len()));
                    if ui.button("+ Add").clicked() {
                        // Follow on from the last note rather than stacking
                        // everything at tick 0, which is silent overlap.
                        let start = self
                            .theme
                            .notes
                            .last()
                            .map(|n| n.start_tick + n.duration_ticks)
                            .unwrap_or(0);
                        self.theme.notes.push(NoteEvent {
                            degree: 0,
                            octave: 4,
                            velocity: 100,
                            start_tick: start,
                            duration_ticks: 24,
                        });
                    }
                });

                let mut remove = None;
                for (i, note) in self.theme.notes.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{i:>3}"));
                        ui.label("deg");
                        ui.add(egui::DragValue::new(&mut note.degree).range(0..=11));
                        ui.label("oct");
                        ui.add(egui::DragValue::new(&mut note.octave).range(0..=8));
                        ui.label("vel");
                        ui.add(egui::DragValue::new(&mut note.velocity).range(0..=127));
                        ui.label("at");
                        ui.add(egui::DragValue::new(&mut note.start_tick).range(0..=9999));
                        ui.label("len");
                        ui.add(egui::DragValue::new(&mut note.duration_ticks).range(1..=9999));
                        if ui.small_button("✕").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    self.theme.notes.remove(i);
                }
                ui.label(egui::RichText::new("24 ticks is a quarter note; 96 is a bar.").weak());
            });

            // One dirty check for the whole form, rather than a `touch()` on
            // every widget — missing one is how an edit gets silently lost on
            // close.
            if self.theme != before {
                self.has_changes = true;
            }
        });
    }

    fn touch(&mut self) {
        self.has_changes = true;
    }

    fn snapshot(&self) -> Option<String> {
        ron::ser::to_string(&self.theme).ok()
    }

    fn restore_snapshot(&mut self, ron_text: &str) -> Result<(), String> {
        self.theme = ron::from_str(ron_text).map_err(|e| e.to_string())?;
        self.has_changes = true;
        Ok(())
    }

    /// Reroll only renames the id, which would break every cross-reference
    /// pointing at it. Opt out rather than corrupt the content graph.
    fn accept_seed_reroll(&self) -> bool {
        false
    }

    fn preview_ui(&self, ui: &mut egui::Ui) {
        ui.strong(self.content_type().name());
        let issues = self.validate();
        if issues.is_empty() {
            ui.colored_label(egui::Color32::from_rgb(0x4C, 0xAF, 0x50), "✔ clean");
        } else {
            ui.colored_label(
                egui::Color32::from_rgb(0xE5, 0x73, 0x73),
                format!("✘ {} issue(s)", issues.len()),
            );
        }

        ui.add_space(6.0);
        ui.label(format!("{:?}", self.theme.scale));
        ui.label(format!(
            "{} note(s), {}–{} bpm",
            self.theme.notes.len(),
            self.theme.bpm_range.0,
            self.theme.bpm_range.1
        ));

        // A crude piano roll. Reading a note list as numbers tells you almost
        // nothing about shape; seeing it does.
        if !self.theme.notes.is_empty() {
            ui.add_space(4.0);
            let width = ui.available_width().max(40.0);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 60.0), egui::Sense::hover());
            let painter = ui.painter_at(rect);
            let end_tick = self
                .theme
                .notes
                .iter()
                .map(|n| n.start_tick + n.duration_ticks)
                .max()
                .unwrap_or(96)
                .max(1) as f32;
            let (lo, hi) = self.theme.notes.iter().fold((u8::MAX, 0u8), |(lo, hi), n| {
                let pitch = n.degree.saturating_add(n.octave.saturating_mul(12));
                (lo.min(pitch), hi.max(pitch))
            });
            let span = (hi.saturating_sub(lo)).max(1) as f32;
            for n in &self.theme.notes {
                let pitch = n.degree.saturating_add(n.octave.saturating_mul(12));
                let x0 = rect.left() + rect.width() * (n.start_tick as f32 / end_tick);
                let x1 = rect.left()
                    + rect.width() * ((n.start_tick + n.duration_ticks) as f32 / end_tick);
                let y = rect.bottom() - rect.height() * ((pitch.saturating_sub(lo)) as f32 / span);
                painter.line_segment(
                    [egui::pos2(x0, y), egui::pos2(x1.max(x0 + 1.0), y)],
                    egui::Stroke::new(
                        2.0,
                        // Velocity as brightness, so a dynamic line reads.
                        egui::Color32::from_gray(80 + (n.velocity as u32 * 175 / 127) as u8),
                    ),
                );
            }
        }
    }

    fn mark_saved(&mut self) {
        self.has_changes = false;
    }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(ThemeEditor::new())
}
