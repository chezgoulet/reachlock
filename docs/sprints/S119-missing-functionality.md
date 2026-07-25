# S119 — Missing Functionality: Market, Library, Redis (M16, M17, M18, M4)

**Wave: Hotfix · Depends on:** None (client + server standalone fixes)

## Fix 1 — M16: Implement handle_text_input (contract library)

**File:** `reachlock-client/src/systems/contract_library.rs` lines 484-488

**Current:** Empty no-op function:
```rust
fn handle_text_input(keys: &Res<ButtonInput<KeyCode>>, state: &mut ContractLibraryState) {
    // Text input handler placeholder.
}
```

**Fix:** Implement real text input by consuming `MessageReader<KeyboardInput>` events. Follow the pattern from `dialogue.rs:172-213` (free-text typing mode). Add `MessageReader<KeyboardInput>` parameter. Read character by character into the active text buffer (search/import/share/publish). Enter commits, Escape clears. Add `TextField` enum and `active_text_field: Option<TextField>` to `ContractLibraryState`.

The full fix spans ~80 lines. Reference the dialogue.rs pattern at `reachlock-client/src/systems/dialogue.rs` lines 172-213 for the exact implementation.

---

## Fix 2 — M17: Market buy/sell use EditorConfirm instead of wrong panel keys

**File:** `reachlock-client/src/systems/market.rs` lines 99, 119

**Current:** Buy uses `OpenMarketPanel` key, sell uses `OpenMissionBoard` key.

**Fix:** Change both to `InputAction::EditorConfirm` (Enter key). The market panel is already open — Enter commits the selected action:

```rust
// Line 99 — was: keys.just_pressed(settings.key(InputAction::OpenMarketPanel))
if keys.just_pressed(settings.key(InputAction::EditorConfirm)) {
    if state.mode == MarketMode::Buy {
        // ... existing buy logic ...
    } else {
        // ... existing sell logic ...
    }
}
```

Delete the separate sell block at line 119 (merge into the Enter handler above).

---

## Fix 3 — M18: Market .get() instead of [] index

**File:** `reachlock-client/src/systems/market.rs` lines 96-97

**Current:** `stations[&loc.station_id]` — panics on unknown station.

**Fix:** Replace with `.get()`:
```rust
let Some(station) = ticker.state.economy.stations.get(&loc.station_id) else {
    return;
};
let buy_quote = station.buy_price(&good, tariff_num);
let sell_quote = station.sell_price(&good, tariff_num);
```

---

## Fix 4 — M4: Redis session count via SCAN instead of DBSIZE

**File:** `reachlock-server/src/services/redis.rs` line 153

**Current:** `DBSIZE` counts ALL keys.

**Fix:** Use `SCAN session:*` pattern matching, or `KEYS session:*` for dev:

```rust
fn active_sessions(&self) -> usize {
    self.pool.block_on(move |mut mgr| async move {
        let keys: Vec<String> = redis::cmd("KEYS")
            .arg("session:*")
            .query_async(&mut mgr)
            .await
            .unwrap_or_default();
        keys.len()
    })
}
```

Add `// TODO: SCAN for production` comment.

## Acceptance gates

```bash
cargo build --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && make check
```
