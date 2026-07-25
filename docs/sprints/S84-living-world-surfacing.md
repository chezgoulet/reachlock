# S84 — Living World Surfacing

**Spec:** §11 (factions), §17 (planets & culture) · **Wave B (the systems you already paid for)** · **Depends on:** S81 (content dispatch — `ContentDispatcher` must have registered consumers for `Ecosystem` and `PlanetCulture` that populate registries)

**Closes: D2** — Ecosystem events (extinction, invasion, mutation) and planet culture data exist in core (S39, S47) with full generators, types, and tests. They have zero references outside `reachlock-core`. Players never see an extinction or hear about a planet's greeting custom. This sprint surfaces them.

## Outcome

When the player arrives in a system, the game checks the `EcosystemOverrideRegistry` for planet overrides and emits ecosystem events (a notification: "EXTINCTION EVENT: The Glimmerfin has gone extinct on Aethon due to mining runoff."). When interacting with a station or planet, the culture panel shows authored culture data — greeting customs, architecture style, social structure, faction allegiance — instead of "No culture data for this planet." The living world becomes visible. Every system with an authored ecosystem or culture override surfaces it without the player needing to dig through debug views.

## Context

- D2 (MASTER-PLAN.md): "Ecosystem events dark." `apply_ecosystem_event` at `ecosystem_events.rs:60` is a pure function with unit tests. It has never been called from a game system. The event types (Extinction, InvasiveSpecies, Mutation, PopulationBoom, etc.) exist in core, can clone and serialize, and nobody reads them.
- S47's `generate_culture` produces a full `PlanetCulture` with language, customs, architecture, clothing, attitude, allegiance, values, and a quirk. The culture panel at `culture_view.rs` renders it — but the `CultureResource` is almost always `None` because no system populates it from authored overrides.
- S81's `ContentDispatcher` registered consumers for both `ContentPayload::Ecosystem` (`load_ecosystems` → `EcosystemOverrideRegistry`) and `ContentPayload::PlanetCulture` (`load_cultures` → `CultureOverrideRegistry`). Those registries are populated at startup but nothing reads them during gameplay.
- The culture panel (`render_culture_panel`) already formats a `PlanetCulture` beautifully — language, greeting, farewell, customs, architecture, clothing, values, quirk. It's waiting for data.
- Ecosystem events are authored in `.ron` files as `ContentPayload::Ecosystem`. Each file defines one or more `EcosystemEvent`s that fire on system arrival or on a trigger condition.
- Offline-first: all ecosystem event triggering and culture display works identically with no server. Overrides are local content files. Online adds the ability to receive ecosystem events from server-driven world updates.

## Freeze first

No new core types. The frozen surfaces are:

- `ecosystem_events.rs: EcosystemEvent`, `EcosystemEventType` — S39 types
- `culture.rs: PlanetCulture`, `LanguageProfile`, `Custom`, `SocialStructure`, `OutsiderAttitude`, `FactionAllegiance`, `CulturalValue` — S47 types
- `content/envelope.rs: ContentPayload::Ecosystem(Box<Ecosystem>)`, `ContentPayload::PlanetCulture(Box<PlanetCulture>)` — S01/S81 types
- S81's registry contracts: `EcosystemOverrideRegistry`, `CultureOverrideRegistry` — populated by the dispatcher, read by this sprint's systems

### Registry lookup signatures (already exist post-S81)

```rust
// EcosystemOverrideRegistry — probably a HashMap<String, Vec<EcosystemEvent>>
pub fn events_for_planet(planet_id: &str) -> Vec<EcosystemEvent>;

// CultureOverrideRegistry — probably a HashMap<String, PlanetCulture>
pub fn culture_for_planet(planet_id: &str) -> Option<PlanetCulture>;
```

These are the read-side APIs this sprint consumes. Exact names depend on S81 implementation.

### EcosystemEventTrigger resource

```rust
/// Tracks which ecosystem events have already been fired per system
/// to avoid replaying events on every orbit entry.
#[derive(Resource, Default)]
pub struct EcosystemEventTriggerLog {
    pub fired: HashSet<String>,  // planet event ids that have been shown
}
```

### Incoming notification struct

```rust
/// Shown in the notification area when an ecosystem event triggers.
pub struct EcosystemNotification {
    pub title: String,
    pub description: String,
    pub event_type: EcosystemEventType,
    pub planet_name: String,
}
```

## Deliverables

### 1. Post-S81 Ecosystem consumer — ecosystem event notification system (`client/src/systems/ecosystem_event_system.rs`)

- [ ] Read `EcosystemOverrideRegistry` (populated by S81 dispatcher from `ContentPayload::Ecosystem` files).
- [ ] On system arrival (`OnEnter(SystemState)` or equivalent transition into a system): look up each planet in the system against the registry. For each matching planet, retrieve `events_for_planet(planet_id)`.
- [ ] Fire events that haven't been fired yet (check `EcosystemEventTriggerLog.fired`). Apply `apply_ecosystem_event` to the planet's ecosystem.
- [ ] Show notification: a temporary HUD element (same pattern as S09's jump notifications). "🌍 EXTINCTION — The Glimmerfin has vanished from Aethon's northern coasts."
- [ ] Track fired events so they don't repeat on re-entry. Persist fired-ids in the save file for that planet (loaded on save, checked on arrival).
- [ ] Event descriptions use `description_template` with variable substitution: `{planet}`, `{species}`, `{cause}` filled from the event + planet context.
- [ ] Test: load a content file with one `EcosystemEvent` for a known planet → arrive in that system → notification appears → arrive again → notification does not repeat.

### 2. Post-S81 PlanetCulture consumer — culture override loading (`client/src/systems/culture_override_system.rs`)

- [ ] Read `CultureOverrideRegistry` (populated by S81 dispatcher from `ContentPayload::PlanetCulture` files).
- [ ] When entering a planet's orbit or interacting with a station (via `StationInteraction` or `OrbitEvent`): look up the planet/station id in the registry.
- [ ] Populate `CultureResource.0` with the authored `PlanetCulture`. If no override exists, leave it as `None` (the culture panel already handles `None` gracefully — shows "No culture data for this planet").
- [ ] Online extension: when connected to a server, the server may push culture data derived from the universe tick. The `CultureResource` accepts both local and remote sources — last-writer-wins (remote overwrites local for the same planet id).
- [ ] Test: place an authored `PlanetCulture` file for "Aethon" → arrive at Aethon → open culture panel → see the authored greeting, customs, values. Remove the file → panel shows "No culture data."

### 3. Ecosystem event notification UI (`client/src/systems/notification.rs` or inline)

- [ ] Notification widget: a text box in the top-right corner of the HUD (matching the existing notification pattern from S09 jumps).
- [ ] Notification content: title line ("EXTINCTION EVENT"), description line ("The Glimmerfin has gone extinct on Aethon."), icon/prefix matching event type (Extinction = skull, InvasiveSpecies = ship, Mutation = vial, etc.). Icons are single Unicode glyphs or the existing procedural icon system.
- [ ] Notifications auto-dismiss after 8 seconds. Player can dismiss early with the `Interact` key. Dismissed notifications are suppressed on re-entry (they're still in `EcosystemEventTriggerLog`).
- [ ] Notification history: a "Notifications" panel accessible from the pause menu or a dedicated key. Shows the last 50 notifications with timestamp and planet context. Includes both ecosystem events and other game notifications.

### 4. Planet culture display — culture panel integration

- [ ] In `render_culture_panel`: when `CultureResource.0` is `Some(culture)`, render the full detail:
  - Language: base + accent + unique terms + greeting/farewell
  - Customs: each custom type with description and trigger
  - Social structure (with castes if Hierarchical)
  - Architecture style, materials, dominant shape, adapted-to conditions
  - Clothing style, material, practicality level
  - Attitude toward outsiders + explicit label (not just debug enum formatting)
  - Faction allegiance: formatted as faction name + loyalty level
  - Dominant values: formatted as a readable list
  - Cultural quirk: a one-liner
- [ ] When `CultureResource.0` is `None`, the existing fallback "No culture data for this planet" displays (no change).
- [ ] The culture panel is accessed through the existing `InputAction::OpenCulturePanel` (bound to `P` by default from S31/S47).
- [ ] Rendered text is scrollable if it exceeds the panel height (existing `Text` entity with `Overflow::clip` or a scroll component).

### 5. System arrival integration

- [ ] Hook into the system arrival event (existing `ArrivedInSystem` event or equivalent from S09/S21). On arrival:
  1. Query `EcosystemOverrideRegistry` for any planets in this system
  2. Fire pending ecosystem events (deliverable 1)
  3. Query `CultureOverrideRegistry` to pre-populate culture data for stations/planets the player can interact with
- [ ] Pre-population means the culture data is ready when the player opens the panel — no loading hitch.
- [ ] If both an ecosystem event and culture data exist for the same planet, both fire. They're independent.

### 6. Save/load integration

- [ ] `EcosystemEventTriggerLog.fired` is serialized into the save file. On load, restore the set so already-fired events don't re-fire.
- [ ] Culture override data is not saved — it's loaded fresh from content files on startup. The `CultureResource` is populated on system arrival, not from save data.

## Acceptance gates

```
cargo test -p reachlock-client ecosystem_event_system::
cargo test -p reachlock-client culture_override_system::
make check
```

Manual:
1. Place an authored `Ecosystem` file listing one extinction event for Aethon → launch → jump to Aethon's system → notification appears: "EXTINCTION EVENT: The [species] has gone extinct on Aethon"
2. Re-enter the Aethon system (leave and return) → notification does not repeat
3. Place an authored `PlanetCulture` file for Sorrow Station → dock at Sorrow Station → open culture panel (`P`) → see the full culture profile: language, customs, greeting, architecture, values, quirk
4. Remove the PlanetCulture file → relaunch → Sorrow Station shows "No culture data for this planet"
5. Both ecosystem event and culture override on the same planet → both work, notification fires and panel shows authored data

## Non-goals

- Live ecosystem simulation tick — changes are event-driven, not per-tick sim. S39 explicitly deferred this.
- Player-triggered ecosystem change — the player reads events, doesn't create them. S39's harvesting/medicinal interactions are separate.
- Culture propagation / cultural drift over time — cultures are static after generation. Settlement waves and faction shifts can override, but not per-tick.
- 3D cultural visuals (clothing meshes, architecture models) — the culture panel is text. The S47 architecture/clothing fields are data for future rendering sprints.
- Multi-biome ecosystem events — an event applies to the whole planet. Per-biome event targeting exists in the type system but this sprint fires the event once per planet.
- Server-broadcast ecosystem events (e.g., another player's mining operation triggers an extinction visible to everyone) — that's post-MMO. This sprint is local: authored events only.

## Gotchas

- The `EcosystemEventTriggerLog` uses `HashSet<String>` keyed by a composite `"{planet_id}/{event_type}/{species_id}"` to avoid re-firing the same event on the same planet. Use a hash of the event's identity, not the event index — events can be reordered in content files.
- `CultureResource` is set on system arrival, not on startup. If the player opens the culture panel before arriving at any planet, it shows "No culture data" (correct). Don't pre-populate from the override registry at game start — many planets may never be visited, and the culture data might reference planet-specific state.
- The notification widget should not block gameplay. It's a passive HUD element. If the player is in combat, ecosystem event notifications queue and display after combat ends (or they're stacked in the notification history).
- `apply_ecosystem_event` is a pure function in core that returns a new `Ecosystem`. The client system receives the returned ecosystem and stores it in a per-planet cache (or writes it to the save). The original authored `Ecosystem` is not mutated — the event produces a derived state. Store the derived state alongside the planet data.
- Culture overrides may reference faction ids, language names, or custom types that have no matching faction engine data. That's fine — the culture panel renders what it has. Missing faction data shows the id as-is ("Compact") rather than crashing.
