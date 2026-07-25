use crate::app::Editor;
use crate::cross_ref::CrossReferenceIndex;

#[expect(dead_code)]
pub fn validate_cross_refs(
    editor: &dyn Editor,
    _index: &CrossReferenceIndex,
) -> Vec<(String, String)> {
    let mut issues = Vec::new();
    let existing = editor.validate();

    issues.extend(existing.into_iter().map(|msg| (String::new(), msg)));

    issues
}

pub fn broken_reference_report(
    editors: &[(String, &dyn Editor)],
    index: &CrossReferenceIndex,
) -> Vec<(String, Vec<String>)> {
    let mut report: Vec<(String, Vec<String>)> = Vec::new();

    let broken = index.broken_references();
    if !broken.is_empty() {
        let mut grouped: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();
        for (source_id, target_id, field_path) in &broken {
            let msg = format!("→ {} (via {})", target_id, field_path);
            grouped.entry(source_id.clone()).or_default().push(msg);
        }
        for (source, msgs) in grouped {
            report.push((source, msgs));
        }
    }

    for (name, editor) in editors {
        let mut issues = editor.validate();
        let cross_refs = editor.validate_cross_refs(index);
        for (_field, msg) in &cross_refs {
            issues.push(msg.clone());
        }
        if !issues.is_empty() {
            report.push((name.clone(), issues));
        }
    }

    report
}

pub fn count_broken_refs_in_editor(editor: &dyn Editor, index: &CrossReferenceIndex) -> usize {
    let cross_issues = editor.validate_cross_refs(index);
    cross_issues.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ContentType;
    use crate::cross_ref::{CrossReferenceIndex, Reference};

    #[expect(dead_code)]
    struct TestEditor {
        issues: Vec<String>,
    }

    impl Editor for TestEditor {
        fn title(&self) -> &str {
            "test"
        }
        fn content_type(&self) -> ContentType {
            ContentType::Soul
        }
        fn has_unsaved_changes(&self) -> bool {
            false
        }
        fn load(&mut self, _: &std::path::Path) -> Result<(), String> {
            Ok(())
        }
        fn save(&self, _: &std::path::Path) -> Result<(), String> {
            Ok(())
        }
        fn validate(&self) -> Vec<String> {
            self.issues.clone()
        }
        fn ui(&mut self, _: &mut egui::Ui) {}
        fn generate_from_seed(&mut self, _: u64) {}
    }

    #[test]
    fn broken_ref_detected() {
        let mut index = CrossReferenceIndex::new();
        index.outgoing.insert(
            "test_soul".into(),
            vec![Reference {
                source_id: "test_soul".into(),
                source_type: ContentType::Soul,
                field_path: "faction_affiliation".into(),
                target_id: "nonexistent_faction".into(),
            }],
        );

        let broken = index.broken_references();
        assert_eq!(broken.len(), 1);
        assert_eq!(broken[0].1, "nonexistent_faction");
    }

    #[test]
    fn fix_clears_report() {
        let mut index = CrossReferenceIndex::new();
        index.all_ids.insert("existing_faction".into());
        index.outgoing.insert(
            "test_soul".into(),
            vec![Reference {
                source_id: "test_soul".into(),
                source_type: ContentType::Soul,
                field_path: "faction_affiliation".into(),
                target_id: "existing_faction".into(),
            }],
        );

        let broken = index.broken_references();
        assert!(broken.is_empty(), "known target should not be broken");
    }
}
