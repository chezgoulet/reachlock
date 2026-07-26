use reachlock_core::crew::{CrewMemberEntry, CrewPackage};

use super::super::app::{ContentType, Editor};
use crate::io::EnvelopeMeta;

pub struct CrewPackageEditor {
    path: Option<std::path::PathBuf>,
    pkg: CrewPackage,
    /// Envelope fields the UI doesn't edit but the file must keep.
    meta: EnvelopeMeta,
    has_changes: bool,
}

impl Editor for CrewPackageEditor {
    fn title(&self) -> &str {
        "Crew Package Editor"
    }

    fn content_type(&self) -> ContentType {
        ContentType::CrewPackage
    }

    fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }

    fn touch(&mut self) {
        self.has_changes = true;
    }

    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        let (meta, pkg) = crate::io::read_enveloped::<CrewPackage>(path)?;
        self.pkg = pkg;
        self.meta = meta;
        self.path = Some(path.to_path_buf());
        self.has_changes = false;
        Ok(())
    }

    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        crate::io::write_enveloped(path, &self.meta, self.pkg.clone())
    }

    fn validate(&self) -> Vec<String> {
        let mut errs = Vec::new();
        if self.pkg.id.is_empty() {
            errs.push("id must not be empty".into());
        }
        if self.pkg.name.is_empty() {
            errs.push("name must not be empty".into());
        }
        for (i, m) in self.pkg.members.iter().enumerate() {
            if m.soul_id.is_empty() {
                errs.push(format!("member {i}: soul_id must not be empty"));
            }
        }
        errs
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        egui::Grid::new("crew_pkg").show(ui, |ui| {
            ui.label("ID:");
            changed |= ui.text_edit_singleline(&mut self.pkg.id).changed();
            ui.end_row();
            ui.label("Name:");
            changed |= ui.text_edit_singleline(&mut self.pkg.name).changed();
            ui.end_row();
            ui.label("Description:");
            changed |= ui.text_edit_multiline(&mut self.pkg.description).changed();
            ui.end_row();
        });
        ui.separator();
        ui.label("Members:");
        let mut remove = None;
        for (i, member) in self.pkg.members.iter_mut().enumerate() {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Soul:");
                    changed |= ui.text_edit_singleline(&mut member.soul_id).changed();
                    ui.label("Role:");
                    changed |= ui.text_edit_singleline(&mut member.role).changed();
                    if ui.button("\u{00d7}").clicked() {
                        remove = Some(i);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Duty room:");
                    let mut duty = member.duty_room.clone().unwrap_or_default();
                    changed |= ui.text_edit_singleline(&mut duty).changed();
                    member.duty_room = if duty.is_empty() { None } else { Some(duty) };
                    ui.checkbox(&mut member.starting, "Starting");
                });
            });
        }
        if let Some(i) = remove {
            self.pkg.members.remove(i);
            changed = true;
        }
        if ui.button("+ Add Member").clicked() {
            self.pkg.members.push(CrewMemberEntry {
                soul_id: String::new(),
                role: String::new(),
                duty_room: None,
                starting: true,
                salary: 0,
            });
            changed = true;
        }
        if changed {
            self.touch();
        }
    }

    fn generate_from_seed(&mut self, _seed: u64) {}

    fn accept_seed_reroll(&self) -> bool {
        false
    }

    fn snapshot(&self) -> Option<String> {
        ron::ser::to_string(&self.pkg).ok()
    }

    fn restore_snapshot(&mut self, ron_text: &str) -> Result<(), String> {
        self.pkg = ron::from_str(ron_text).map_err(|e| e.to_string())?;
        self.has_changes = true;
        Ok(())
    }

    fn mark_saved(&mut self) {
        self.has_changes = false;
    }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(CrewPackageEditor {
        path: None,
        pkg: default_pkg(),
        meta: EnvelopeMeta::new_for("new_crew_package"),
        has_changes: false,
    })
}

fn default_pkg() -> CrewPackage {
    CrewPackage {
        id: String::new(),
        name: String::new(),
        description: String::new(),
        members: vec![],
    }
}
