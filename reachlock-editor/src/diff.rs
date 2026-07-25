use std::path::Path;

#[expect(dead_code)]
pub struct DiffResult {
    pub old: Vec<String>,
    pub new: Vec<String>,
    pub unified: String,
    pub unchanged: bool,
}

impl DiffResult {
    pub fn compute(path: &Path, new_text: &str) -> Result<DiffResult, String> {
        let old_text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(_) => {
                return Ok(DiffResult {
                    old: Vec::new(),
                    new: new_text.lines().map(|l| l.to_string()).collect(),
                    unified: String::new(),
                    unchanged: false,
                });
            }
        };

        if old_text == new_text {
            return Ok(DiffResult {
                old: old_text.lines().map(|l| l.to_string()).collect(),
                new: new_text.lines().map(|l| l.to_string()).collect(),
                unified: String::new(),
                unchanged: true,
            });
        }

        let old_lines: Vec<&str> = old_text.lines().collect();
        let new_lines: Vec<&str> = new_text.lines().collect();

        let diff = similar::TextDiff::from_lines(old_text.as_str(), new_text);

        let mut unified = String::new();
        for change in diff.iter_all_changes() {
            let sign = match change.tag() {
                similar::ChangeTag::Delete => "-",
                similar::ChangeTag::Insert => "+",
                similar::ChangeTag::Equal => " ",
            };
            unified.push_str(&format!("{}{}", sign, change.value()));
        }

        Ok(DiffResult {
            old: old_lines.into_iter().map(|l| l.to_string()).collect(),
            new: new_lines.into_iter().map(|l| l.to_string()).collect(),
            unified,
            unchanged: false,
        })
    }
}

pub fn render_diff_ui(ui: &mut egui::Ui, diff: &DiffResult) {
    if diff.unchanged {
        ui.label("No changes — file is up to date.");
        return;
    }

    if diff.old.is_empty() {
        ui.label("New file — will be created on save.");
        return;
    }

    egui::ScrollArea::vertical()
        .max_height(400.0)
        .show(ui, |ui| {
            egui::Frame::NONE
                .fill(ui.visuals().extreme_bg_color)
                .show(ui, |ui| {
                    for line in diff.unified.lines() {
                        let (color, text) = if line.starts_with('+') {
                            (egui::Color32::from_rgb(0x4C, 0xAF, 0x50), line)
                        } else if line.starts_with('-') {
                            (egui::Color32::from_rgb(0xF4, 0x43, 0x36), line)
                        } else if line.starts_with('@') {
                            (egui::Color32::from_rgb(0x41, 0x69, 0xE1), line)
                        } else {
                            (ui.visuals().text_color(), line)
                        };
                        ui.colored_label(color, egui::RichText::new(text).monospace().size(12.0));
                    }
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn unchanged_diff() {
        let dir = std::env::temp_dir().join("reachlock_diff_tests");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test.ron");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "foo: 1").unwrap();
        writeln!(f, "bar: 2").unwrap();
        drop(f);

        let result = DiffResult::compute(&path, "foo: 1\nbar: 2\n").unwrap();
        assert!(result.unchanged);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn new_file_diff() {
        let path = Path::new("/tmp/reachlock_nonexistent_test.ron");
        let result = DiffResult::compute(path, "foo: 1\n").unwrap();
        assert!(!result.unchanged);
        assert!(result.old.is_empty());
    }

    #[test]
    fn changed_diff_has_changes() {
        let dir = std::env::temp_dir().join("reachlock_diff_tests2");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test.ron");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "foo: 1").unwrap();
        writeln!(f, "bar: 2").unwrap();
        drop(f);

        let result = DiffResult::compute(&path, "foo: 1\nbar: 3\n").unwrap();
        assert!(!result.unchanged);
        assert!(result.unified.contains("+bar: 3"));

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }
}
