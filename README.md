# REACHLOCK

> *A game about surviving the frontier, choosing your allegiances, and living with the consequences of a universe that doesn't wait for you.*

A procedurally-generated spacefaring MMO. The universe is generated from seeds, player-authored automations run your ship with LLMs as the fallback at deterministic-tree leaf nodes, and the server is a ledger, not a simulator.

**v2 is being built fresh** — Rust · Bevy · Postgres · Redis. The full design is in [docs/REACHLOCK-V2-SPEC.md](docs/REACHLOCK-V2-SPEC.md).

## Repository Layout

| Path | What it is |
|---|---|
| `docs/REACHLOCK-V2-SPEC.md` | The v2 comprehensive specification (design draft, rev 2) |
| `archive/v1/` | **ReachLock v1, archived.** Godot 4 client, Go server, Pan soul engine, all sprint docs and demo ledgers. Kept for inspiration — the crew, the contracts, the three-ring architecture, and the design decisions live here. Not maintained, not built by CI |

The pre-archive tree is also tagged as `v1` — `git checkout v1` restores the original v1 layout.

## What carries forward from v1

The ideas, not the code:

- **The crew of the Loup-Garou** — Tib, Tove, Bardo, Doc Keene, Prudence, Risc, and Boris
- **Contract-first automation** — player-authored rules with LLM fallback (v1's soul/contract system, redesigned in spec §6)
- **Fail states are valid outcomes** — emergent stories over scripted safety
- **The universe moves without you** — v1's universe tick, redesigned in spec §8

## Status

**Spike #1 passed (2026-07-10):** the full plugin stack — bevy + bevy_rapier2d + bevy_prototype_lyon + bevy_audio — compiles to `wasm32-unknown-unknown` (spec §2, WASM Build Risk). Version note: rapier lags bevy by one release, so the workspace pins **bevy 0.18.1 + rapier 0.34 + lyon 0.16**; bump to bevy 0.19 when rapier 0.35 ships.

Native Linux builds need the usual Bevy system deps:

```sh
sudo apt-get install -y libwayland-dev libxkbcommon-dev libudev-dev libasound2-dev
```
