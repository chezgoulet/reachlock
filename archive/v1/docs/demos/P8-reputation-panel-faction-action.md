# P8 — ReputationPanel + FactionAction (Standing)

**Sprint 02 remainder — delivered 2026-07-04**

## What was built

### ReputationManager autoload (engine-side, P8)

The `Reputation` singleton processes FactionAction triggers from every hot path.
When `Reputation.trigger("on_trade_completed", {...})` is called, it matches
every authored FactionAction whose `trigger` field matches, applies the
`faction_delta` to the affected faction, and applies `rival_faction_delta` to
the faction with the worst stance toward it (or the named rival if specified).

**Trigger vocabulary registered:**
- `on_dock` — fired from `StationDock.configure()`
- `on_trade_completed` — fired from `MarketBoard._trade()`
- `on_undock` — fired from `StationDock` undock signal (StationDock already uses
  autoload signals; the undock trigger hook is wired to fire on the
  `undock_requested` signal path)

### GameState faction standing methods (engine-side, P8)

Added to `GameState`:
- `faction_standing(faction_id)` → `{trust, contribution, notoriery}`
- `adjust_faction_standing(faction_id, axis, amount)` — clamp-safe
- `price_modifier_for(faction_id)` → float in [-0.25, 0.25]
- `is_good_unlocked(good_id, faction_id)` — trust >= 25 AND notoriety <= 50

Standings are serialized in saves (the `factions` block was already in the
save schema — it's now populated).

### ReputationPanel UI (framework scene, P8)

A `ReputationPanel` scene at `scenes/framework/reputation_panel.tscn` that
renders a table of all known factions with:
- Trust, contribution, notoriety columns (color-coded: green ≥ 10, red ≤ -10)
- Price modifier column (computed from current standing)
- Authored relationship stance column (allied/friendly/neutral/tense/hostile/war)

Accessible from the StationDock "Reputation" button as an overlay panel.

### Content: FactionAction instances

- `trade_with_compact.json` — trading at a Compact-aligned station (+1 trust/contrib
  with controlling faction, −2 trust/−1 contrib with its worst rival).
  For Sorrow Station (Reach-controlled), trading gives +1 Reach standing, −2 Compact.

### Content: Planet data

- `aethon.json` — updated with `biome` block (arid_temperate ground #8a7a55,
  hazy orange sky #d4933a, ruins horizon) and 6 `points_of_interest`
  (landing pad, market district, ruins, settlement w/ Bardo, mine, refinery).

### Content: Dock data

- `sorrow_station.json` — added `dock` block with Tib positioned at normalized
  [0.12, 0.48] (near the bar that occupies the left side of the bay).

### Dialogue content (addressing the "known gap")

Authored 3 new dialogues:

1. **tib_station_greeting** — Tib at Sorrow Station bar, off-duty. Guard:
   `player.docked and player.location == "sorrow_station" and "talked_tib_station" not in player.flags`.
   Branches: buy a drink (ratchets trust), ask for intel (compact patrols at Verne,
   Duskway routes referenced to Tove), or just talk (LLM-generated exchange, sets
   `talked_tib_station` flag, memory seeded).

2. **tove_cargo_check** — Tove at any station dock. Guard: `player.docked and
   player.location == "sorrow_station"`. Branches: intel on Compact liaison
   (funds Duskway favours), trust-the-captain path (sets `tove_trust_earned` flag,
   +5 trust, memory seeded), or efficiency path.

3. **tove_duskway_offer** — Tove after the ambush, offers Duskway contact.
   Guard: `"survived_ambush" in player.flags and "tove_trust_earned" not in player.flags`.
   Branches: greenlight the contact (sets `tove_duskway_active` flag, +5 trust),
   or defer (LLM-generated response, offer remains open).

### Stand-in sprite art pass

Generated 8 placeholder PNG sprites (92×148, flat SNES-RPG style) for all
named NPCs: tib (blue), tove (brown), bardo (olive), doc_keene (teal),
prudence (purple), risc (rust), boris (slate), vex (dark). Each has
deterministic silhouette variation (hat/hood/hair) so same-color characters
read differently. Placed at `assets/npcs/<id>.png` — the StandIn override
path loads them when present.

## Verification

- `godot/scripts/framework/reputation_manager.gd` — class_name ReputationManager,
  registered as autoload "Reputation" in project.godot
- `godot/scripts/framework/reputation_panel.gd` — class_name ReputationPanel,
  scene at `scenes/framework/reputation_panel.tscn`
- `godot/mods/reachlock/manifest.json` — updated with `faction_actions` and
  new dialogues
- All JSON files validated against their schemas (validate_mod_data.py passes)
- Architecture guard: no content ids in engine code — existing code uses
  `DataRegistry.get_entity()` and `DataRegistry.ids()` patterns

## What needs a Godot headless run to verify

1. Headless Godot import succeeds (no broken scene references)
2. StationDock boots, "Reputation" button renders, panel opens
3. Trading at Sorrow Station market fires `on_trade_completed` → Reach standing +1
4. Docking at Sorrow Station fires `on_dock` trigger
5. PlanetScene renders Aethon with biome colors and 6 POIs
6. NPC stand-in sprite overrides load (tib.png renders instead of procedural figure)
7. Dialogue guard evaluation: tib_station_greeting triggers on first Sorrow dock after
   surviving ambush; tove_duskway_offer triggers after ambush flag
