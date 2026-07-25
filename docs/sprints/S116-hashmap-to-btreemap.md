# S116 — HashMap → BTreeMap in Core (M9, M10, M25, M26)

**Wave: Hotfix · Depends on:** None (determinism fixes in core)

## Outcome

Four `HashMap` occurrences in core types are replaced with `BTreeMap`. HashMaps have non-deterministic iteration order, which means serialize→deserialize→iterate can produce different results across platforms. BTreeMap guarantees sorted order on every platform.

## Fix 1 — M9: Career ActiveCareerPath.progress

**File:** `reachlock-core/src/career/mod.rs` line 159

**Before:**
```rust
pub progress: HashMap<ProgressionCriterionType, u64>,
```

**After:**
```rust
pub progress: BTreeMap<ProgressionCriterionType, u64>,
```

Replace the `use std::collections::HashMap;` import with `BTreeMap` (or add the import if there are other `HashMap` uses in the file). Progression criteria are iterated during career advancement checks — iteration order must be deterministic.

---

## Fix 2 — M10: PiracyState.pirate_reputation

**File:** `reachlock-core/src/career/piracy.rs` line 22

**Before:**
```rust
pub pirate_reputation: HashMap<String, i64>,
```

**After:**
```rust
pub pirate_reputation: BTreeMap<String, i64>,
```

Same import substitution as M9.

---

## Fix 3 — M25: Planet generation FactionMap

**File:** `reachlock-core/src/generator/planet_extended.rs` line 21

**Before:**
```rust
pub type FactionMap = HashMap<FactionId, u8>;
```

**After:**
```rust
pub type FactionMap = BTreeMap<FactionId, u8>;
```

This type is used during planet culture generation. If faction influence is iterated in any order-dependent way, the generated planet differs between runs.

---

## Fix 4 — M26: Agency log pair_count

**File:** `reachlock-core/src/agency/log.rs` lines 130-131

**Before:**
```rust
let mut pair_count: std::collections::HashMap<(String, String), u32> =
    std::collections::HashMap::new();
```

**After:**
```rust
let mut pair_count: std::collections::BTreeMap<(String, String), u32> =
    std::collections::BTreeMap::new();
```

This HashMap is iterated to detect recurring argument patterns in crew relationship deltas. Iteration order affects which patterns are detected first.

---

## Determinism check

After all four changes, verify determinism manifests haven't changed:

```bash
cargo run -p reachlock-cli -- determinism check
```

If any golden manifest changed, re-emit and commit:
```bash
cargo run -p reachlock-cli -- determinism emit
# Compare to existing goldens, update if needed
```

**Note:** Changing HashMap→BTreeMap changes serialization order. If any of these structs are part of the determinism manifest, the golden file will change. This is intentional — BTreeMap produces a stable, deterministic order.

## Acceptance gates

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings

# Determinism gate
cargo run -p reachlock-cli -- determinism check

make check
```

## Non-goals

- Changing client-side HashMap usage (client-side only — no determinism requirement)
- Replacing all HashMap in the codebase (only the 4 core types with determinism impact)
- Converting to `IndexMap` or other ordered map (BTreeMap is sufficient and standard library)

## Gotchas

- **`ProgressionCriterionType` must implement `Ord`.** If it doesn't already, add `Ord` to the derive list. Check the current derives: if it has `PartialEq, Eq, PartialOrd, Ord` or just `PartialEq, Eq`, you may need to add `PartialOrd, Ord`.
- **`FactionId` is a newtype.** Check if it implements `Ord`. Newtypes typically derive it if the inner type does. `FactionId(String)` — `String` implements `Ord`, so `#[derive(Ord)]` should work.
- **`(String, String)` tuple implements `Ord`.** No additional derives needed for the pair_count key.
- **Serialization format.** Both `HashMap` and `BTreeMap` serialize to RON/JSON identically (both serialize as maps). No migration needed. Deserialization "just works" — `serde` doesn't distinguish the map type.
- **Check callers.** These types are in core and consumed by the client and CLI. `cargo build --workspace` catches any compilation issues from the type change.
