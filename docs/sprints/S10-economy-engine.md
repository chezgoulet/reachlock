# S10 — Economy Engine

**Spec:** §20 (all) · **Wave 3 · Depends on:** S01

## Outcome

Prices mean something: goods are authored content, every station carries
supply/demand/storage state, prices derive from the spec formula, NPC
shipping moves goods between stations each tick, and the whole thing is
deterministic from seed + event history. Offline and online run the same
engine.

## Context

- Server-as-ledger philosophy: the engine lives in `reachlock-core`
  (`economy/` module) so offline single-player hosts it locally; the server
  hosts it for online universes (S12 wires both).
- S01 gives the content pipeline for goods definitions; S07 built the
  market UI against a `PriceSource` trait — you are the real
  implementation.
- v1 inspiration (read, don't port): `archive/v1/server/` economy engine and
  `archive/v1/docs/UNIVERSE-TICK.md`.

## Freeze first

Core `economy` types: `Good { id, category, base_price, weight, rarity,
production: Vec<StationKind>, consumption: Vec<StationKind> }` (authored,
schema in `content/schemas/goods.schema.json`); `StationEconomy { supply,
demand, storage: BTreeMap<GoodId, i64> }`; `EconomyState` (all stations in
scope) with `fn tick(&mut self, seed, tick_no)` and
`fn price(&self, station, good) -> i64` implementing
`base * demand/supply * tariff * event_modifier` in fixed-point.

## Deliverables

- [ ] `content/economy/goods.ron` — an authored starter set (~12 goods
      across raw/refined/consumer/contraband) validating against the new
      schema via `reachlock content validate`.
- [ ] Seeded initialization: a station's starting economy derives from its
      seed + kind + biome (mining stations start ferrite-heavy, etc.).
- [ ] Tick step (pure, in core): production/consumption apply → NPC shipping
      moves goods along naive best-price routes → prices recompute → events
      emitted (shortage/surplus/price-spike) as data.
- [ ] Determinism property test: two `EconomyState`s from the same seed
      ticked N times with the same player-trade log are identical.
- [ ] Tariff hook: a `tariff(faction, good) -> i64` input, constant 1024
      (=1.0) until S11 supplies real factions — keep the parameter, not a
      TODO.
- [ ] Client integration: the market's `PriceSource` now reads
      `EconomyState`; buying/selling mutates storage (a player IS demand);
      economy events surface in a station news ticker.
- [ ] CLI: `reachlock gen economy --seed N --ticks 100` prints a price table
      time series for eyeballing balance.

## Acceptance gates

```
cargo test -p reachlock-core economy::        # incl. determinism property test
reachlock content validate content/economy/goods.ron
reachlock gen economy --seed 42 --ticks 100   # prices move, none go zero/negative
make check
```
Manual: buy the local surplus cheap, jump one system, sell into a shortage.

## Non-goals

Faction tariff VALUES (S11). Server tick hosting (S12). Contract-driven
autotrading (post-S12 brief). Infrastructure investment, sabotage (Phase 3).
Crafting recipes (own brief once economy + items are merged).

## Gotchas

- All money and quantities are integers; the price formula multiplies three
  ratios — order operations to avoid overflow (i128 intermediate is fine)
  and truncation collapse (multiply before divide).
- Prices must clamp to a floor of 1 — the property test should assert it.
- Events are data in core; rendering/notifying is the client's job. No
  `println!` in the engine.
