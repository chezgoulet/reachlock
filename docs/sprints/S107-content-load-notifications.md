# S107 — Content Load Failure Notifications

**Wave: UX-Hardening · Depends on:** S01 (Content pipeline), S85 (Discovery permanence / notification queue)

## Outcome

Content files that fail to parse during startup are reported to the player as visible in-game notifications instead of silent `warn!` log messages. The `NotificationQueue` resource (already exists in `discovery.rs`) is used to push toast messages for each failed content file.

## Context

When `content_index.rs` loads RON files from `mods/reachlock/`, parse failures are logged but never surfaced:

```rust
Err(err) => warn!("content index: bad manifest {}: {err}", manifest_path.display()),
```

Similarly, `crew.rs` silently skips unparseable crew packages:
```rust
Err(e) => warn!("crew: failed to parse {}: {e}", path.display()),
```

And `dispatch.rs` skips content files that don't match any known payload type:
```rust
// Silently skipped — the file is dead content but the player never knows
```

A broken file means a station, soul, faction, or crew member is missing from the game. The player notices the absence (empty station, missing NPC) but never the cause.

### Key files

| File | Role |
|------|------|
| `reachlock-client/src/systems/content_index.rs` | Content loading — capture parse errors |
| `reachlock-client/src/systems/crew.rs` | Crew package loading — capture parse errors |
| `reachlock-client/src/systems/dispatch.rs` | Content dispatch — capture unknown payload types |
| `reachlock-client/src/systems/discovery.rs` | `NotificationQueue` — already exists, push to it |

## Freeze first

### Content error notification data

```rust
/// Queued for player-visible notification when content fails to load.
pub struct ContentErrorNotification {
    pub path: String,
    pub reason: String,
    pub severity: ContentErrorSeverity,
}

pub enum ContentErrorSeverity {
    /// Content is missing but the game can continue (e.g., optional mod).
    Warning,
    /// Content required by the current origin/save — game may be degraded.
    Error,
}
```

### Notification text format

```
⚠ Content error: souls/hero.ron — expected struct SoulFile, found struct ItemSeed
⚠ Mod failed: my_faction_mod/manifest.ron — unknown field 'depends_on'
✘ Missing asset: hulls/loup_garou.ron — referenced by origin 'Loup-Garou Veteran'
```

Shown as toasts in the top-right corner, auto-dismissing after 8 seconds.

## Deliverables

### 1. Add error collection to ContentIndex

- [ ] In `content_index.rs`: add a `Vec<ContentErrorNotification>` to `ContentIndex`
- [ ] On each parse failure during `load_content_index`: push an error with the file path and parse error message
- [ ] Ditto for mod manifest parse failures in `resolve_load_order`
- [ ] Ditto for individual `.ron` file parse failures

### 2. Add error collection to CrewRoster loading

- [ ] In `crew.rs` `load_from_content()`: on parse failure, push a `ContentErrorNotification`
- [ ] On missing soul reference (crew member references a soul_id that doesn't exist in `souls`): push a warning

### 3. Add error collection to dispatch

- [ ] In `dispatch.rs`: when a `ContentFile` has a payload variant that no consumer recognizes, push a warning
- [ ] This replaces the silent skip — the file is still skipped, but the player knows why

### 4. Push to NotificationQueue on game enter

- [ ] In `OnEnter(AppState::InGame)` system chain (after content loading):
  - A new system `report_content_errors` reads `ContentIndex` errors and pushes them to `NotificationQueue`
  - The `NotificationQueue` already renders as toasts via `process_notifications` in `discovery.rs`
- [ ] Each error becomes one toast notification

### 5. Severity-gated display

- [ ] `Warning` level errors show as yellow toasts
- [ ] `Error` level errors show as red toasts and persist longer (12s vs 8s)
- [ ] If total error count > 5, show a summary toast: `"6 content errors — check logs for details"`
- [ ] If total error count > 0, show a persistent badge in the HUD: `"{n} content errors"` (yellow)

### 6. Test

- [ ] Create a deliberately broken `.ron` file in a `mods/` test directory
- [ ] Launch the game → verify a toast appears with the file path and error reason
- [ ] Fix the file → relaunch → no error toast

## Acceptance gates

```bash
cargo clippy -p reachlock-client -- -D warnings

# Manual:
# 1. Create a broken .ron file in mods/reachlock/souls/broken.ron
# 2. Launch game → see toast: "Content error: souls/broken.ron — ..."
# 3. Remove the broken file → relaunch → no toast
# 4. Create a broken mod manifest → see toast for the manifest

make check
```

## Non-goals

- In-game content file editor (you can't fix RON from inside the game)
- Auto-reload fixed content (requires restart)
- Content error dashboard (just toasts)
- Granular error categorization (no need to distinguish "type mismatch" from "missing field")

## Gotchas

- **`NotificationQueue` is in `discovery.rs`.** It's designed for discovery notifications ("New system charted: Aethon") but works for any toast. Push content errors the same way.
- **Toast spam.** If 50 content files fail, 50 toasts in sequence would take minutes. Cap at 5 toasts total from content errors; show the summary after the first 5.
- **`ContentIndex` is created in `load_content_index` (Startup).** The error collection must be part of the resource initializer, not a separate system. Fill errors during `load_content_index` and store them on the `ContentIndex` struct.
- **Don't add `ContentErrorNotification` to core.** It's a client-side concern. Define it in `client/src/systems/content_index.rs` or a new `content_errors.rs`.
- **The `ContentIndex` struct already has a `files: Vec<ContentFile>` field.** Add `errors: Vec<ContentErrorNotification>` — minimal struct expansion.
