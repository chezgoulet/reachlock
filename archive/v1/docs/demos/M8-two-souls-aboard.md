# M8 — "Two souls aboard" ✅ (2026-07-03)

Talk to Tib and Tove aboard the Loup-Garou. The interior is a place with
rooms and crew; both perceive the same faction event and answer from their
own personalities; the CrewRoster tracks their relationship and it evolves
through shared experience. Verified headless against the live stack (pan +
Ollama gemma4:e4b + reachlock-simd); transcript below is verbatim.

## The pieces

- **CrewRoster** (P6) — framework autoload, a pure data structure other
  systems query: membership (`is_aboard`), stations (`assigned_to`,
  `assignment`), the relationship graph (`relationship(a,b)` → trust +
  affinity), shared history. Seeded from content (npc `ship` + `aboard` +
  `station`, edges from `relationships` — both sides' authored strengths
  average), then EVOLVES and persists in the save's new `crew` block. It
  does not touch soul lifecycle, dialogue, or rendering.
- **ShipInterior** (P5) — framework scene (scenes/framework/
  ship_interior.tscn): rooms from the hull's `interior_rooms`, color-coded
  zones (framework defaults per room type; hull `room_zones` overrides),
  crew as recognizable colored stand-in figures at their stations (npc
  `color`, deterministic fallback), ship status readouts, talk buttons
  (authored dialogue first, ambient perceive otherwise), mode buttons
  (fly / disembark when docked). On Board mode is now
  `extends Node2D` + instantiate-and-configure — the one-line rule holds.
- **Content**: npc schema gains optional `station`, `color`, `aboard`
  (additive); Tib (cockpit, #4a7ab5) and Tove (cargo_hold, #b5654a) are
  aboard; the other five authored crew stay ashore until their story joins.

## What a tester does

```sh
./scripts/dev_stack.sh              # pan + simd + ragamuffin
make godot                          # F2 / board the ship
```

Headless replay of the same thing (what this transcript is): start simd,
inject one faction event, run the game in On Board mode.

## What was observed

```
aboard: crew present: tib@cockpit, tove@cargo_hold
sim: connected to reachlock-simd/0.1.0 (tick 0, seed 1)
souls: connected to pan-serve/0.1.0 (protocol 0)
```

One real simulation event (Compact stance toward the Reach shifts to
hostile) lands in the journal; the news feed broadcasts it to every soul
aboard as a `news.stance_change` perceive. **The same event, two voices:**

```
aboard: Tib:  Keep the shields up. We move fast when they show teeth.
aboard: Tove: Keep the watch clear and the bulkheads sealed. Let's see
              how far they want to push this time.
```

Tib answers like the pilot he is (speed, shields, evasion); Tove like a
Duskway runner (watchfulness, containment, seeing how far the Compact
pushes). Same perceive, different souls — the differentiation comes
entirely from their authored personas and allegiances riding the persona
channel.

**The relationship evolved and persisted.** The save's crew block after
the run:

```json
"edges":   { "tib|tove": { "trust": 65, "affinity": 11 } },
"history": [ { "kind": "shared_event", "topic": "news.stance_change",
               "between": ["tib", "tove"], "tick": 1 } ]
```

Trust 65 is the two authored edge strengths averaged (Tib→Tove 60,
Tove→Tib 70); affinity ticked 10 → 11 because they lived the same moment
together (`CrewRoster.record_shared_event`). Trust-moving events (a
survived firefight, a botched job) pass a `trust_delta` — the judgment
belongs to the caller (story beats, dialogue mutations), not the engine.

## Also caught during verification

- Pre-hull saves carry `hull_id: ""` and `load_game` was clobbering the
  content fallback — the whole crew (and hull stats) vanished for the
  oldest saves. Fixed: the fallback re-applies after load, and the roster
  never bakes an empty seed when the hull is unknown.
- All seven authored crew initially boarded (every npc file says
  `ship: loup_garou`). The `aboard` flag now scopes membership: Bardo,
  Doc Keene, Prudence, Risc, and Boris stay authored-but-ashore until
  their sprints.

## Known gaps (tracked, non-blocking)

- The interior is UI-first (panels, not a walkable 2D space with movement)
  — the M8 contract's "walkable" is represented as rooms-with-presence;
  free movement inside the ship rides the sprite/art pass.
- Only the shared-news path moves the graph automatically; disagreement
  detection (trust ↓) needs authored moments (dialogue mutations can call
  `CrewRoster.adjust_relationship` when that content lands).
- Crew souls perceive news only while the interior scene is open; a
  background crew-perception service is future framework work.
