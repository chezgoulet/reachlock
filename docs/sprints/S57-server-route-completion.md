# S57 — Server Route Completion

**Spec:** §4 (seed discovery), §8 (WebSocket protocol), §10 (content distribution) · **Wave 15 (Server Routes) · Depends on:** S56

## Outcome

Every HTTP route and WebSocket message type defined in the spec exists and is wired. Clients can discover seeds, fetch content overrides, and publish content over HTTP. The WebSocket protocol has `player.jumped` and `player.disconnected` as distinct message types.

## Context

- Current routes: `/health`, `/metrics`, `/auth/dev`, `/byok`, `/ws`, `/stripe/webhook`, `/billing/checkout`, `/billing/portal`, `/billing/entitlement-token`, `/admin/*`.
- Missing routes from spec: `POST /seed/discover` (HTTP variant — spec §4 line 245-249), `GET /content/system/{system_id}` (spec §10 line 853-855), `POST /content/publish` (shared with S56).
- Missing WebSocket messages: `player.jumped` (system travel notification), `player.disconnected` (distinct from `player.left` — session expiry vs explicit leave).
- `POST /content/publish` is shared with S56's deliverable — this sprint adds it from the server side.
- The `network/messages.rs` defines 10 `ClientMessage` and 20 `ServerMessage` variants. Protocol version is 4.

## Freeze first

1. New message types are added to `network/messages.rs` with their serde-derive serialization. Protocol version bumps to 5. Wire-shape tests are updated.
2. HTTP routes follow the existing Axum pattern — handler functions in `ws/mod.rs` or a new `routes/` module.

## Deliverables

### 1. `POST /seed/discover` (HTTP route)

- [ ] **Route** — `POST /seed/discover`. Body: `{ universe, system_id, tentative_seed }`. Response: `{ canonical_seed, diffs, you_discovered }`.
- [ ] **Same logic as WS handler** — calls `state.seeds.discover(universe, system, tentative)`. Returns the canonical seed and diffs.
- [ ] **Auth** — requires a valid bearer token (same as other authenticated routes).
- [ ] **Testing** — integration test: POST twice with different seeds → first returns `you_discovered: true`, second returns `you_discovered: false` with the first seed as canonical.

### 2. `GET /content/system/{system_id}` (content distribution)

- [ ] **Route** — `GET /content/system/{system_id}`. Query params: `?universe=classic` (optional, defaults to `all`). Response: array of content overrides for that system.
- [ ] **Content override query** — queries `content_overrides` for matching `system_id` + `universe`. Filters by `available_at <= now()` and `(expires_at IS NULL OR expires_at > now())`. Returns all matching rows with their full content JSON.
- [ ] **Auth** — optional (no auth required for content fetching). The client requests this during system entry — requiring auth would break the cache-first path (spec §10 stage 4).
- [ ] **Cache headers** — returns `Cache-Control: public, max-age=300` and `ETag` header from a hash of the response body. Client can send `If-None-Match` for conditional requests.
- [ ] **Empty response** — if no overrides exist, returns `[]` with 200 (not 404). An empty system is the common case.

### 3. WebSocket protocol additions

- [ ] **Add `player.jumped` ServerMessage** — `{ type: "player.jumped", player_id, system_id, target_system_id }`. Broadcast by the WS handler when a player sends a `jump.initiate` (or equivalent) and the jump completes.
- [ ] **Add `player.disconnected` ServerMessage** — `{ type: "player.disconnected", player_id, reason: "timeout" | "quit" }`. Distinct from `player.left` which is an explicit leave (player presses "disconnect"). `player.disconnected` fires on session timeout (30s no heartbeat). Broadcast to all players in the same system.
- [ ] **Session timeout detection** — in `ws/session.rs`, track the last received heartbeat. After 30s of inactivity, close the connection and emit `player.disconnected` with reason "timeout".
- [ ] **Heartbeat message** — add `ClientMessage::Ping` and `ServerMessage::Pong`. Client sends `Ping` every 10s. Server responds with `Pong`. If server receives no message for 30s, session is timed out.
- [ ] **Bump protocol version** — `network/messages.rs` protocol constant from `4` to `5`. Update wire-shape tests.

### 4. `POST /content/publish` (shared with S56)

- [ ] Wired on the server side — route, handler, schema validation, insert, broadcast. Details in S56 deliverables section 3.

## Acceptance gates

```bash
# Seed discovery
curl -X POST http://localhost:3333/seed/discover \
  -H "Content-Type: application/json" \
  -d '{"universe":"classic","system_id":"sys-42","tentative_seed":12345}' \
  -H "Authorization: Bearer ..."
# → {"canonical_seed":12345,"diffs":{},"you_discovered":true}

# Second POST returns first seed
curl -X POST http://localhost:3333/seed/discover \
  -H "Content-Type: application/json" \
  -d '{"universe":"classic","system_id":"sys-42","tentative_seed":99999}' \
  -H "Authorization: Bearer ..."
# → {"canonical_seed":12345,"diffs":{},"you_discovered":false}

# Content fetch
curl http://localhost:3333/content/system/sys-42
# → [] or overrides array

# Wire-shape tests
cargo test -p reachlock-core network::messages::wire_shape
# protocol version 5

cargo test -p reachlock-server
make check
```

## Non-goals

Content caching layer on the server (Redis cache for `GET /content/system/{system_id}` — Phase 3). Content differential updates (delta patches instead of full override responses). Client-side content prefetching.

## Gotchas

- The `POST /seed/discover` route duplicates the WS handler's seed logic. Both call the same `SeedStore::discover` trait method. Keep the Route handler as a thin HTTP wrapper — do NOT duplicate the business logic.
- `player.disconnected` with reason "timeout" requires the session to track the last message timestamp. Add a `last_heartbeat: Instant` field to the session struct. The fairness concern: a player whose connection drops (network blip) gets a `disconnected` with reason "timeout." This is intentional — the player can reconnect and their session (if using Redis from S50) will recover.
- Heartbeat messages (`Ping`/`Pong`) are new wire types. They must be added to both `ClientMessage` and `ServerMessage` enums. They do NOT require a response from the game logic — just acknowledgement. The server tracks the `Ping` receipt as a heartbeat without acting on it.
