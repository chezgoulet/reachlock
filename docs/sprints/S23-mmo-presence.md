# S23 — MMO Presence & Coordination

**Spec:** §8, Phase 3 (player coordination), §10 (content distribution) ·
**Wave 6 · Depends on:** S02, S03

## Outcome

Other people exist: players in the same system see each other's ships move,
proximity chat works, presence flows through Redis-backed sessions, and the
authored-content deployment pipeline reaches live clients —
`reachlock content publish` pushes an override and connected players see it
on next system entry. The persistent universe becomes a shared one.

## Context

- The server already broadcasts `player.entered` and rebroadcasts
  `player.position` as universe events — but to ALL sessions. This sprint
  adds scoping (per-universe, per-system interest) so it fans out sanely.
- S02 gives the client socket; S03 gives sessions/auth/Postgres; S12 (if
  merged) has the server ticking real state.
- Redis appears here (spec §8: sessions, live positions, rate counters) —
  S03 and S14 designed their stores behind traits for exactly this moment.

## Freeze first

Interest scoping rules, written in the module docs before code: a session
subscribes to (universe, current system); position updates fan out only to
co-located sessions at a fixed cadence with change-compression; chat has
two scopes (system, direct). Extend `network/messages.rs` with
`chat.send` / `chat.message` / `player.left` / `player.moved` — wire-shape
tests updated (this is a protocol revision; version tag the protocol:
add a `hello` exchange carrying `protocol_version`, refused loudly on
mismatch — v1's share_version lesson).

## Deliverables

- [ ] Interest management: sessions register their system on entry
      (`seed.discover` already marks it); position fan-out per system;
      entering/leaving systems emits joined/left to co-located players.
- [ ] Client: remote players render as ships (their hull from their config
      seed — falls back to a default hull) with name labels, eased between
      updates (v1's RemotePawns easing pattern).
- [ ] Chat: system-scope chat panel (Enter to type, escape to close), server
      relays with rate limiting; direct messages by player name.
- [ ] Redis integration behind the existing traits: SessionStore (S03) and
      rate counters (S14) get Redis impls, enabled by `REACHLOCK_REDIS=…`,
      memory impls remain the default; server stays horizontally honest
      (presence via Redis pub/sub between server instances is the design —
      single-instance is fine to ship, the trait must not preclude it).
- [ ] Content deployment: `reachlock content publish <file> --server <url>
      --priority …` (auth'd, admin token) → inserts into content_overrides →
      `content.update` WS broadcast → clients invalidate their cache for
      that system and re-fetch on next entry (`GET /content/system/{id}`).
      Local cache per spec §10 stage 4.
- [ ] Deployment history: content_deployments row per publish (the table
      exists); `reachlock content rollback <deployment-id>` restores the
      prior version.

## Acceptance gates

```
cargo test -p reachlock-server presence:: chat:: content_api::
# two-client integration test: A sees B enter, move, chat, leave
make check
```
Manual: two clients in one system — see each other fly and chat; publish a
station override while both connected → both see the new station on
re-entry; a third client in another system sees none of it.

## Non-goals

Trading/grouping/party mechanics. PvP combat rules (needs a design pass —
flag, don't improvise). Colonization (next brief after this ships).
Cross-server sharding. Voice.

## Gotchas

- Fan-out math: N players broadcasting positions to N players is the
  classic quadratic trap — interest scoping is the sprint, not an
  optimization afterthought.
- Chat is user content: length caps, rate limits, and no server-side
  logging of message bodies at info level.
- The protocol version handshake must land BEFORE the new messages — a
  v1-protocol client against a v2 server should get one clear error, not
  serde noise.
