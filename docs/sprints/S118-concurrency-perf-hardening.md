# S118 — Concurrency & Perf Hardening (M1, M2, M3, M5, M6, M7, M30)

**Wave: Hotfix · Depends on:** None (server + client fixes, independent)

## Outcome

Seven concurrency, performance, and correctness issues fixed across server and client:
- **M1**: Argon2 hashing moved outside Mutex
- **M2**: Broadcast channel buffer increased to prevent stall
- **M3**: Duplicate session detection prevents sender overwrite
- **M5**: Share code collision retry loop
- **M6/M7**: Lock ordering documented and standardized
- **M30**: ShipInterior clone optimized (cloned only on change)

---

## Fix 1 — M1: Move Argon2 outside Mutex

**File:** `reachlock-server/src/services/auth.rs` ~line 355

The password hashing happens inside `self.players.lock()`. Argon2 is CPU-intensive (~100ms per hash). Holding the lock during hashing blocks ALL other auth operations (login, lookup, by_id, by_email).

**Fix:** Refactor the function that hashes passwords to:
1. Acquire `self.players.lock()` for the minimum duration (just read/write operations)
2. Hash the password OUTSIDE the lock
3. Re-acquire for the write if needed

Find the registration/login function that calls the argon2 hash. Pattern:

```rust
// Before (pseudocode):
let mut players = self.players.lock().unwrap();
let hash = argon2::hash_encoded(password.as_bytes(), salt, &config)?;  // SLOW inside lock
players.insert(id, record);

// After:
let (salt, config) = {
    let players = self.players.lock().unwrap();
    // Read existing salt or generate new one
    (existing_salt_or_new, ARGON2_CONFIG)
}; // lock dropped
let hash = argon2::hash_encoded(password.as_bytes(), &salt, &config)?;
let mut players = self.players.lock().unwrap();
players.insert(id, record);  // fast, lock held briefly
```

---

## Fix 2 — M2: Increase broadcast channel capacity

**File:** `reachlock-server/src/ws/handler.rs` line 50

**Current:**
```rust
let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<ServerMessage>(64);
```

If the writer task (spawned at ~line 65) blocks because the channel is full, ALL broadcasts to this player stall. With 64 messages and many universe events, a slow consumer can fill the buffer quickly.

**Fix:** Increase to 256:

```rust
let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<ServerMessage>(256);
```

Alternatively, use `tokio::sync::mpsc::unbounded_channel()` if the server can tolerate unbounded memory (messages are small JSON strings; the tradeoff is memory vs latency).

For a bounded channel with backpressure handling, add `out_rx.try_recv()` in the writer loop with a log-warn on full buffer:

```rust
// In the writer task:
tokio::select! {
    Some(msg) = out_rx.recv() => { /* send as before */ }
    _ = tokio::time::sleep(Duration::from_secs(1)) => {
        tracing::warn!(player = %session.player_id, "broadcast buffer full — messages dropped");
        continue;
    }
}
```

**Minimum fix:** Change `64` to `256`. Full backpressure handling is optional.

---

## Fix 3 — M3: Detect duplicate sessions

**File:** `reachlock-server/src/ws/handler.rs` lines 92-97

**Current:**
```rust
state
    .player_senders
    .write()
    .await
    .insert(session.player_id.clone(), out_tx.clone());
```

If the same `player_id` connects from a second WebSocket, the first connection's sender is silently replaced. Targeted messages (voice signaling, direct messages) go to the NEW connection only. The old connection can still receive broadcast events but misses targeted ones.

**Fix:** Check for existing sender before inserting:

```rust
let mut senders = state.player_senders.write().await;
if let Some(old_tx) = senders.get(&session.player_id) {
    tracing::warn!(
        player = %session.player_id,
        "duplicate session detected — closing previous connection"
    );
    // Send a kick message to the old connection
    let _ = old_tx.send(ServerMessage::Error {
        message: "duplicate connection — you connected from another session".into(),
    }).await;
    // The old connection will close naturally when it fails to send
}
senders.insert(session.player_id.clone(), out_tx.clone());
```

---

## Fix 4 — M5: Share code collision retry

**File:** `reachlock-server/src/services/library.rs` lines 75-90

**Current:**
```rust
fn generate_share_code() -> String {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    // ...
}
```

Two concurrent calls in the same nanosecond produce identical codes.

**Fix:** Add a collision check with retry (max 5 attempts):

```rust
fn generate_share_code(existing_codes: &HashSet<String>) -> String {
    let chars: Vec<char> = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789".chars().collect();
    for _ in 0..5 {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut h = nanos;
        let mut code = String::with_capacity(8);
        for _ in 0..8 {
            code.push(chars[(h as usize) % chars.len()]);
            h = h.wrapping_mul(6364136223846793005); // better bit mixing
        }
        if !existing_codes.contains(&code) {
            return code;
        }
    }
    // Fallback: use incrementing suffix
    format!("{:08X}", nanos as u32)
}
```

Update the function signature to accept `existing_codes`. Update the caller to pass the current code set.

---

## Fix 5 — M6: Document lock ordering in library.rs

**File:** `reachlock-server/src/services/library.rs` lines 209-253

**Fix:** Add a doc comment at the top of the `MemoryContractLibrary` struct or the publish function:

```rust
/// Lock ordering (acquire in this order to prevent deadlocks):
/// 1. `self.contracts` (RwLock)
/// 2. `self.indices` (RwLock)
/// 3. `self.shares` (RwLock)
///
/// Never acquire locks out of this order.
pub struct MemoryContractLibrary {
    // ...
}
```

Do NOT change the lock acquisition code — just document it. If the code already follows a consistent order, no functional change.

---

## Fix 6 — M7: Standardize lock order in auth.rs

**File:** `reachlock-server/src/services/auth.rs` lines 364-374, 380-384

**Current:** `by_login` acquires `players` then `by_email`. `by_email` acquires `by_email` then `players`. This is inverted.

**Fix:** Both functions must follow the same order: `players` first, `by_email` second.

**`by_email` at line 380 — fix to:**
```rust
fn by_email(&self, email: &str) -> Option<PlayerRecord> {
    // Standard lock order: players → by_email
    let players = self.players.lock().unwrap();
    let emails = self.by_email.lock().unwrap();
    let pid = emails.get(email)?;
    players.get(pid).cloned()
}
```

This acquires `players` FIRST, then `by_email`, matching `by_login`'s order.

Add a comment above BOTH functions:
```rust
// Lock order: self.players → self.by_email (never the reverse)
```

---

## Fix 7 — M30: Avoid ShipInterior clone every frame

**File:** `reachlock-client/src/systems/crew.rs` line 1090

**Current:**
```rust
let deck_interior = roster.current_interior.clone();
```

This clones the entire `ShipInterior` (multiple `Vec<Room>`, `Vec<Door>`, etc.) every frame at 60fps. The interior only changes when the player docks or refits the ship.

**Fix:** Clone only when the interior changes. Use Bevy's change detection:

```rust
// At the top of crew_shift_system, before the clone:
// Cache the interior and only clone when roster changes
```

Simplest fix: add a `Local<Option<(ShipInterior, usize)>>` cache:

```rust
fn crew_shift_system(
    // ... existing params ...
    mut cached_interior: Local<Option<reachlock_core::generator::ship::ShipInterior>>,
) {
    // Only clone when interior actually changed
    if cached_interior.as_ref().map_or(true, |c| *c != roster.current_interior.as_ref().unwrap_or(&default_interior)) {
        *cached_interior = roster.current_interior.clone();
    }
    let deck_interior = cached_interior.clone().unwrap_or_default();
```

Better: use `Arc<ShipInterior>` on `CrewRoster` (changes the struct definition):

```rust
// In crew.rs:
pub struct CrewRoster {
    pub members: Vec<CrewMember>,
    pub current_interior: Option<Arc<ShipInterior>>,
}
```

Then `clone()` on the `Arc` is cheap (reference count increment). Every frame at 60fps.

**Minimum fix:** The `Local` cache approach — no struct changes, just avoids the clone when interior is unchanged.

## Acceptance gates

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings

make check
```

## Non-goals

- Full mutex-async migration (keeping current std::sync::Mutex pattern)
- Channel architecture redesign (band-aid: increased buffer size)
- Full deadlock prevention audit (just the two documented cases)

## Gotchas

- **M1: The Argon2 config must be accessible outside the lock.** If the config is created inside the lock, extract it to a module-level constant or struct field.
- **M2: Channel size 256.** This increases memory by ~1KB per connection (256 × ~40 bytes per message). For 1000 players, that's 1MB. Acceptable.
- **M3: The `ServerMessage::Error` variant may not exist.** Check the `ServerMessage` enum in `reachlock-core/src/network/messages.rs`. Use whatever message variant represents a kick/disconnect notification. If none exists, just warn and let the old connection time out naturally.
- **M5: The `existing_codes` set.** The caller must have access to the current share codes. This may be the `MemoryContractLibrary.shares` field. Pass it as a parameter.
- **M6/M7: Documentation doesn't prevent bugs.** But documenting the order makes the next developer aware. A real fix would use deadlock detection or a `tracing` span with lock-ordering.
- **M30: `#[serde(skip)]` on `current_interior`.** If converting to `Arc<ShipInterior>`, ensure serde still works. `Arc<T>` serializes identically to `T`. No migration needed.
