# S120 — Content Pipeline Bypasses (M21, M22)

**Wave: Hotfix · Depends on:** S01 (Content pipeline), S109 (Crews ContentType)

## Outcome

Two content loading paths that bypass the `ContentIndex` are rewritten to use it:
- **M21**: Crew packages loaded via `read_dir("mods/reachlock/crews")` → use `ContentIndex`
- **M22**: Soul mutations loaded via `read_dir("mods/reachlock/storylines")` → use `ContentIndex`

## Fix 1 — M21: Load crew packages via ContentIndex

**File:** `reachlock-client/src/systems/crew.rs` lines 273-291

**Current:** Direct filesystem scan of `mods/reachlock/crews/`:
```rust
let crews_dir = std::path::Path::new("mods/reachlock/crews");
if crews_dir.is_dir() {
    if let Ok(entries) = std::fs::read_dir(crews_dir) { ... }
}
```

**Fix:** Read crew packages from `ContentIndex` instead:

```rust
pub fn load_from_content(
    content: &ContentIndex,
    souls: &BTreeMap<String, SoulFile>,
) -> Self {
    let mut members = Vec::new();
    for file in &content.files {
        if let reachlock_core::content::ContentPayload::CrewPackage(pkg) = &file.payload {
            for entry in &pkg.members {
                if !entry.starting { continue; }
                // ... (rest of the member-construction logic, unchanged)
            }
        }
    }
    // ...
}
```

This removes the hardcoded `read_dir` call entirely. Crew packages come from the content index, which already scanned all content directories.

If `ContentPayload` doesn't have `CrewPackage` variant, add it to `reachlock-core/src/content/envelope.rs`:
```rust
pub enum ContentPayload {
    // ... existing variants ...
    CrewPackage(crew::CrewPackage),
}
```

Also add `CrewPackage` to `AssetType` if needed.

---

## Fix 2 — M22: Load soul mutations via ContentIndex

**File:** `reachlock-client/src/systems/soul.rs` lines 121-145

**Current:** Direct filesystem scan of `mods/reachlock/storylines/` with CWD-dependent path:
```rust
for root in ["mods/reachlock/storylines", "../mods/reachlock/storylines"] {
    let Ok(entries) = std::fs::read_dir(root) else { continue; };
    // ...
    if let Ok(mutations) = ron::from_str::<Vec<SoulMutation>>(&text) {
        registry.mutations.extend(mutations);
    }
}
```

**Fix:** Read from `ContentIndex` instead. Soul mutations should be a `ContentPayload` variant:

```rust
pub fn init_souls(content: Res<ContentIndex>, mut registry: ResMut<SoulRegistry>) {
    // ... existing soul loading from content.files ...

    // Load mutations from content index
    for file in &content.files {
        if let reachlock_core::content::ContentPayload::SoulMutations(mutations) = &file.payload {
            registry.mutations.extend(mutations.clone());
        }
    }
}
```

Add `SoulMutations(Vec<SoulMutation>)` to `ContentPayload` if it doesn't exist.

---

## Acceptance gates

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings

# Verify crew roster loads from content index
cargo run -p reachlock-client  # check log output: "loaded N member(s) from content"

# Verify soul mutations load from content index
cargo run -p reachlock-client  # check log: "loaded N authored soul(s)"

make check
```

## Gotchas

- **M21: `ContentPayload` may need a `CrewPackage` variant.** If it doesn't exist, add it to both `ContentPayload` enum and `AssetType`. Also register it in the content loader in `content_index.rs`.
- **M22: `SoulMutations` wrapper.** The mutation arcs file was a bare `Vec<SoulMutation>` (without `ContentFile` envelope). After S115 (M15 fix), the file should be wrapped in `ContentFile`. If M15 hasn't been merged yet, coordinate: wrap the file first (M15), then update the reader (M22).
- **Both: The `content_index.rs` loader must recognize the new payload types.** Check `content/mod.rs` or `content_index.rs` for the `match payload { ... }` block that dispatches `ContentPayload` variants. Add the new variants there.
- **CWD-dependent path `../mods/reachlock/storylines`.** After M22, this fallback path is deleted entirely. The content index uses the configured content root (from preferences/command-line), not CWD.
