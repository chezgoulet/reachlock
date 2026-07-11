# M6 — "The universe moves" ✅ (2026-07-03)

The north-star sentence, first half, verified headless end-to-end: *dock at
Sorrow Station; prices differ because the simulation moved while you flew;
the news feed reports real faction events; quit — the universe was saved
mid-motion, and resumes.* Every observation below is from actual runs on
the dev machine; commands included so a human can replay them.

## The pieces

- **Sim Protocol v0** — godot/framework/protocol/SIM-PROTOCOL.md, 15
  fixtures under `sim/fixtures/`, round-tripped in `go test ./internal/simd/`.
- **reachlock-simd** — the universe tick as a loopback daemon
  (`make simd-build`; the dev stack starts it on 127.0.0.1:40708).
- **SimGateway** (P1) — framework autoload, SoulGateway's pattern: 1
  tick/second while playing, batch advance on time skips, offline-tolerant.
- **MarketBoard** (P2) + **EventFeed** (P3) — framework scenes
  (scenes/framework/), configured from location data; landed mode
  instantiates them for any location whose services include "market"/"bar".

## 1. The universe moves on its own

What a tester does:

```sh
make simd-build && ./server/bin/reachlock-simd --port 45990 --mods godot/mods
# then, over the wire (or just run the game and fly for an hour):
#   advance 3600 ticks; query_prices at two stations
```

What was observed (real REACHLOCK content, 2.5 in-game days):

```
sorrow_station:  rations 4 cr (base 8)   raw_ore 24 cr (base 12)   alloys 72 cr
verne:           rations 16 cr (base 8)  raw_ore 17 cr (base 12)   alloys 73 cr
```

Sorrow Station produces rations (cheap where made) and consumes ore (dear
where needed); Verne is the mirror. **Prices differ across stations and
from yesterday**, driven by the production/consumption pulse, per-location
`PriceAt`, and 60-tick reprices — all deterministic (same seed + inputs =
same universe, journal included; `go test ./internal/universe/`).

## 2. The news is the simulation's event log

The journal records what actually happened — reprices, player trades,
stance changes, patrols, skirmishes (strained faction pairs make news on
the 1440-tick goal cadence; verified by `TestStrainedFactionsMakeNews`).
The EventFeed renders it in the bar, and **every item is re-broadcast to
the souls present as a `news.<kind>` perceive event** — Tib reads the same
feed you do.

## 3. Trades flow back as sim inputs

Selling 200 ore to Sorrow Station's market visibly depresses the local ore
price on the very next quote, and the trade lands in the journal
(`TestLifecycleOverTCP`, and the MarketBoard's Buy/Sell/Sell-all buttons
in-game). The market remembers you.

## 4. Saved mid-motion, resumes — across daemon restarts

What a tester does: play (headless here), save, kill the daemon, start a
fresh one, relaunch. Observed transcript (run A, then run B):

```
sim: connected to reachlock-simd/0.1.0 (tick 3600, seed 1)   # run A
game_state: saved to user://saves/slot0.json (tick 3611)     # 11 s later

sim: connected to reachlock-simd/0.1.0 (tick 0, seed 1)      # run B, FRESH daemon
sim: pushed saved universe (tick 3611) to the daemon         # save wins
game_state: saved to user://saves/slot0.json (tick 3616)     # ticking resumed
```

Every `advanced` reply carries a full snapshot into
`GameState.universe["sim"]`, so a save at any moment is mid-motion; on
connect (or save load) the snapshot is pushed back with `load`.

Found and fixed during this verification: Godot's JSON writes every number
as a float (`"trust": -43.0`), which Go's strict decode rejected —
`universe.UnmarshalJSON` now normalizes number styles
(`TestUnmarshalToleratesFloatStyleNumbers`). This is why the demo transcript
exists: the bug only lives at the cross-language boundary.

## 5. Time passes while you fly

SimGateway drives 1 tick/second whenever connected (verified: 12 wall
seconds → 11-12 ticks in run A) and `landed._undock()` batch-advances 30
ticks of departure clearance — the determinism contract makes the batch
identical to having lived it.

## Offline honesty

Kill the daemon: the game keeps running, the MarketBoard renders static
authored prices labeled "(static — no sim)", the feed reports no signal,
and the status line marks the tick "(static)". Same posture as Pan and
Ragamuffin: the game never requires the stack.

## Gates at time of writing

`make check` green (architecture guard, mod validation, both protocol
conformance suites, DSL battery); `go test ./internal/...` green including
the new simd + journal/PriceAt/determinism suites; headless import clean;
DSL bridge 27/27; soul-protocol harness 21/21.

## Known gaps (tracked, non-blocking)

- The player's own trade can be missed by the news feed's since-tick
  cursor if it lands on an already-seen tick (the landed log reports it
  anyway). Cosmetic; a journal sequence number would close it.
- Faction news depends on strained pairs existing in content; with the
  current authored stances most early news is economic. Content tuning,
  not engine work.
- Wire seeds must stay ≤ 2^53 (Godot JSON is float64-only) — documented
  in SIM-PROTOCOL.md.
