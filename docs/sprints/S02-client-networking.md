# S02 — Client Networking (Online Mode)

**Spec:** §4 (discovery flow), §6 (signed evaluations, client side), §8
(protocol) · **Wave 1 · Depends on:** nothing

## Outcome

The client has an online mode: it connects to reachlock-server over
WebSocket, discovers its system seed (first-write-wins round trip), submits
signed contract evaluations, and routes LLM calls through the server —
with the deliberation overlay driven by `llm.deliberating`/`llm.response`
messages instead of the offline timer. Offline mode is untouched and remains
the default.

## Context

- Protocol types are shared and pinned: `reachlock-core/src/network/messages.rs`.
- The server already answers everything you need (see
  `reachlock-server/tests/ws_roundtrip.rs` for living examples of every
  exchange). `make server` boots it on `127.0.0.1:40711`.
- The client's offline deliberation flow lives in
  `reachlock-client/src/systems/contract.rs` — extend, don't fork it.
- `SignatureChain` (client-side signing helper) is in
  `reachlock-core/src/contract/signature.rs`.

## Freeze first

`NetMode` resource: `Offline` | `Online { url, player, universe }`, selected
by env var (`REACHLOCK_SERVER=ws://…`) or menu toggle. Every network system
early-outs on `Offline`. Write this and the connection-state enum
(Disconnected/Connecting/Connected/Errored) before any socket code.

## Deliverables

- [ ] WS transport that works on BOTH native and wasm32 (e.g. `ewebsock`, or
      a thin native-thread + web-sys pair behind one trait). Nonblocking:
      Bevy systems poll a channel; no async runtime inside the ECS.
- [ ] `network.rs` client systems: connect on entering Playing (online mode),
      send `seed.discover` for the current system, adopt the canonical seed
      from `seed.canonical` (re-run the generator if the seed differs —
      show the spec §4 "Synchronizing system data…" beat).
- [ ] Signed evaluations: in online mode every fired contract action is
      signed via `SignatureChain` and sent as `eval.submit`; rejections
      surface in the ship's log.
- [ ] LLM routing: online deliberation sends `llm.call`, the overlay turns on
      at `llm.deliberating`, resolves at `llm.response`/`llm.failed`; the
      offline timeout path stays as the fallback when the socket drops
      mid-call.
- [ ] Reconnect with backoff; a dropped socket flips the HUD to an "OFFLINE"
      badge and the game keeps playing locally (spec: online adds, never
      replaces).

## Acceptance gates

```
make server &                 # then:
REACHLOCK_SERVER=ws://127.0.0.1:40711 cargo run -p reachlock-client
```
- Two clients into the same system: second one logs "canonical seed adopted".
- `X` anomaly online: overlay text comes from the server stub's reasoning.
- Kill the server mid-flight: client keeps flying, HUD shows OFFLINE.
- An integration test (native, headless: use core types + the transport
  against a spawned server, no Bevy) covering discover/adopt and eval accept.
- `make check` (the WASM build must still compile — that's the transport's
  real test).

## Non-goals

Auth tokens (S03 — connect with `?player=` query until then). Presence/chat
(S23). Contract sync persistence (S03).

## Gotchas

- wasm32 has no tokio: the transport must not pull tokio into the client.
  Keep server-only deps out of `reachlock-client/Cargo.toml`.
- The universe field must match the session's or the server errors — read
  `ws/handler.rs::route` first.
- Never block a Bevy system on a socket; poll, always.
