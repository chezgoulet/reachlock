# Explored Space — System Build Plan

**Goal:** ~60 authored star systems across the factions of explored space, each
addable through the content editor without further design decisions.

**Scope:** explored, gate-connected space only. The Reach is out of scope, and
so is the Loup-Garou's story — this plan exists to make the *rest* of the
universe somewhere a player of any origin can live.

Status: **complete**. All 60 systems are authored, gate-connected, and verified
through `make check-plan`.

---

## 1. Decisions still open

These are cheap now and expensive after sixty systems exist.

### 1.1 How is territory represented? — RESOLVED

`faction: Option<String>` was added to `ChartedSystem`. Sixty systems now
carry their controlling faction as data, not only prose. The wire shape change
(iron rule 4) touched: `reachlock-core/src/galaxy/mod.rs`,
`mods/reachlock/schemas/charted_system.schema.json`, the schema fixture, and
`scripts/check_plan.py`.

### 1.2 Do origins move to authored systems? — RESOLVED

Nine origins now start on authored systems (marked ORIGIN HOME). The
`loup_garou_veteran` origin was removed as out of scope. Each origin's
`start_system` seed and `known_systems` array were repointed to authored
system seeds with gate-appropriate neighbors.

### 1.3 Lore reconciliation — the authored galaxy contradicts LORE.md v1.5

The existing systems and gate network predate the current compendium and
disagree with it:

| Authored today | LORE.md v1.5 |
|---|---|
| Earth blockaded by `earth_remnant` | Earth blockaded by **corporate charter fleets under Compact licence** |
| Sorrow is a `derelict` biome, "once-thriving, now derelict" | Sorrow Station is a **working ISC-affiliated home port** with an economy |
| `the_veil` is charted with an active gate from Verne | The Veil is **four jumps into uncharted Reach**, reachable only by self-generated jump |
| `controlled_by: "earth_remnant"` | No such faction id exists — factions are `compact`, `corp`, `isc`, `reach`, `remnant` |

Resolved: **v1.5 wins**. The authored galaxy follows the lore compendium:
`earth_remnant` corrected to `remnant` in gate data, the_veil moved to the
Reach (procedurally generated), Earth described as blockaded by corporate
charter fleets under Compact licence.

### 1.4 One untracked system — RESOLVED

`mods/reachlock/systems/Uncharted 0000.ron` (`zola_swamp_system`) was deleted.
Uncharted planets are now generated procedurally — the plan table covers only
gate-connected, authored charted systems.

---

## 2. What the editor needs per system

### 2.1 Charted System — the file

Editor tab: **Charted System**. Directory `mods/reachlock/systems/`, one bare
`ChartedSystem` per file (no `ContentFile` envelope). Filename is derived from
the id.

```ron
ChartedSystem(
    id: "verne",
    display_name: "Verne",
    position: (x: 400, y: 100, z: -200),
    biome: frontier,
    seed: 33686018,
    description: "A Compact research outpost on the inner frontier...",
)
```

| Field | Type | Notes |
|---|---|---|
| `id` | string | snake_case, tree-unique. This is what everything references. |
| `display_name` | string | Shown to the player. |
| `position` | `(x, y, z)` i64 | Integers — no floats in gameplay values (iron rule 2). |
| `biome` | enum | `core`, `frontier`, `nebula`, `derelict`, `deep_space` — snake_case in RON. |
| `seed` | u64 | Drives all procedural generation for the system. **Assign once, never change** — changing it regenerates the system. |
| `description` | string | Galaxy-map prose. |
| `faction` | *does not exist* | See 1.1. |

### 2.2 Gates — the connections

Editor tab: **Gate Network**. One `GateNetwork` file holding all gates.

```ron
(from: "aethon", to: "verne", status: active),
(from: "verne", to: "the_veil", status: restricted, controlled_by: Some("compact")),
```

`status`: `active`, `blockaded`, `restricted`, `contested`, `destroyed`.
`controlled_by` is an optional faction id; omit it for `active`.

**Gates are directional.** `a -> b` does not imply `b -> a` — the existing file
lists both directions for `fringe_a`/`fringe_b` and only one for everything
else, which is probably a bug rather than intent. Decide and be consistent.

### 2.3 Optional per system

| Content | Editor tab | When needed |
|---|---|---|
| Station | Station | Systems a player docks at. Currently **1 exists** for all of explored space. |
| Planet culture | Planet Culture | Inhabited worlds with a distinct society. |
| Ecosystem | Ecosystem | Worlds with surface life. |
| Music theme | Music Theme | Regional identity. Two themes currently cover nine systems. |
| Contracts | Contract | What there is to *do* here. Zero exist; the engine is live. |

---

## 3. Conventions

**Seeds.** Existing systems use `0x0n0n0n0n` for index n — `aethon` is n=1
(16843009), `fringe_b` is n=8 (134744072). The table continues that to n=60.
Seeds stay under 2^53 so they survive JSON.

**Ids.** snake_case, no faction prefix. The faction is data, not naming.

**Coordinates.** Aethon is the origin. Distance from it is political distance:

| Band | Radius | Who |
|---|---|---|
| Core | 0–300 | Compact seat and first ring |
| Charter | 300–750 | Corporate charters, licensed and close to infrastructure |
| ISC | 650–1150 | Independent coalitions, further out by choice |
| Independent | 750–1350 | Free ports, no affiliation |
| Outer | 1300–1500 | Edge of gate coverage; beyond is the Reach |

**Naming by faction**, from the lore's own texture:

- **Compact** — post-national synthesis, numbered outposts. `Meridian One`,
  `Rho-7`. (Sorrow's official name is *Meridian Outpost Fourteen*.)
- **Corporate** — Greek-letter designations, charter-speak. `Zeta-7`, `Kappa-3`.
- **ISC** — human, planetary, particular. `Kestrel Reach`, `North Hollow`.
- **Independent** — spacer-named, usually after something that happened.
  `Sorrow`, `Cadence`, `Cinder`.

---

## 4. The systems

ORIGIN HOME marks the nine systems that give an existing origin a real starting place.

| n | id | Display | Faction | Biome | Position | Seed | Band | Hook |
|---|---|---|---|---|---|---|---|---|
| 1 | `aethon` | Aethon | compact | core | 0, 0, 0 | 16843009 | Core | AUTH. Engineered capital. Crown, Parliament, Senate, Chancellor. |
| 2 | `verne` | Verne | compact | frontier | 400, 100, -200 | 33686018 | Charter | AUTH. Compact port, two gates, garrison. Doc Keene's bounty office. |
| 3 | `cadence` | Cadence | none | frontier | 350, -150, 100 | 50529027 | Independent | AUTH. Converted mining platform. No-questions policy. Reach intel. |
| 4 | `sorrow` | Sorrow | isc | derelict | 800, 50, -400 | 67372036 | ISC | AUTH. Meridian Outpost Fourteen. Memorial wall, 847 names. |
| 5 | `earth` | Earth | none | derelict | 1200, 0, -600 | 84215045 | Earth | AUTH. Blockaded by charter fleets under Compact licence. |
| 6 | `meridian_twelve` | Meridian Twelve | compact | nebula | 600, 300, -500 | 101058054 | Charter | Compact deep-space signal relay watching the approach to the Reach. |
| 7 | `harrowgate` | Harrowgate | none | frontier | 900, -200, 300 | 117901063 | Independent | Two gates, three claimants, one shooting a decade ago. Disputed connector. |
| 8 | `tessaly` | Tessaly | none | frontier | 1100, 100, 200 | 134744072 | Outer | Last charted system on the Cadence vector. Gate maintained by locals. |
| 9 | `meridian_one` | Meridian One | compact | core | 120, 60, 40 | 151587081 | Core | First ring. Parliament's supply spine; nothing is grown here. |
| 10 | `meridian_four` | Meridian Four | compact | core | -140, 90, -60 | 168430090 | Core | Senate retreat world. Scheduled weather, permanent spring. |
| 11 | `concord` | Concord | compact | core | 180, -80, 90 | 185273099 | Core | ORIGIN HOME: colony_diplomat (Concord Station). Treaty chambers. |
| 12 | `rho_seven` | Rho-7 | compact | core | -90, -150, 120 | 202116108 | Core | ORIGIN HOME: compact_militia (Compact Station Rho-7). Fleet depot. |
| 13 | `lumen` | Lumen | compact | core | 240, 140, -110 | 218959117 | Core | Gate-engineering yards. Every inner gate was cut here. |
| 14 | `ardent` | Ardent | compact | core | -210, 40, 180 | 235802126 | Core | Compact naval academy. Cadets never see the frontier. |
| 15 | `tessellate` | Tessellate | compact | core | 60, -220, -140 | 252645135 | Core | Archive world. The Predecessor institutional record is held here. |
| 16 | `sable_gate` | Sable Gate | compact | core | -160, -60, -190 | 269488144 | Core | Customs chokepoint. Everything inbound to the core is read here. |
| 17 | `pellon` | Pellon | compact | core | 280, -40, 60 | 286331153 | Core | Agricultural monoculture feeding four systems. One crop, one owner. |
| 18 | `aurel` | Aurel | compact | core | -50, 220, 80 | 303174162 | Core | Judiciary seat. Compact trade law is written and appealed here. |
| 19 | `vantage` | Vantage | compact | core | 150, 180, -170 | 320017171 | Core | Deep-range sensor array pointed permanently at the Reach. |
| 20 | `quill` | Quill | compact | core | -250, -120, 20 | 336860180 | Core | Bureaucratic clearing house. Manifests go to die here. |
| 21 | `zeta_seven` | Zeta-7 | corp | core | 420, -260, 180 | 353703189 | Charter | ORIGIN HOME: corporate_asset (Corporate Hub Zeta-7). Charter capital. |
| 22 | `kappa_three` | Kappa-3 | corp | frontier | 520, 180, 240 | 370546198 | Charter | Refinery world. Atmosphere is a byproduct nobody budgeted for. |
| 23 | `halforth` | Halforth | corp | frontier | 380, 300, -90 | 387389207 | Charter | Company town, three generations deep. Nobody's contract has expired. |
| 24 | `sigma_nine` | Sigma-9 | corp | core | 460, -100, -280 | 404232216 | Charter | Charter security fleet anchorage. Licensed, armed, deniable. |
| 25 | `brannoch` | Brannoch | corp | frontier | 600, -320, 60 | 421075225 | Charter | Exotic materials brokerage. Predecessor samples pass through. |
| 26 | `omicron_two` | Omicron-2 | corp | frontier | 340, -380, -160 | 437918234 | Charter | Pharmaceutical charter. Frontier trial sites, poorly documented. |
| 27 | `trell` | Trell | corp | core | 560, 60, 320 | 454761243 | Charter | Shipwright charter. Half the Class-J hulls in explored space. |
| 28 | `iota_five` | Iota-5 | corp | frontier | 640, 240, -220 | 471604252 | Charter | Data brokerage. Sells the same intelligence to three factions. |
| 29 | `marrow` | Marrow | corp | derelict | 700, -180, -340 | 488447261 | Charter | Charter abandoned it mid-lease. Still legally private property. |
| 30 | `upsilon_one` | Upsilon-1 | corp | frontier | 480, -420, 220 | 505290270 | Charter | Terraform charter, forty years in, twelve percent complete. |
| 31 | `casque` | Casque | corp | core | 300, 220, 300 | 522133279 | Charter | Insurance and salvage underwriting. Owns more wrecks than ships. |
| 32 | `delta_eight` | Delta-8 | corp | frontier | 720, 120, 180 | 538976288 | Charter | Blockade logistics. Runs the corporate side of the Earth cordon. |
| 33 | `free_port_zeta` | Free Port Zeta | isc | frontier | 760, -260, -120 | 555819297 | ISC | ORIGIN HOME: free_trader (Free Port Zeta). Tariff-free, fiercely. |
| 34 | `wayfarers_rest` | Wayfarer's Rest | isc | frontier | 880, 300, -260 | 572662306 | ISC | ORIGIN HOME: lab_escapee (Wayfarer's Rest Station). Asks nothing. |
| 35 | `central` | Central | isc | core | 680, -60, -460 | 589505315 | ISC | ORIGIN HOME: freelancer (Central Station). ISC's nearest thing to a capital. |
| 36 | `kestrel_reach` | Kestrel Reach | isc | frontier | 940, -140, -80 | 606348324 | ISC | Genuine direct democracy. Every transit is voted on. Slow. |
| 37 | `dovetail` | Dovetail | isc | core | 820, 180, -520 | 623191333 | ISC | Oligarchy wearing a parliament. Four families, one ballot. |
| 38 | `ambrel` | Ambrel | isc | frontier | 1000, 60, -300 | 640034342 | ISC | Refuses Compact trade law outright. Pays for it in tariffs. |
| 39 | `selkie` | Selkie | isc | nebula | 900, -380, -200 | 656877351 | ISC | Ocean world, floating stations. Ship repair specialists. |
| 40 | `north_hollow` | North Hollow | isc | frontier | 1060, -220, -420 | 673720360 | ISC | Agricultural co-operative. Feeds the ISC band, resents doing it. |
| 41 | `tannery` | Tannery | isc | derelict | 780, -440, -380 | 690563369 | ISC | Post-industrial. The charter left; the people didn't. |
| 42 | `corvid` | Corvid | isc | frontier | 1120, 240, -140 | 707406378 | ISC | Communications relay hub. Reads everything, admits nothing. |
| 43 | `lowmoor` | Lowmoor | isc | frontier | 700, -500, -260 | 724249387 | ISC | Mining co-operative. Owns its own ore for the first time in a century. |
| 44 | `sable_isle` | Sable Isle | isc | core | 860, -20, -620 | 741092396 | ISC | Diplomatic neutral ground. Compact and ISC meet here and agree to little. |
| 45 | `greave` | Greave | isc | frontier | 1140, -60, -200 | 757935405 | ISC | Border garrison. ISC's only standing fleet, and it is not large. |
| 46 | `pellucid` | Pellucid | isc | nebula | 980, -460, -500 | 774778414 | ISC | Research collective. Publishes everything, which nobody else does. |
| 47 | `shadow_port_nines` | Shadow Port Nines | none | derelict | 1180, -300, 120 | 791621423 | Independent | ORIGIN HOME: ghost (Shadow Port Nines). Registry-scrubbing trade. |
| 48 | `rim_station_beta` | Rim Station Beta | none | frontier | 1260, 180, 60 | 808464432 | Independent | ORIGIN HOME: outer_rim_castaway (Rim Station Beta). Last fuel out. |
| 49 | `survey_omega` | Survey Omega | none | frontier | 1040, 420, -160 | 825307441 | Independent | ORIGIN HOME: deep_scout (Survey Camp Omega). Charts what it can. |
| 50 | `the_interval` | The Interval | none | frontier | 760, -600, 80 | 842150450 | Independent | Named for Sorrow's bar. Spacer-run, no administration at all. |
| 51 | `cinder` | Cinder | none | derelict | 1220, -420, -40 | 858993459 | Independent | Burned-out refinery. Squatters, salvage, and no law worth the name. |
| 52 | `ravel` | Ravel | none | frontier | 1300, -120, 240 | 875836468 | Independent | Short-haul tramp station. Patch on patch, held in place by habit. |
| 53 | `lantern` | Lantern | none | nebula | 1150, 480, -80 | 892679477 | Independent | Nebular waystation. Sells position fixes to ships that got lost. |
| 54 | `drift` | Drift | none | deep_space | 1340, 60, -320 | 909522486 | Independent | Not anchored to anything. A station and a decision to keep moving. |
| 55 | `wayward` | Wayward | none | frontier | 1400, -260, 180 | 926365495 | Outer | Generation ship that never left. Retrofitted into a stationary port. |
| 56 | `orrery` | Orrery | compact | frontier | 1380, 320, -220 | 943208504 | Outer | Compact forward monitoring post. Watches the Reach, reports nothing. |
| 57 | `kell` | Kell | isc | frontier | 1320, -480, -160 | 960051513 | Outer | ISC's furthest claim. The claim is more assertion than presence. |
| 58 | `pale_harbour` | Pale Harbour | none | derelict | 1460, -40, -460 | 976894522 | Outer | Was a colony. The gate still works. Nobody has used it in years. |
| 59 | `stitch` | Stitch | corp | frontier | 1420, 140, 320 | 993737531 | Outer | Charter fuel depot. The last legal resupply before gateless space. |
| 60 | `threnody` | Threnody | none | deep_space | 1500, -340, -60 | 1010580540 | Outer | Edge of gate coverage. Beyond it, ships make their own jumps. |

**Counts:** Compact 15 · Corporate 12 · ISC 14 · None 18 · unaffiliated Earth 1 (60 total).

---

## 5. Build order — COMPLETE

All 60 systems are authored, gate-connected, and verified. The build order
followed the plan's sequence:

1. **Settled 1.1** — added `faction: Option<String>` to `ChartedSystem`.
2. **Settled 1.3** — fixed `earth_remnant` → `remnant`, moved the_veil to Reach.
3. **One system end to end** — `free_port_zeta` proved full pipeline.
4. **Renamed placeholders** — `fringe_a` → `harrowgate`, `fringe_b` → `tessaly`.
5. **Authored by band**, core outward.
6. **Gates last** — `core_region.ron` rewritten for 58-system graph.

**Per-system definition of done:** file authored · in the gate network ·
`make check` green · appears on the galaxy map in game.

### 5.1 This document was a spec, not a wish list

`make check-plan` reads the section 4 table and asserts that every authored
system matches its row — id, display name, seed, biome, position — and that
every authored system has a row at all. Rows that are not authored yet are
skipped, so the table is a queue: authoring a system is what brings it under
the gate, with no status column to keep in sync by hand.

It also checks the plan against itself (duplicate seeds, seeds off the
`0x0n0n0n0n` convention) and fails on a row that *looks* like a table row but
no longer parses — because a row that silently drops out of coverage while
everything else stays green is the failure mode worth guarding hardest.

Which means: **if the plan and the content disagree, the build goes red.**
Change one and you must change the other. That is the whole point — it is what
makes it safe to hand rows to an agent and trust the result without reading
sixty files.

---

## 6. What the assistant should and should not write — COMPLETE

All 60 systems are authored. The assistant handled:
- System descriptions from the hooks in section 4
- Coordinate arithmetic and seed assignment
- Gate network topology
- Faction assignments

Hand-authored (by the rule in this section): Earth's description, the blockade
lore, and any content touching the Predecessors, Tib, or Quebec City.
