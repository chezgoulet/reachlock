# S11 — Faction Engine & Reputation

**Spec:** §21 (all) · **Wave 3 · Depends on:** S01

## Outcome

The galaxy has sides: factions are authored content with territory,
doctrine, internal divisions, and diplomatic standings; the player carries
multi-axis reputation per faction AND per division; faction moves advance on
the tick as pure state transitions; storyline chapters trigger from tick and
reputation conditions.

## Context

- Core module (`faction/`), same offline/online-parity reasoning as S10.
- v1 inspiration (read, don't port): `archive/v1/godot/mods/reachlock/`
  faction files and reputation panel; the canon factions are Compact, ISC,
  Corp Charter, The Reach, Earth's Remnant.
- S10 defined the tariff hook you now fill; S12 wires you into the tick.

## Freeze first

Core `faction` types per spec §21: `Faction { id, name, territory,
resources, relationships: BTreeMap<FactionId, DiplomaticStanding>, goals,
internal_divisions, doctrine }`; `Reputation { trust: i64, contribution:
i64, notoriety: i64, crimes: Vec<Crime> }` (fixed-point, −100..100 scaled
by 1024) keyed by `(FactionId, Option<DivisionId>)`; `RelationStatus`
(Allied/Friendly/Neutral/Hostile/War). Schema:
`content/schemas/faction.schema.json`. Serde + wire-shape test.

## Deliverables

- [ ] `content/factions/*.ron` — the five canon factions authored with
      doctrine, 2–3 internal divisions each, starting relationships, and
      per-good tariff policies (spec §20 table: Compact regulated, ISC flat
      5%, Corp Charter dynamic, Reach zero).
- [ ] Reputation engine: `apply_event(rep, ReputationEvent) -> rep` pure
      transitions for the event vocabulary (delivered_contract,
      smuggling_caught, faction_kill, mission_complete, division_favor…).
      Division standing moves independently of faction standing; crossing
      thresholds emits standing-change events.
- [ ] Reputation gates: a pure `access(rep, requirement) -> bool` used by
      markets (price modifier), restricted docking, and mission unlocks —
      consumed now by the market (better prices at high trust) as the proof.
- [ ] Faction tick step: doctrine-driven resource allocation and relation
      drift as deterministic rules; war/peace transitions from thresholds;
      all moves emitted as events (S12 broadcasts them).
- [ ] Tariffs: implement S10's `tariff(faction, good)` from the authored
      policies; Corp Charter's dynamic tariff reads the S10 demand ratio.
- [ ] Storyline chapters: `content/storylines/*.ron` schema with `trigger`
      conditions (tick count, chapter-complete, reputation predicates) and
      `events` (FactionMove, DiplomaticShift, ContentRelease, MissionUnlock)
      — evaluated per tick, fired once, recorded in state. Author the spec's
      `compact_arc.ron` first chapter as the fixture.
- [ ] Client: a reputation panel (per faction, expandable divisions) and
      faction-tinted station banners (palette by faction id).

## Acceptance gates

```
cargo test -p reachlock-core faction::   # reputation transitions, gates, chapter triggers
reachlock content validate content/factions/compact.ron
make check
```
Manual: smuggle contraband past a Compact station → trust drops, notoriety
rises, Compact prices worsen; ISC unchanged.

## Non-goals

Full faction WARFARE conduct (border flips, fleet combat — Phase 3 brief).
LLM-generated diplomacy text (S16 consumes your events). Colonization
(post-S23). The Duskway/Veil narrative arcs (Phase 2 briefs — you build the
chapter machinery, not the story).

## Gotchas

- Reputation is fixed-point like everything else; UI converts for display.
- Chapter triggers must be idempotent-once: fired chapters live in state,
  and the property test replays a tick log to prove no double-fires.
- Faction relationship changes are symmetric bookkeeping (A→B implies B→A
  status coherence) — assert it in tests; drift bugs here are miserable
  later.
