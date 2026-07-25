# S85 — Discovery Permanence

**Spec:** §4 (exploration & discovery), §12 (universe tick) · **Wave E (shared world & distribution)** · **Depends on:** S81 (content dispatch — discovery attribution data flows through content dispatch for the system info panel)

## Outcome

When a player scans an uncharted system and the server confirms first-write-wins, that discovery is permanent. The system displays "Discovered by ⟨player⟩" in every player's galaxy map and system info panel. A discovery log panel lists every system the player has charted, with timestamps and galaxy coordinates. The exploration career tracks discovery count as a progression metric. A toast notification confirms the discovery the moment the server responds. The player's name on the system is their mark on the galaxy — the exploration career's permanent reward.

## Context

- The hard part is done: `SeedStore::discover()` in `reachlock-server/src/services/seed.rs` implements atomic first-write-wins. `discoverer_id` (a `String` field for the player's public name) is persisted per system in the seed store and the Postgres `systems` table. The server returns the `discoverer_id` in its response to a scan.
- S64 (discovery trigger in `sensors.rs`) fires when the player scans a system. The client sends a `ClientMessage::SeedDiscover` and receives a `ServerMessage::SeedDiscovered` with the discoverer_id. Currently the client receives this data but only logs it — it doesn't display it anywhere.
- The galaxy map (`galaxy_map.rs`) renders charted systems as dots on the grid. Each dot has a `ChartedSystem { id, coord, name, seed }` but no `discoverer_name` field. The map needs to display attribution.
- The discovery panel (`discovery.rs`) currently shows ecosystem data for the current planet — species cards and biome summaries. Its OpenDiscoveryPanel keybind (from S31's InputAction enum) is `OpenDiscoveryPanel` (bound to `U`). This sprint ADDS a discovery log view to the same panel, toggled with a sub-tab.
- S42 (career progression) defined the Exploration career track. Discovery count is listed in `career.rs`'s `CareerProgression::exploration.systems_discovered` field. The career system has the counter but no code increments it from the discovery workflow.
- S81's `ContentDispatcher` consumed `ContentPayload::ChartedSystem` files but didn't add runtime attribution — the content pipeline loads authored charted systems from disk, which have no discoverer (they're pre-charted by the content author). Dynamically discovered systems during gameplay are not content files; they come from the seed store via the network.

## Freeze first

### Extended system info data

```rust
// reachlock-core/src/seed/types.rs additions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub id: SystemId,
    pub name: String,
    pub discovered_by: Option<String>,   // player_name, None = uncharted
    pub discovered_at: Option<i64>,      // unix timestamp
    pub galaxy_coord: GalaxyCoord,
    pub seed: Seed,
    pub kind: SystemKind,               // star type, etc.
}
```

### Discovery attribution in charted systems

```rust
// reachlock-client/src/systems/galaxy_map.rs additions
#[derive(Debug, Clone)]
pub struct ChartedSystemExt {
    pub id: SystemId,
    pub name: String,
    pub coord: GalaxyCoord,
    pub discoverer_name: Option<String>,
    pub discovered_at: Option<i64>,
}
```

### Server message enrichment

The existing `ServerMessage::SeedDiscovered` already carries `discoverer_name: Option<String>`. The server's `seed.discover` handler returns the player's display name on first write and `Some(existing_name)` on subsequent reads. The client stores this in the charted systems map.

### Discovery log entry

```rust
// reachlock-client/src/systems/discovery.rs additions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryLogEntry {
    pub system_name: String,
    pub galaxy_coord: GalaxyCoord,
    pub discovered_at: i64,
    pub system_id: SystemId,
}
```

The discovery log is a `Resource: Vec<DiscoveryLogEntry>` persisted to the save file (`save/player.ron`) so the player's discoveries survive reload. The server does NOT store the player's discovery history — it only tracks the discoverer per system. The client builds the log from the player's own discovery events.

## Deliverables

### 1. Discovery attribution in system info panel (`systems/sensors.rs` or new info panel)

- [ ] When the player targets a system (via sensor scan or the system map selection), show a system info overlay that includes: system name, star type, planet count, and `"Discovered by {player_name}"` or `"Uncharted"` if `discovered_by` is `None`.
- [ ] The info overlay appears when the player presses `F` (CycleTarget) and a system is selected. Use the existing `ActiveContact` resource pattern from `sensors.rs` (the currently selected contact/highlighted system).
- [ ] The `discovered_by` value comes from the `KnownSystems` resource (expanded — add `discovered_by: Option<String>` and `discovered_at: Option<i64>` to each known system entry).
- [ ] When a new system is discovered (the `SeedDiscovered` response arrives), update the known system entry with the discoverer info.
- [ ] Offline: systems from the content pipeline (`ContentPayload::ChartedSystem`) have no discoverer — display as "Charting Authority" or omit the line. Systems discovered this session display the player name as soon as the server confirms.

### 2. Galaxy map attribution (`systems/galaxy_map.rs`)

- [ ] Extend `ChartedSystem` to include `discoverer_name` and `discovered_at`. The galaxy map's `charted_systems: HashMap<SystemId, ChartedSystem>` comes from the content index (authored) and the seed store (dynamically discovered). Both sources now carry `discoverer_name`.
- [ ] On the galaxy map, when the player selects a charted system (click or keyboard nav), show a tooltip/info line: `"⟨system name⟩ — charted by ⟨player⟩"` or `"⟨system name⟩ — pre-charted"` for authored systems.
- [ ] If the player charted the system themselves, show `"charted by YOU"` in a distinct color (green or yellow — differentiate from other players' discoveries).
- [ ] The galaxy map's projection and rendering code already computes positions and labels. Add the attribution text below the system name label, in a smaller font and muted color.
- [ ] Galaxy map attribution is visible to ALL players — not just the discoverer. This is the permanence reward: the discoverer's name is public on the galaxy map forever.

### 3. Discovery log panel (`systems/discovery.rs`)

- [ ] The existing discovery panel (`DiscoveryPanelVisible`, toggled with `OpenDiscoveryPanel` / `U`) currently shows the ecosystem resource. Add a sub-tab: `Ecosystem` | `Discoveries`. Tab cycled with `Tab`/`Shift+Tab`. Default to `Ecosystem`.
- [ ] The `Discoveries` tab shows a scrollable list of every system the player has discovered, ordered by most recent first: `"⟨system name⟩ — ⟨coord⟩ — ⟨date⟩"`. Each entry shows the date the player first scanned and confirmed discovery.
- [ ] The discovery log is loaded from the save file on game start: `DiscoveryLog` resource. When a new discovery is confirmed (server responds with `discoverer_id == player_id`), the client appends to the log and marks the save file dirty.
- [ ] If the player has no discoveries yet, show `"No discoveries yet. Scan an uncharted system to claim it."`.
- [ ] The discovery log is local-only (not synced to the server). The server stores per-system discoverer_id; the client stores the player's personal list for display. On a fresh client (no save file), the log starts empty and fills as the player rediscovers systems — but the server still shows their name from the first discovery. This means the log may be incomplete if the player plays on multiple clients, but the server's attribution is canonical.

### 4. Exploration career integration (`systems/career.rs`)

- [ ] The `CareerProgression` resource has `exploration.systems_discovered: u32`. Increment this counter when the client receives `ServerMessage::SeedDiscovered` with `discoverer_name == player_name`.
- [ ] The career panel (accessible from the pause menu or crew roster) shows the discovery count as a career stat: `"Systems Discovered: {N}"`. The exploration career's rank progression checks `systems_discovered` against the rank thresholds.
- [ ] The career rank-up trigger (S42 defined this but it's stubbed — the career module has the metrics and thresholds but no "rank up" behavior). For this sprint, rank thresholds are values only — no rank-up side effects (no bonuses, no titles). That's a future sprint. The counter increments correctly and the display updates.

### 5. Discovery notification (`systems/discovery.rs` or `systems/hud.rs`)

- [ ] When the client receives `ServerMessage::SeedDiscovered` confirming the player's discovery, show a HUD toast notification: `"System charted: ⟨name⟩"`. The toast appears at the top-center of the screen and fades out after 5 seconds.
- [ ] If the system was ALREADY discovered by someone else, show: `"System already charted by ⟨player⟩"` — informational, not a failure. The player still gets the system's data; they just don't get the exploration credit.
- [ ] The toast is a Bevy `Text` entity spawned at the notification area, with a `NotifTimer` component that despawns it after the duration. Follows the same pattern as `S37`'s captain log entry flash or the S19 combat damage indicator — a transient text that fades.
- [ ] The notification area exists on the HUD (top-center, above the crosshair or centered). If no notification area exists, create a simple one: a `Node` at `top: 5%, left: 50%, transform: translateX(-50%)` with a fixed height slot for one notification at a time.

## Acceptance gates

```
cargo test -p reachlock-core seed::system_info_discovered_by_round_trip
cargo test -p reachlock-client discovery::log_persists_in_save
cargo test -p reachlock-client discovery::career_counter_increments
cargo test -p reachlock-client galaxy_map::attribution_display

# Manual:
# 1. Start server, connect client
# 2. Fly to uncharted system → scan → "System charted: Proxima" toast appears
# 3. Open galaxy map (G) → select the system → "[Proxima — charted by YOU]" shown
# 4. Open discovery panel (U) → tab to Discoveries → Proxima listed with timestamp
# 5. Save → quit → reload → discovery log still contains Proxima
# 6. Switch to "other player" scenario: the system shows "charted by ⟨name⟩"
# 7. Career panel shows "Systems Discovered: 1"
make check
```

## Non-goals

- Naming rights / renaming discovered systems (the system name is procedurally generated from the seed — players don't rename it. The "naming rights" from the MASTER-PLAN description means their NAME is on it, not that they get to rename it.)
- Discovery PvP / stealing (first-write-wins is atomic — no stealing. If two players scan the same system in the same tick, one wins and the other gets "already charted by X".)
- Server-side discovery history for each player (the server stores per-system discoverer_id, not per-player discovery list. The client builds the log from its own events.)
- Discovery leaderboards / "most systems discovered" ranking (the data exists in the career counter, but no leaderboard UI — that's a social/sprint for Phase 4)
- Discovery as NFT/blockchain (fundamentally against the design — discovery is a server-side record, not a tradable asset)
- Discovery notifications for OTHER players (when someone discovers a system, only they get the toast. No "X discovered Y" broadcast to the universe.)

## Gotchas

- The `ChartedSystem` type in `galaxy_map.rs` comes from the content index (`ContentIndex::charted_systems`). The content index loads these from disk (authored charted systems). Dynamically discovered systems from the seed store are a SEPARATE source. The galaxy map must merge both sources for display. If a system exists in both (content author charted a system that a player later re-encounters in multiplayer), the author's version has no discoverer — the seed store's version might. The content index version wins for static data; the seed store's discoverer_id overlays on top.
- The `discoverer_id` on the wire is the player's `public_name` (from `PlayerRecord.public_name`), not the internal `player_id` UUID. The server's `SeedStore` stores the public name for display. Verify that S66's auth store correctly returns the public name in the WS session context — the seed discovery handler resolves it from the session.
- The discovery log save integration: save files use RON serialization. The `DiscoveryLog` resource must derive `Serialize`/`Deserialize` and be included in the save file's resource list. Follow the existing `SaveFile` pattern: a section in the save RON that serializes/deserializes the `Vec<DiscoveryLogEntry>`. If the save file doesn't have the field (old save), the log starts empty — `#[serde(default)]`.
- Exploration career progression: S42 defined the career progression system. The `CareerProgressionResource` holds `exploration: ExplorationProgression { rank: CareerRank, systems_discovered: u32, ... }`. Verify the career system exists and the counter field exists before merging. If either is missing, this sprint CREATES the exploration counter (add it to `CareerProgression` in `core/src/career.rs` or the client's career resource).
- The toast notification entity: spawn it at the notification position with a `Notification { kind: Discovery, expires_at: Instant + Duration::from_secs(5) }` component. A cleanup system runs every frame and despawns expired notifications. Keep the entity pool small — one notification at a time, stacked if multiple arrive within the same window.
