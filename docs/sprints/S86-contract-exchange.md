# S86 — Contract Exchange

**Spec:** §6 (contract system), §18 (LLM agency model) · **Wave E (shared world & distribution)** · **Depends on:** S81 (content dispatch — the contract pipeline from workshop→runtime is wired), S34 (contract crafting workshop + library)

## Outcome

Contracts are social. A player crafts a contract in the workshop, publishes it to the server library, and any other player can browse, filter, search, and import it into their own workshop for customization or direct installation. Each contract carries community stories — player-submitted anecdotes about the memorable moments that contract produced. Publishing is rate-limited per player to prevent spam. A share code (or URL) lets a player point a friend directly at a specific published contract. The existing server `ContractLibrary` service, which currently has no real client surface, is fully wired: browsing, publishing, story submission, and import all work end-to-end.

## Context

- S34 built the contract crafting workshop (898 lines in `contract_crafting.rs`) and the contract library browser (376 lines in `contract_library.rs`). The library has sort/filter/import UI but uses LOCAL entries only — the `entries: Vec<ContractLibraryEntry>` is populated by a hardcoded seed or by pasting RON text into the import buffer. There is no network sync.
- The server has a `ContractLibrary` service (172 lines in `services/library.rs`) with `list`/`publish`/`submit_story`/`stories_for` methods. The `MemoryContractLibrary` stores entries in-memory. The Pg impl stores them in a `contract_library` table. The service works, but no client code calls it.
- S81 wired `ContentPayload::Contract` files into the `ContractRuntime` (contracts from authored content files load into the runtime). Player-crafted contracts from the workshop can now be "installed" (the draft → runtime path exists). But workshop → library → other player's runtime is the missing link.
- The network messages exist in `reachlock-core/src/network/messages.rs`: `LibraryList`, `LibraryPublish`, `LibrarySubmitStory`, etc. are defined as wire messages with serialization tests locking their format. They're never sent.
- The contract story system (`ContractStory` in `metadata.rs` + `story_submission.rs` in the client) has a UI for writing stories but no submit button that reaches the server. Stories are local-only.
- S34's `ContractLibraryEntry` carries `metadata: ContractMetadata` with `author`, `crew_role`, `personality_tags`, `story_tags`, `description`. The library browser already renders these fields — they just need network data.

## Freeze first

### Extended wire messages

```rust
// reachlock-core/src/network/messages.rs additions

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibrarySyncRequest {
    pub role_filter: Option<String>,
    pub sort: Option<String>,           // "newest", "stories", "interesting"
    pub search: Option<String>,         // keyword search across name/description/tags
    pub page: u32,                      // pagination — 0-indexed
    pub page_size: u32,                 // default 50, max 100
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibrarySyncResponse {
    pub entries: Vec<ContractLibraryEntry>,
    pub total: u32,                      // total matching entries (for pagination UI)
    pub page: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryPublishRequest {
    pub entry: ContractLibraryEntry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryPublishResponse {
    pub ok: bool,
    pub share_code: Option<String>,     // 8-char alphanumeric code
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryShareLookup {
    pub share_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryShareResponse {
    pub entry: Option<ContractLibraryEntry>,
}
```

### Rate limit config for publishing

```rust
// reachlock-server/src/config.rs additions
pub struct LibraryConfig {
    pub publish_cooldown_secs: u64,  // min seconds between publishes per player
    pub max_publishes_per_day: u32,
}
```

Defaults: 60 seconds cooldown, 10 publishes per day. Configurable via `REACHLOCK_LIBRARY_PUBLISH_COOLDOWN` and `REACHLOCK_LIBRARY_MAX_PUBLISHES_PER_DAY`.

### ContractLibrary trait additions

```rust
// reachlock-server/src/services/library.rs additions
pub trait ContractLibrary: Send + Sync {
    // Existing:
    fn list(&self, role_filter: Option<&str>, sort: Option<&str>) -> Vec<ContractLibraryEntry>;
    fn publish(&self, player_id: &str, entry: ContractLibraryEntry);
    fn submit_story(&self, story: ContractStory) -> u64;
    fn stories_for(&self, contract_id: &str) -> Vec<ContractStory>;

    // New:
    fn publish_rate_limited(&self, player_id: &str, entry: ContractLibraryEntry) -> Result<String, PublishError>;
    fn search(&self, query: &str) -> Vec<ContractLibraryEntry>;
    fn lookup_share_code(&self, code: &str) -> Option<ContractLibraryEntry>;
    fn generate_share_code(&self, contract_id: &str) -> String;
}

pub enum PublishError {
    RateLimited { retry_after_secs: u64 },
    DailyLimitExceeded,
    DuplicateContract,
}
```

## Deliverables

### 1. Library network sync (`systems/network.rs` + `systems/contract_library.rs`)

- [ ] On client startup (and when the library panel is opened), send a `ClientMessage::LibraryList` (or the new `LibrarySyncRequest`) to the server. The server's `ContractLibrary::list()` returns the entries. The client stores them in `ContractLibraryState::entries`.
- [ ] The sync request includes the current sort/filter/search/page parameters from the library UI. On sort/filter/page change, re-request from the server.
- [ ] Pagination: the library shows 20 entries per page. The server returns `total` for the "X of Y" display. Page controls in the library UI: `[< Prev] [Page N of M] [Next >]`.
- [ ] Wire the existing `ClientMessage` / `ServerMessage` variants: `LibraryList` (request), `LibraryPublish` (request), `LibrarySubmitStory` (request), and their response variants. Add `LibrarySync`, `LibraryShareLookup` and `LibraryShare` if the existing messages don't cover the new paginated/search interface. The principle: add new message variants rather than breaking existing ones.
- [ ] Offline behavior: the library panel shows a "Server required for library" message when offline. Local-only entries (imported via RON paste) are still shown in a "Local" filter tab — they cannot be published until online.
- [ ] Test: connect a test client → open library → entries appear from server → sort by newest → entries re-sorted → server log shows the query.

### 2. Import to workshop (`systems/contract_crafting.rs`)

- [ ] In the library panel, when viewing a contract's detail view (existing `detail: bool` state in `ContractLibraryState`), show an "Import to Workshop" action. Default key: `I`. This copies the `ContractLibraryEntry`'s embedded `Contract` (the library stores the full serialized contract) into `ContractWorkshopState::draft`.
- [ ] If the workshop already has a modified draft, show a confirmation: "Import will overwrite your current draft. Continue? [Y/N]". Default: N.
- [ ] After import, switch the active panel from library to workshop (`ActivePanel::ContractLibrary` → `ActivePanel::ContractWorkshop`). The workshop now shows the imported contract's rules, LLM config, and persona for editing.
- [ ] The imported contract retains the original author's `ContractMetadata.author` field. The workshop shows "Original author: ⟨name⟩" in the persona tab. The player can modify everything including the author field.
- [ ] Test: select a contract in library → press I → workshop opens with the contract loaded → rules, LLM config, and persona match the original.

### 3. Publish contract (`systems/contract_crafting.rs` + `systems/network.rs`)

- [ ] In the workshop, add a "Publish" action. Default key: `P`. Opens a sub-prompt: "Fill in metadata to publish:" — author name (pre-filled from player name), description, personality tags, story tags. The player fills these and confirms.
- [ ] On confirm, the client serializes the `ContractWorkshopState.draft` into a `ContractLibraryEntry` and sends `ClientMessage::LibraryPublish` with the entry.
- [ ] The server calls `ContractLibrary::publish_rate_limited()`. If rate-limited, the server returns an error and the client shows the retry time: "Publish again in 45 seconds."
- [ ] On success, the server returns a `share_code: String` (8-char alphanumeric, e.g., "A3X9K2M1"). The client shows: "Published! Share code: A3X9K2M1" with a "Copy" action (copies the share code to clipboard).
- [ ] After successful publish, the library panel re-syncs (the new entry appears at the top).
- [ ] Test: craft a minimal contract in workshop → press P → fill metadata → confirm → server log shows publish → library shows the new entry → share code is returned.

### 4. Contract stories (`systems/story_submission.rs` + `systems/contract_library.rs`)

- [ ] In the library's contract detail view, show the "Stories" tab alongside "Details". The Stories tab lists all `ContractStory` entries for this contract, fetched from the server via `stories_for()`.
- [ ] Each story entry shows: story text (truncated to 200 chars with "…read more"), event type, outcome type, timestamp, and author (the submitting player's name, not a privacy concern — the author chooses to submit).
- [ ] "Submit Story" button at the bottom of the Stories tab: opens a text input for the story, a dropdown for event type (combat, trade, exploration, social, crisis), and a dropdown for outcome type (triumph, disaster, comedy, drama). Submits via `ClientMessage::LibrarySubmitStory`.
- [ ] The existing `story_submission.rs` system already has the UI for writing stories — redirect it to the server instead of local-only storage. If the system stores stories in a local `Vec<ContractStory>`, add a "Submit" button that sends to the server AND keeps a local copy.
- [ ] Test: view a contract in library → Stories tab → "No stories yet" → submit a story → tab refresh shows the new story.

### 5. Browse/filter/search (`systems/contract_library.rs`)

- [ ] The library panel has: sort dropdown (Newest / Most Stories / Most Interesting), role filter dropdown (All / Engineer / Medic / Pilot / Tactical / Science / Command), and a search text input.
- [ ] Search: free-text keyword search across `name`, `description`, `personality_tags`, and `story_tags`. Server-side filter — the client sends `search` in `LibrarySyncRequest`. The server's `ContractLibrary::list()` accepts the search param and filters entries that contain the keyword (case-insensitive).
- [ ] The search input appears at the top of the library panel. The player types and presses Enter to search. A clear button (Esc or a "Clear" action) resets the search.
- [ ] Active filters are displayed as a status line: "Showing: All roles · Newest · Search: 'combat'". This helps the player understand what's filtered.
- [ ] Empty state: "No contracts found matching your filters. Try adjusting your search or clearing the filter."

### 6. Share contract (`systems/contract_library.rs` + `services/library.rs`)

- [ ] In the library's contract detail view, show the share code prominently: "Share Code: A3X9K2M1" with a "Copy" action.
- [ ] Add a "Share URL" display: `reachlock://contract/A3X9K2M1` or `https://reachlock.example/library/contract/A3X9K2M1`. The URL is informational only — the game registers a custom protocol handler (`reachlock://`) or the user pastes the URL/bookmarks it.
- [ ] "Import by Share Code" action in the library panel: a text input where the player types an 8-char code and presses Enter. The client sends `LibraryShareLookup { share_code }` to the server. If found, the contract appears as if imported from the library (same import flow as deliverable 2). If not found (expired, invalid, never existed), show "No contract found with that share code."
- [ ] Share codes are NOT permanent. They can be regenerated by the author (re-publishing generates a new code). The server stores the latest share code per contract entry. Old share codes are invalidated on re-publish.
- [ ] Test: publish a contract → note share code → switch to a different client → "Import by Share Code" → enter code → contract imports into workshop.

### 7. Rate-limited publishing (`services/library.rs`)

- [ ] `MemoryContractLibrary` gains a `publish_history: Mutex<HashMap<String, VecDeque<Instant>>>` tracking per-player publish timestamps.
- [ ] `publish_rate_limited()` checks:
  1. Cooldown: the player's last publish was at least `config.publish_cooldown_secs` ago. If not, return `RateLimited { retry_after_secs }`.
  2. Daily limit: the player has published fewer than `config.max_publishes_per_day` times in the last 24 hours. If exceeded, return `DailyLimitExceeded`.
  3. Duplicate: the contract's serialized form matches an existing entry by the same player. If yes, return `DuplicateContract`. (Optional — the player may want to publish the same contract again as an updated version. If this becomes a pain point, remove this check.)
- [ ] Pg impl: `INSERT INTO contract_library_publishes (player_id, timestamp)`. The cooldown and daily limit are SQL queries: `SELECT COUNT(*) FROM contract_library_publishes WHERE player_id = $1 AND timestamp > NOW() - INTERVAL '1 day'` and `SELECT timestamp FROM contract_library_publishes WHERE player_id = $1 ORDER BY timestamp DESC LIMIT 1`.
- [ ] Test: publish → publish again immediately → error with retry_after_secs ≈ 60 → wait 60s → publish succeeds. Publish 10 times → 11th returns DailyLimitExceeded.

### 8. ContractLibrary Pg impl (`services/library.rs` — if missing)

- [ ] If the `ContractLibrary` trait already has a Postgres impl (checking the `postgres` feature flag), verify it's complete: `list`, `publish`, `submit_story`, `stories_for`, and the new `search`, `lookup_share_code`, `generate_share_code`, `publish_rate_limited` methods.
- [ ] If no Pg impl exists, create `PgContractLibrary` with the same pattern as other store traits: a `PgPool` field, SQL queries for each trait method. Migrations: `CREATE TABLE IF NOT EXISTS contract_library (id SERIAL PRIMARY KEY, player_id VARCHAR(64) NOT NULL, entry JSONB NOT NULL, share_code VARCHAR(8), published_at TIMESTAMPTZ NOT NULL DEFAULT NOW())` and `contract_library_stories` (already created by S34 migrations or add them now).
- [ ] Wire in `AppState::new_pg`: when REACHLOCK_DB is set, use `PgContractLibrary`; otherwise `MemoryContractLibrary`.
- [ ] Test: `REACHLOCK_DB=postgres://…` → publish a contract → restart server → library still contains the entry → stories survive restart.

## Acceptance gates

```
cargo test -p reachlock-server library::publish_rate_limited
cargo test -p reachlock-server library::search_filter
cargo test -p reachlock-server library::share_code_round_trip
cargo test -p reachlock-server library::pg_persistence
cargo test -p reachlock-core network::library_messages_serialize

# Manual:
# 1. Start server + client A
# 2. Open workshop → craft a contract → Publish → fill metadata → confirm
# 3. Share code appears. Note it.
# 4. Start client B (or restart A with different player)
# 5. Open library → synced entries include A's contract
# 6. Filter by role → only matching contracts shown
# 7. Search "combat" → only contracts with combat tag shown
# 8. Open contract detail → Stories tab → submit a story
# 9. Import to workshop → workshop opens with A's contract loaded
# 10. "Import by Share Code" → enter A's code → same contract loads
# 11. Publish again immediately → rate limit error shown
# 12. Wait 60s → publish succeeds
make check
```

## Non-goals

- Contract ratings / upvote/downvote (stories are the social signal, not a numeric score)
- Contract versioning / updates (re-publishing replaces the entry; old share codes break. Version history is a future addition if the community demands it)
- Contract forking / derivation tracking (the imported contract does not link back to the original. Derivation attribution is a nice-to-have for a future sprint)
- Server-side contract validation (the client validates the contract before publishing — the server trusts the client's validation. A future sprint could add server-side re-validation to catch buggy clients)
- Contract bundles / collections (one publish = one contract. No "mod packs" or collections)
- In-game contract marketplace with currency (no token economy around contracts — they are shared freely. Rate limiting prevents spam, not commerce)
- Share code QR codes or deep-link generation beyond the `reachlock://` URL string

## Gotchas

- The existing `ContractLibraryEntry` on the wire includes the full `Contract` serialized in a `contract: Contract` field. The publish request serializes the entire contract JSON. Verify the wire message size stays under the 64KB message limit from S26 — a large contract with many rules and long LLM system prompts could exceed this. If so, split the publish into a metadata message + contract body message, or increase the limit for library messages only.
- `share_code` generation: use a CSPRNG to generate 6-8 alphanumeric characters. Collision risk is `1/(36^8)` per publish — negligible. If a collision somehow occurs, the server returns `DuplicateContract` and the client retries (generating a new code). The share code is indexed in the DB with a `UNIQUE` constraint.
- The `ContractLibraryState` currently populates `entries` from hardcoded seed data in `Default::default()`. This sprint replaces the seed data with server response data. Remove the hardcoded seed data — otherwise the library shows 5 fake entries alongside the real ones. The seed data was placeholder for the S34 implementation before networking existed. Its job is done.
- Contract cooldown per-player: the `publish_history` map keyed by `player_id` (internal UUID, from the session). The daily limit queries by player_id. If the player has never published, the map has no entry for them — the cooldown check should return `Ok` (no history = no cooldown).
- Pagination state: the library UI tracks current page, sort, filter, search. When any of these change, the UI must re-fetch from the server AND reset the page to 0 (first page). If the player is on page 3 and changes sort to "Most Stories", the request should be `page=0` with the new sort — otherwise they see an empty page 3 with the new sort applied to the wrong offset.
- Story submission authorship: `ContractStory.author` should be the player's public name (same as discovery attribution). The client fills this from the session's player info — the server trusts it. Future sprint: verify the player actually owns a version of the contract before they can submit a story for it (anti-spam for unrelated contracts).
