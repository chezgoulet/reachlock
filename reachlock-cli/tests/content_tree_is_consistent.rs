//! The committed content tree must resolve — every id a file references has
//! to be defined somewhere by something of the right kind.
//!
//! `make check-content` runs the same scan, but a gate only in the Makefile is
//! a gate that a `cargo test` run does not have. This is here so authoring a
//! reference to something that does not exist fails in the ordinary test loop,
//! at the moment it is introduced.
//!
//! When this fails, run it for the detail:
//!
//! ```text
//! cargo run -p reachlock-cli -- content check mods/reachlock
//! ```

use std::path::PathBuf;

use reachlock_core::content::ContentTree;

fn content_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is reachlock-cli/, so the tree is one level up.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate dir has a parent")
        .join("mods/reachlock")
}

#[test]
fn committed_content_tree_has_no_dangling_references() {
    let tree = ContentTree::scan(&content_root());
    let report = tree.check();

    let mut problems = Vec::new();
    for r in &report.dangling {
        problems.push(format!(
            "  {} ({}) points at {} \"{}\", which nothing defines",
            r.source_id,
            r.field,
            r.target_kind.label(),
            r.target_id
        ));
    }
    for (kind, id, files) in &report.duplicates {
        problems.push(format!(
            "  {} \"{}\" is defined {} times: {}",
            kind.label(),
            id,
            files.len(),
            files
                .iter()
                .map(|f| f.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    for u in &report.unparseable {
        problems.push(format!(
            "  {} parses as no known payload — every loader skips it: {}",
            u.file.display(),
            u.reason
        ));
    }

    assert!(
        problems.is_empty(),
        "the content tree does not resolve:\n{}",
        problems.join("\n")
    );
}

/// The scan has to actually find the tree. Without this, a wrong path would
/// make the test above pass by scanning nothing at all — the exact failure
/// mode that lets a gate report green while checking no files.
#[test]
fn the_scan_finds_the_committed_tree() {
    let tree = ContentTree::scan(&content_root());
    assert!(
        tree.definitions.len() > 40,
        "expected the committed tree, found {} definitions — is the path right?",
        tree.definitions.len()
    );
    assert!(
        tree.references.len() > 40,
        "expected cross-references, found {}",
        tree.references.len()
    );
}

/// Every origin must be playable: it names a career, and if it names a ship or
/// crew those must exist too. This is the specific invariant that was broken —
/// nine of ten origins were unplayable as authored — so it gets its own test
/// rather than relying on the aggregate above to imply it.
#[test]
fn every_origin_resolves_completely() {
    use reachlock_core::content::RefKind;

    let tree = ContentTree::scan(&content_root());
    let report = tree.check();

    let origins: Vec<_> = tree
        .definitions
        .iter()
        .filter(|d| d.kind == RefKind::Origin)
        .collect();
    assert!(!origins.is_empty(), "no origins found");

    for origin in &origins {
        let broken: Vec<_> = report
            .dangling
            .iter()
            .filter(|r| r.source_id == origin.id)
            .map(|r| {
                format!(
                    "{} -> {} \"{}\"",
                    r.field,
                    r.target_kind.label(),
                    r.target_id
                )
            })
            .collect();
        assert!(
            broken.is_empty(),
            "origin \"{}\" is not playable: {}",
            origin.id,
            broken.join(", ")
        );

        let has_career = tree
            .references
            .iter()
            .any(|r| r.source_id == origin.id && r.target_kind == RefKind::Career);
        assert!(
            has_career,
            "origin \"{}\" names no starting career",
            origin.id
        );
    }
}
