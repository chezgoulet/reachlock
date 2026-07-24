# S56 — CLI Content Commands

**Spec:** §10 (authoring pipeline, stages 2-3) · **Wave 14 (Editor & CLI) · Depends on:** S55

## Outcome

The content lifecycle is complete: `reachlock-cli content validate <file>`, `reachlock-cli content preview <file>`, and `reachlock-cli content publish <file>` all work. A server endpoint accepts published content and broadcasts it to connected clients. Authors can go from a `.ron` file to seeing their content in-game without touching the database directly.

## Context

- `reachlock-cli/src/content.rs` has `validate` and a placeholder `list` command. No `preview` or `publish`.
- The spec §10 defines four stages: Tool → Content File → CLI Validation → Server Import → Client Download. We have stages 1-2 (tool + validation). Missing stages 3-4 (publish + distribution).
- `POST /content/publish` endpoint does not exist on the server. The `content_overrides` table exists but has no HTTP interface.
- No way for an author to see their authored asset in-game without manually inserting into Postgres.

## Freeze first

1. `preview` launches a minimal Bevy window (headless-ish: render only, no gameplay). On WASM-hostile platforms (Linux without X11/Wayland), `preview` prints "not supported on this platform" and exits 0.
2. `publish` sends the content file to the server's `POST /content/publish` endpoint. The server URL defaults to `http://127.0.0.1:3333` and is overridable with `--server`.

## Deliverables

### 1. `reachlock-cli content preview <file>` (CLI side)

- [ ] **Spawn minimal Bevy window** — create a headless-ish Bevy app with just the renderer (no physics, no sound, no gameplay systems). The window displays the asset: mesh objects for hull/station, text for souls/dialogues/contracts, icon grid for items, color swatch for palettes.
- [ ] **Asset-specific preview** — for each asset type:
  - Hull/Station/Location: 3D-ish mesh view (2D with orthographic projection showing the layout)
  - Soul: text card with portrait placeholder, name, role, backstory snippet
  - Dialogue: tree view of the dialogue graph
  - Trope/ScriptedEncounter: narrative text preview with slot fill placeholders
  - Recipe: ingredient list → output display
  - Dungeon: 2D room graph overlay
  - Event: timeline visualization
- [ ] **Close on Esc** — the window closes when the user presses Esc or closes the window. Print "preview closed" on exit.
- [ ] **Platform fallback** — on platforms where Bevy can't open a window (headless CI, Wayland without X11), print "preview requires a display (skipped)" and exit 0.

### 2. `reachlock-cli content publish <file> [--server] [--universe] [--priority] [--available-at] [--expires-at]`

- [ ] **Upload flow** — reads the content file, serializes to JSON, sends as `POST /content/publish` to the server. Includes `Authorization: Bearer <token>` from `REACHLOCK_TOKEN` env var.
- [ ] **Flags** — `--server` (default `http://127.0.0.1:3333`), `--universe` (default `all`), `--priority` (default `curated`), `--available-at` (default `now`, ISO 8601), `--expires-at` (optional, ISO 8601).
- [ ] **Response** — prints the `content_override_id` and the system_id/object_id it was assigned to. On error, prints the server error message.
- [ ] **Auth** — reads `REACHLOCK_TOKEN` env var. If not set and the server requires auth, print "REACHLOCK_TOKEN not set (or auth disabled)" and proceed.

### 3. Server endpoint `POST /content/publish`

- [ ] **Route** — `POST /content/publish` with JSON body: `{ system_id, object_id?, universe?, asset_type, seed?, priority?, expires_at?, content }`.
- [ ] **Auth** — requires a valid bearer token. Returns 401 on bad/missing token.
- [ ] **Validation** — runs the content through the schema validator (using the `asset_type` to select the correct schema). Returns 400 with validation errors on failure.
- [ ] **Insert** — upserts into `content_overrides` table (insert on new, update on conflict of `system_id + object_id + universe + asset_type`). Generates `seed` from hash of content if not provided.
- [ ] **Broadcast** — sends a `ServerMessage::ContentUpdate { system_id, object_id, asset_type }` to all connected clients in the relevant universe via `state.events.send()`.
- [ ] **Deployment logging** — records the publish in a `content_deployments` table entry (in-memory or Postgres, depending on S49 wiring).

### 4. Client cache invalidation

- [ ] **Handle `ContentUpdate` message** — in `reachlock-client/src/systems/network.rs`, when a `ServerMessage::ContentUpdate` arrives, clear the local cache entry for that `(system_id, object_id, asset_type)`. The next time the player enters the system, the client re-fetches the updated content.

## Acceptance gates

```bash
# Preview
reachlock-cli content preview content/souls/boris.ron  # opens Bevy window

# Publish
REACHLOCK_TOKEN=... reachlock-cli content publish content/souls/boris.ron \
  --universe all --priority authoritative

# Verify: content appears in server
curl -H "Authorization: Bearer ..." http://localhost:3333/content/system/sys-001
# → returns boris soul in the overrides list

make check
```

## Non-goals

A full asset management UI (content browser, version history, rollback in the editor). Multi-file publish (batch content deployment). Content moderation pipeline.

## Gotchas

- The `preview` command spawns a full Bevy App internally. This requires the `bevy` dependency in `reachlock-cli`. Currently the CLI does NOT depend on Bevy — adding it will increase compile time significantly. Mitigation: gate the `preview` feature behind a `bevy-preview` feature flag in `reachlock-cli/Cargo.toml`. `make check` builds without the flag.
- `preview` on Wayland: the winit gotcha applies. The `make run` workaround (`WAYLAND_DISPLAY= WINIT_UNIX_BACKEND=x11`) must be applied inside the CLI. Detect Wayland and warn: "Wayland detected. Set WAYLAND_DISPLAY= and WINIT_UNIX_BACKEND=x11 if preview fails to render."
- The `POST /content/publish` auth check: use the same bearer token as the WS handshake. The token is issued by `/auth/dev` or `/auth/login`. If auth is disabled, accept any request (same as WS).
- Content update broadcast goes to `state.events` (broadcast channel). Sessions that are not in the relevant system still get the message but ignore it (they check if the system_id matches their current system). This is fine — broadcast is more reliable than targeted delivery for updates.
