//! S30 agent CI gate: fast checks for iron-rule violations.
//! `reachlock check agent` runs all checks and reports failures.

/// Result of a single check.
pub struct CheckResult {
    pub name: &'static str,
    pub passed: bool,
    pub detail: String,
}

/// Run every check. Returns a report.
pub fn run() -> Vec<CheckResult> {
    vec![
        check_core_purity(),
        check_determinism_coverage(),
        check_no_floats_in_gameplay(),
        check_wire_shape_pinned(),
        check_gotcha_scan(),
        check_sprint_branch(),
        check_module_registration(),
    ]
}

/// Check 1: core-purity — reachlock-core must not have rendering/IO deps.
fn check_core_purity() -> CheckResult {
    let path = "reachlock-core/Cargo.toml";
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => return CheckResult::fail("core-purity", "cannot read Cargo.toml"),
    };
    let forbidden = ["bevy", "rapier", "tokio", "reqwest", "wgpu", "ewebsock"];
    // Simple line scan for dependency names.
    for line in text.lines() {
        let trimmed = line.trim();
        for dep in &forbidden {
            if trimmed.starts_with(&format!("{dep} ")) || trimmed.starts_with(&format!("{dep}=")) {
                return CheckResult::fail("core-purity", &format!("{dep} found in Cargo.toml"));
            }
        }
    }
    CheckResult::pass("core-purity", "no rendering/IO deps in core")
}

/// Check 2: determinism-coverage — every generator function has a golden key.
fn check_determinism_coverage() -> CheckResult {
    let gen_dir = "reachlock-core/src/generator";
    let det_file = "reachlock-core/src/determinism.rs";
    let det_text = match std::fs::read_to_string(det_file) {
        Ok(t) => t,
        Err(_) => return CheckResult::fail("determinism-coverage", "cannot read determinism.rs"),
    };
    let gen_dir = std::path::Path::new(gen_dir);
    if !gen_dir.is_dir() {
        return CheckResult::fail("determinism-coverage", "generator dir not found");
    }
    let mut missing = Vec::new();
    for entry in walkdir::WalkDir::new(gen_dir) {
        let entry = match entry {
            Ok(e) => e,
            _ => continue,
        };
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "rs") {
            let text = match std::fs::read_to_string(path) {
                Ok(t) => t,
                _ => continue,
            };
            for line in text.lines() {
                let line = line.trim();
                // Match pub fn names.
                if let Some(name) = line.strip_prefix("pub fn ") {
                    if let Some(fn_name) = name.split('(').next() {
                        if fn_name.starts_with("generate_")
                            && !det_text.contains(fn_name)
                        {
                            missing.push(fn_name.to_string());
                        }
                    }
                }
            }
        }
    }
    if missing.is_empty() {
        CheckResult::pass("determinism-coverage", "all generators have golden keys")
    } else {
        CheckResult::fail("determinism-coverage", &format!("missing goldens: {}", missing.join(", ")))
    }
}

/// Check 3: no-floats-in-gameplay — no f32/f64 in core outside allowed paths.
fn check_no_floats_in_gameplay() -> CheckResult {
    let core_src = "reachlock-core/src";
    let allowed = [
        "util/color.rs",
        "util/trig.rs",
        "generator/music.rs",
        "combat/melee.rs",
    ];
    let mut violations = Vec::new();
    for entry in walkdir::WalkDir::new(core_src) {
        let entry = match entry {
            Ok(e) => e,
            _ => continue,
        };
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        // Skip allowed paths.
        let rel = path.to_string_lossy().replace('\\', "/");
        if allowed.iter().any(|a| rel.contains(a)) {
            continue;
        }
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            _ => continue,
        };
        // Heuristic: look for f32/f64/float type annotations outside comments.
        for (i, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with("///") {
                continue;
            }
            if ["f32", "f64", " Float", " f32", " f64"].iter().any(|h| trimmed.contains(h)) {
                violations.push(format!("{}:{}", rel, i + 1));
            }
        }
    }
    if violations.is_empty() {
        CheckResult::pass("no-floats-in-gameplay", "no f32/f64 in core outside allowed paths")
    } else {
        CheckResult::fail("no-floats-in-gameplay", &violations[..5].join("; "))
    }
}

/// Check 4: wire-shape-pinned — network message changes update tests.
fn check_wire_shape_pinned() -> CheckResult {
    let msg_path = "reachlock-core/src/network/messages.rs";
    let test_tag = "wire_tags_match_spec";
    let msg_text = match std::fs::read_to_string(msg_path) {
        Ok(t) => t,
        Err(_) => return CheckResult::fail("wire-shape-pinned", "cannot read messages.rs"),
    };
    // Check that the test file contains the wire tag test.
    if msg_text.contains(test_tag) || msg_text.contains("round_trip") {
        CheckResult::pass("wire-shape-pinned", "wire shape tests present")
    } else {
        CheckResult::fail("wire-shape-pinned", "no wire-shape test found")
    }
}

/// Check 5: gotcha-scan — scan for known gotcha patterns in staged code.
fn check_gotcha_scan() -> CheckResult {
    // Rust raw string gotcha: r#"..."# on lines containing "# → use r##"...##"
    let mut violations = Vec::new();

    // Scan core and client for common gotcha patterns.
    let dirs = ["reachlock-core/src", "reachlock-client/src"];
    for dir in &dirs {
        for entry in walkdir::WalkDir::new(dir) {
            let entry = match entry {
                Ok(e) => e,
                _ => continue,
            };
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let text = match std::fs::read_to_string(path) {
                Ok(t) => t,
                _ => continue,
            };
            for (i, line) in text.lines().enumerate() {
                // r#" ... "# pattern where the content contains "# 
                if line.contains("r#\"") && line.contains("\"#") && !line.contains("r##\"") {
                    violations.push(format!("{}:{}: possible raw string escape", path.display(), i+1));
                }
            }
        }
    }

    if violations.is_empty() {
        CheckResult::pass("gotcha-scan", "no gotcha patterns found")
    } else {
        CheckResult::fail("gotcha-scan", &violations.join("; "))
    }
}

/// Check 6: sprint-branch — branch name matches the sprint convention.
fn check_sprint_branch() -> CheckResult {
    let branch = match std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
    {
        Ok(out) => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        Err(_) => return CheckResult::fail("sprint-branch", "not a git repository"),
    };
    // Skip for testing/main/other non-sprint branches.
    if branch == "testing" || branch == "main" {
        return CheckResult::pass("sprint-branch", "on mainline branch (skipping)");
    }
    // Check pattern: sprint-v2/sXX-name
    if branch.starts_with("sprint-v2/") {
        CheckResult::pass("sprint-branch", &format!("branch {branch} follows convention"))
    } else {
        CheckResult::fail("sprint-branch", &format!("branch {branch} does not follow sprint-v2/sXX-name"))
    }
}

/// Check 7: module-registration — new pub mod declarations have matching files.
fn check_module_registration() -> CheckResult {
    let lib_rs = "reachlock-core/src/lib.rs";
    let text = match std::fs::read_to_string(lib_rs) {
        Ok(t) => t,
        Err(_) => return CheckResult::fail("module-registration", "cannot read lib.rs"),
    };
    let mut missing = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        // Match: pub mod X; or pub mod X { OR pub mod X; // comment
        if let Some(rest) = trimmed.strip_prefix("pub mod ") {
            let mod_name = rest.split(|c: char| c.is_whitespace() || c == ';' || c == '{')
                .next()
                .unwrap_or("");
            if !mod_name.is_empty() {
                // Check for corresponding file.
                let file_path = format!("reachlock-core/src/{mod_name}.rs");
                let dir_path = format!("reachlock-core/src/{mod_name}/mod.rs");
                if !std::path::Path::new(&file_path).exists()
                    && !std::path::Path::new(&dir_path).exists()
                {
                    missing.push(mod_name.to_string());
                }
            }
        }
    }
    if missing.is_empty() {
        CheckResult::pass("module-registration", "all modules have files")
    } else {
        CheckResult::fail("module-registration", &format!("missing files for: {}", missing.join(", ")))
    }
}

// Helpers

impl CheckResult {
    pub fn pass(name: &'static str, detail: &str) -> Self {
        CheckResult { name, passed: true, detail: detail.into() }
    }
    pub fn fail(name: &'static str, detail: &str) -> Self {
        CheckResult { name, passed: false, detail: detail.into() }
    }
}
