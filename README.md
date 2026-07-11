# REACHLOCK

> *A game about surviving the frontier, choosing your allegiances, and living with the consequences of a universe that doesn't wait for you.*

A procedurally-generated spacefaring MMO. The universe is generated from seeds, player-authored automations run your ship with LLMs as the fallback at deterministic-tree leaf nodes, and the server is a ledger, not a simulator.

**v2 is being built fresh** — Rust · Bevy · Postgres · Redis. The full design is in [docs/REACHLOCK-V2-SPEC.md](docs/REACHLOCK-V2-SPEC.md).

## Repository Layout

| Path | What it is |
|---|---|
| `docs/REACHLOCK-V2-SPEC.md` | The v2 comprehensive specification (design draft, rev 2) |
| `reachlock-core/` | Shared library, zero rendering deps: generators, seed protocol, contract engine, signed evaluations, universe tiers, network messages, determinism manifest. Everything integer-math, everything golden-tested |
| `reachlock-client/` | Bevy client: menu → a flyable generated system (hull, station, planet, ambient music — all from seeds through the bridge layer), contract engine at the helm, deliberation overlay when rules run out |
| `reachlock-server/` | Axum WebSocket ledger on `127.0.0.1:40711`: first-write-wins seed discovery, signed-evaluation verification, tier-gated LLM proxy (stub responder), non-blocking universe tick. In-memory stores by default; Postgres behind the `postgres` feature (`migrations/0001_init.sql`) |
| `reachlock-cli/` | `reachlock` binary: `gen hull\|station\|planet\|music\|ui-panel` with SVG/PPM/WAV preview exports; `determinism emit\|check` |
| `archive/v1/` | **ReachLock v1, archived.** Godot 4 client, Go server, Pan soul engine, all sprint docs and demo ledgers. Kept for inspiration. Not maintained, not built by CI |

The pre-archive tree is also tagged as `v1` — `git checkout v1` restores the original v1 layout.

## Quick start

```sh
make run          # launch the game (native)
make server       # launch the ledger server
make test         # workspace tests
make check        # fmt + clippy + tests + wasm gate
make determinism  # local generator-determinism self-check
```

In flight: `W/↑` thrust, `A/D` turn, `X` inject an anomaly no rule covers —
watch Boris deliberate and, offline, time out into his fallback routine.

CI enforces the two invariants that define the project: the full plugin
stack keeps compiling to `wasm32-unknown-unknown`, and the determinism
manifest is bit-identical across x86_64, aarch64, and wasm32 (wasmtime).

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
