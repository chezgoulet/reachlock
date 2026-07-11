# The Sim Protocol — v0

The wire contract between a REACHLOCK host (the Godot engine) and the
**simulation daemon** (`reachlock-simd`, the universe tick from
docs/UNIVERSE-TICK.md run as a sidecar). Same family as the Soul Protocol:
NDJSON over TCP loopback, versioned envelope, offline-tolerant host. Where
the Soul Protocol carries minds, this one carries the moving universe —
prices, factions, and the journal the news feed renders.

Status: **v0** (2026-07-03). Changes bump `protocol_version` and require a
migration note — additive only. Conformance: every fixture in
`sim/fixtures/` round-trips through the daemon's wire types in
`server/internal/simd` (`go test ./internal/simd/`), and the integration
path is exercised by `tests/sim-protocol-harness` (planned) and the M6
demo transcript.

## Transport

- **TCP loopback** (`127.0.0.1`), port chosen by the host and passed to the
  daemon (`reachlock-simd --port N`; `--port 0` = OS-assigned, reported on
  stderr as `simd: bound 127.0.0.1:<port>`). Default **40708**
  (`REACHLOCK_SIM_PORT`).
- **NDJSON framing**: one JSON object per line, exactly as the Soul
  Protocol: unparseable line → `error: bad_frame`, connection stays open;
  unknown `type` → `error: unknown_type`; version mismatch at `hello` →
  `error: version_unsupported`, then close.
- One connection per host process; a reconnect displaces the old
  connection. **The daemon owns the live state across connections** — a
  host that reconnects finds the universe where it left it. Persistence
  across daemon restarts is the host's job via `snapshot`/`load` (the
  save file owns the universe block).

## Envelope

Identical to the Soul Protocol envelope:

```json
{"v": 0, "seq": 3, "re": 2, "type": "prices", "body": { ... }}
```

`seq` sender-local monotonic; `re` names the request a response answers;
unknown *optional* envelope keys are ignored.

## Message types

| type | direction | body | reply |
|---|---|---|---|
| `hello` | host → daemon | `{protocol_version, profile, client}` | `welcome` |
| `welcome` | daemon → host | `{protocol_version, server, tick, seed}` | — |
| `advance` | host → daemon | `{ticks}` | `advanced` |
| `advanced` | daemon → host | `{tick, snapshot}` | — |
| `apply_input` | host → daemon | `{input: Input}` | `ack` |
| `query_prices` | host → daemon | `{location_id}` | `prices` |
| `prices` | daemon → host | `{location_id, tick, prices: [PriceEntry]}` | — |
| `query_factions` | host → daemon | `{}` | `factions` |
| `factions` | daemon → host | `{tick, factions: [FactionEntry]}` | — |
| `query_journal` | host → daemon | `{since_tick}` | `journal` |
| `journal` | daemon → host | `{tick, entries: [JournalEntry]}` | — |
| `load` | host → daemon | `{snapshot}` | `ack` |
| `shutdown` | host → daemon | `{}` | `ack`, then close |
| `ack` | daemon → host | `{}` | — |
| `error` | daemon → host | `{code, message}` | — |

Error codes (closed set for v0): `bad_frame`, `unknown_type`,
`version_unsupported`, `unknown_location`, `invalid_args`, `bad_snapshot`.

## Bodies

- **Input** is the sim's own input shape (`server/internal/universe`):
  `{kind, at_location?, good_id?, amount?, faction_id?, other?}`. The kind
  the host sends in v0 is `trade`: `amount > 0` sells goods TO the
  location (supply rises, price falls), `amount < 0` buys FROM it. One
  `apply_input` applies **once** — the daemon never passes one-shot
  inputs into a batch `advance` (universe.Advance re-applies its inputs
  argument every tick of the batch; the daemon always calls it with none).
- **PriceEntry**: `{good_id, base_price, price, supply, demand}` — `price`
  is the LOCAL price (`universe.PriceAt`), which differs across locations
  with different supply/demand. Legality and display names come from the
  host's own content load, not the wire.
- **FactionEntry**: `{id, name, trust, relationships}` — the live faction
  table, relationships being `other_id -> stance` strings from the
  framework vocabulary (`allied|friendly|neutral|tense|hostile|war`).
- **JournalEntry**: `{tick, kind, ...}` with kind-specific fields —
  `reprice` (good_id, amount=new price, delta), `stance_change`
  (faction_id, with, stance), `patrol`/`skirmish` (faction_id, with),
  `trade` (location_id, good_id, amount), `event_fired` (event_kind +
  scoping fields). Every entry is a real simulation event; the host
  renders text from the structured data (news feed, P3) and may broadcast
  entries to souls as `perceive` events (`news.<kind>` topics).
- **snapshot** (in `advanced` and `load`) is the universe state block from
  the save schema — exactly `universe.MarshalJSON`'s shape: `{tick, seed,
  factions, market, locations, events, journal}`. `advanced` carries the
  full snapshot every time (a few KB on loopback) so the host's save
  cache is always current: quit at any moment and the save resumes
  mid-motion.

## Time (the driver contract)

Per UNIVERSE-TICK.md: the HOST decides when time passes; the daemon never
free-runs. SP drives ~1 tick/second while playing (the SimGateway's
timer), pauses when the game pauses, and **batch-advances** across time
skips (undock departure, jump transit) with a single `advance {ticks: N}`
— which the determinism contract guarantees equals N live seconds.

## Offline

No daemon → the host renders static fallback prices from authored content
(`base_price` + the location economy table) and shows no news. The game
must always run without the sim, exactly as it runs without Pan.
