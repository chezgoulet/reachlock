//! The resolver's whole point: find the content tree when the process was
//! not started from the repository root. This test binary lives under
//! `target/debug/deps/`, so resolution step 3 (walk up from the executable)
//! is what has to succeed once the CWD is useless.
//!
//! This file must contain EXACTLY ONE test. `set_current_dir` is
//! process-global and the root is cached on first use, so a second test in
//! this binary would race it.

#[test]
fn content_root_resolves_when_the_cwd_is_not_the_repo() {
    std::env::set_current_dir("/").expect("chdir to /");
    assert!(
        reachlock_core::paths::content_found(),
        "expected the executable walk to find mods/reachlock — {}",
        reachlock_core::paths::describe()
    );
    assert!(
        reachlock_core::paths::content_root()
            .join("origins")
            .is_dir(),
        "resolved content root has no origins/ — {}",
        reachlock_core::paths::describe()
    );
}
