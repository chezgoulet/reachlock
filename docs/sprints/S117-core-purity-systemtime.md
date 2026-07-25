# S117 — Core Purity: Remove SystemTime::now() (M11)

**Wave: Hotfix · Depends on:** None (core purity fix)

## Outcome

`generate_log_entry` in `reachlock-core` no longer calls `SystemTime::now()`. The timestamp is passed in as a parameter by callers (CLI, server). Iron rule #1 restored: core is pure.

## Context

**File:** `reachlock-core/src/agency/log_generation.rs` lines 41-44

```rust
generated_at: std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|d| d.as_secs())
    .unwrap_or(0),
```

`SystemTime::now()` reads the system clock, making `generate_log_entry` impure and non-deterministic. Core functions must be pure (iron rule #1). The fix pushes the wall-clock reading to the callers.

## Fix

### Step 1: Add parameter to the function signature

Find the signature of `generate_log_entry` in `log_generation.rs`. It likely looks like:

```rust
pub fn generate_log_entry(
    session_id: &str,
    title: &str,
    narrative: &str,
    narrator: &str,
    model: &str,
) -> Result<LogEntry, ...> {
```

Add `generated_at_secs: u64` parameter:

```rust
pub fn generate_log_entry(
    session_id: &str,
    title: &str,
    narrative: &str,
    narrator: &str,
    model: &str,
    generated_at_secs: u64,
) -> Result<LogEntry, ...> {
```

### Step 2: Use the parameter instead of SystemTime

Replace lines 41-44:

**Before:**
```rust
generated_at: std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|d| d.as_secs())
    .unwrap_or(0),
```

**After:**
```rust
generated_at: generated_at_secs,
```

### Step 3: Update all callers

Find every call to `generate_log_entry` and add the timestamp argument:

```bash
cd /home/c/git/chezgoulet/reachlock && rg "generate_log_entry" --include '*.rs'
```

For each caller, replace:
```rust
generate_log_entry(session_id, title, narrative, narrator, model)
```
with:
```rust
let now_secs = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|d| d.as_secs())
    .unwrap_or(0);
generate_log_entry(session_id, title, narrative, narrator, model, now_secs)
```

Callers are likely in:
- `reachlock-cli/src/` (generates log entries offline)
- `reachlock-server/src/services/` (generates log entries from LLM output)

### Step 4: Remove SystemTime import if no longer needed

If `generate_log_entry` was the only `SystemTime` user in the file, remove `use std::time::SystemTime;`.

## Acceptance gates

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings

# Verify no SystemTime::now() in core
cd reachlock-core && rg "SystemTime::now" && echo "FAIL: SystemTime found in core" || echo "PASS"

# Engine purity gate
make check-purity

make check
```

## Non-goals

- Making ALL core functions pure (only this one function)
- Timezone-aware timestamp handling (callers pass UTC seconds)

## Gotchas

- **The function may be called by the CLI `gen` command** which runs `generate_log_entry` as part of deterministic generation. Adding a parameter means the CLI must provide the timestamp. For determinism, use a fixed timestamp (e.g., `0` or the seed value) when generating deterministically.
- **Tests for `generate_log_entry` will need updating.** Any unit tests that call `generate_log_entry` must now pass `generated_at_secs`. Use `0` in tests.
- **`generated_at_secs: u64` is the `UNIX_EPOCH.duration_since(SystemTime::now()).as_secs()` value.** The field type on `LogEntry` is already `u64` (set by the previous code's `as_secs()` call). The parameter type matches.
- **`make check-purity` is the gate for iron rule #1.** After this fix, core should pass the purity check (assuming no other non-pure deps in core).
