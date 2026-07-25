# S122 — Low-Severity Cleanup Batch (L1–L18)

**Wave: Hotfix · Depends on:** None (standalone cleanup, 18 small items)

## Outcome

Eighteen low-severity code quality, dead code, and hygiene issues resolved. Each fix is ≤5 lines. All are safe to apply simultaneously — no dependencies between them.

---

## Client fixes (L8, L9, L10)

### L8 — Duplicate FocusRing declaration in gamepad.rs

**File:** `reachlock-client/src/systems/gamepad.rs` line 7

Delete line 7:
```rust
pub struct FocusRing;  // DELETE — duplicate of focus_ring.rs:12
```

`FocusRing` in `gamepad.rs` is an empty unit struct never used. The real `FocusRing` is in `focus_ring.rs:12`.

### L9 — Dead render_storyline_log not registered in main.rs

**File:** `reachlock-client/src/systems/storyline_driver.rs` line 85

`render_storyline_log` is defined but never added to any Bevy schedule. Either:

**Option A (register it):** Add to `main.rs`:
```rust
.add_systems(Update, storyline_driver::render_storyline_log.run_if(in_state(AppState::InGame)))
```

**Option B (delete it):** Remove the function and `StorylineLogVisible` resource. Also remove `.init_resource::<StorylineLogVisible>()` from `main.rs` line 246.

Recommended: Option A — register it. The function exists and the resource is initialized.

### L10 — Duplicate CrewRoster by_id/get methods

**File:** `reachlock-client/src/systems/crew.rs` lines 348, 376

`by_id()` and `get()` do the same thing (`self.members.iter().find(|m| m.id == id)`).

Delete `get()` at line 376 (it's the second one). Update callers of `get()` to use `by_id()`. Search for `.get(` callers in the same file.

```bash
cd /home/c/git/chezgoulet/reachlock && rg "\.get\(" reachlock-client/src/systems/crew.rs
```

Replace `roster.get(id)` → `roster.by_id(id)` in all call sites.

---

## Server fixes (L1, L2, L3, L4, L5, L6, L7)

### L1 — records.remove(0) is O(N); use VecDeque

**File:** `reachlock-server/src/services/cost.rs` line 62

If the code does `records.remove(0)`, replace with `VecDeque::pop_front()`.

```rust
// Before:
pub struct MemoryCostStore { records: Vec<LlmCallRecord> }
records.remove(0);

// After:
pub struct MemoryCostStore { records: VecDeque<LlmCallRecord> }
records.pop_front();
```

If `records` is iterated or serialized, `VecDeque` supports both. Change the type and the `.remove(0)` call.

### L2 — 4 separate mutex acquisitions per LLM request in quota.rs

**File:** `reachlock-server/src/services/quota.rs`

If the code does multiple `self.xxx.lock().unwrap()` calls in sequence, consolidate into a single lock scope:

```rust
// Before:
let a = self.field_a.lock().unwrap();
let b = self.field_b.lock().unwrap();
let c = self.field_c.lock().unwrap();
let d = self.field_d.lock().unwrap();

// After:
let (a, b, c, d) = {
    let a = self.field_a.lock().unwrap();
    let b = self.field_b.lock().unwrap();
    let c = self.field_c.lock().unwrap();
    let d = self.field_d.lock().unwrap();
    (a, b, c, d)
};
```

Same lock count but clearer that they're acquired together. If the fields can be combined into a single struct behind one Mutex, that's better — but out of scope for a low-severity fix.

### L3 — MemoryAuditLog grows without bound

**File:** `reachlock-server/src/services/audit.rs`

Add a maximum size cap:

```rust
const MAX_AUDIT_ENTRIES: usize = 10_000;

pub fn record(&self, entry: AuditEntry) {
    let mut log = self.entries.lock().unwrap();
    log.push(entry);
    if log.len() > MAX_AUDIT_ENTRIES {
        log.drain(0..(log.len() - MAX_AUDIT_ENTRIES / 2)); // trim oldest half
    }
}
```

### L4 — degradation.rs never called

**File:** `reachlock-server/src/services/degradation.rs`

Either:
- **Delete the module** (if not intended to be used): remove `pub mod degradation;` from `services/mod.rs`, delete `degradation.rs`
- **Wire it into startup** (if intended): add a `degradation::start_periodic_check(state.clone())` to `main.rs`

Recommended: keep the file (S26 infrastructure) but add `#[allow(dead_code)]` and a doc comment `// S26: wired when postgres health checks are added`.

### L5 — shutdown.rs never used

**File:** `reachlock-server/src/services/shutdown.rs`

Same as L4. Either delete or document as planned infrastructure:
```rust
//! Graceful shutdown handler. Not yet wired — main.rs uses inline shutdown.
//! S26: replace inline shutdown with this module.
#![allow(dead_code)]
```

### L6 — Account deletion cron is a no-op

**File:** `reachlock-server/src/main.rs` lines 53-62

The spawned task does nothing except `drop(config)` in a loop. Either:

**Fix:** Make it actually purge expired accounts:
```rust
tokio::spawn(async move {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
    loop {
        interval.tick().await;
        let grace_period = state.auth_config.read().unwrap().deletion_grace_period_days;
        // Call state.players.purge_expired(grace_period) or equivalent
    }
});
```

Or add `// TODO: PgPlayerStore batch purge when Postgres is the primary store` comment.

### L7 — Stub admin endpoints return fake success

**File:** `reachlock-server/src/ws/admin.rs` lines 269-289

`admin_tick_trigger` and `admin_content_purge` return `{"tick": "triggered"}` / `{"purged": true}` without actually doing anything.

Add a response field `"implemented": false` or wire the actual tick trigger / content purge logic. For now:
```rust
async fn admin_tick_trigger(...) -> impl IntoResponse {
    // TODO: S73 wire actual tick trigger
    (StatusCode::NOT_IMPLEMENTED, Json(json!({"error": "not yet implemented"})))
}
```

---

## CLI fixes (L13)

### L13 — JSON output without string escaping

**File:** `reachlock-cli/src/main.rs` line 84

**Current:**
```rust
print!("  {{\"name\":\"{}\",\"passed\":{},\"detail\":\"{}\"}}", r.name, r.passed, r.detail);
```

**Fix:** Use `serde_json`:
```rust
let json = serde_json::json!({
    "name": r.name,
    "passed": r.passed,
    "detail": r.detail,
});
print!("  {}", serde_json::to_string(&json).unwrap());
```

Or use `serde_json::to_string` to properly escape special characters in names.

---

## Core fixes (L11, L12)

### L11 — Undocumented OnceLock global state in faction

**File:** `reachlock-core/src/faction/mod.rs` lines 26-27

Add a doc comment explaining the OnceLock:

```rust
/// Global faction registry populated at startup.
/// OnceLock ensures it's only initialized once (content loading).
/// Thread-safe read access for all subsequent lookups.
static FACTION_REGISTRY: OnceLock<HashMap<String, Faction>> = OnceLock::new();
```

### L12 — #[allow(dead_code)] on reachable method

**File:** `reachlock-core/src/galaxy/gate.rs` line 47

If `gate_by_index` is actually called, remove `#[allow(dead_code)]`. If it's not called but is part of the public API, add a test:

```rust
#[test]
fn gate_by_index_lookup() {
    let gates = vec![...];
    assert_eq!(gate_by_index(&gates, 0), Some(&gates[0]));
}
```

---

## Documentation fixes (L14, L15)

### L14 — AGENTS.md: cargo clippy missing --workspace --all-targets

**File:** `AGENTS.md` line 68

Change:
```
cargo clippy -- -D warnings    # CI gate
```
to:
```
cargo clippy --workspace --all-targets -- -D warnings    # CI gate
```

### L15 — AGENTS.md: Missing reachlock-editor/ from repository layout

**File:** `AGENTS.md` lines 16-23

Add to the repository layout table:
```
| `reachlock-editor/` | Egui content editor — 25+ editors, AI generation, seed panel |
```

---

## Content fixes (L16, L17, L18)

### L16-L18 — Empty object schemas

**Files:**
- `mods/reachlock/schemas/theme.schema.json`
- `mods/reachlock/schemas/ecosystem.schema.json`
- `mods/reachlock/schemas/planet_culture.schema.json`

If these are empty `{}` objects, either:
- Generate proper schemas from the Rust types (like S53 did for other types)
- Or add a `"description": "Schema pending — see Rust type for validation rules"` comment

Minimum fix: add the description field so tooling doesn't silently accept everything.

---

## Acceptance gates

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings

# L13: CLI JSON output is valid
cargo run -p reachlock-cli -- agent-check --json | python3 -m json.tool > /dev/null && echo "JSON valid"

make check
```

## Non-goals

- Comprehensive refactoring of any module (each fix is a single, localized change)
- Full feature implementation for no-op stubs (only documentation or removal)
- Changing module structure

## Gotchas

- **L8: Deleting `FocusRing` from gamepad.rs may break compilation if it's referenced elsewhere.** Search: `rg "gamepad::FocusRing"` — if no references, safe to delete.
- **L9: Unused import warnings.** If `StorylineLogVisible` is no longer used after deleting `render_storyline_log`, remove its import from `main.rs` (line 246) and `storyline_driver.rs`.
- **L1: VecDeque serialization.** If `MemoryCostStore` is serialized, `VecDeque` serializes the same as `Vec<T>`. No migration needed.
- **L13: serde_json may already be a dependency.** The CLI already uses it for content validation schemas. Check `Cargo.toml`.
- **L3: MAX_AUDIT_ENTRIES = 10,000.** Adjust based on expected audit volume. 10k entries at ~200 bytes each = 2MB. Acceptable in memory for a dev server.
