# Sprint 02 deep review — the fleet-written internals (M5)

The validation debt named in the Sprint 02 brief: ~3k lines written by the
Sprint 01 fleet that had never been deeply reviewed. This pass covered
pan-daemon (session/wire/server), the reachlock server sims
(universe/economy/factions/loader), the ragamuffin embedded store, and the
GDScript framework layer. Every **correctness** finding below is fixed and
regression-tested; design findings are recorded with their disposition.

## Fixed — correctness

| # | where | finding | fix |
|---|---|---|---|
| 1 | pan `server.rs` | **Connection never closed after `shutdown` ack.** `accept_one` retained a `try_clone` of the stream in `Server::current`; the OS keeps the TCP connection open until every handle drops, so the host waited forever for the promised EOF. | `serve_loopback` calls `close_current()` when a drive ends. Regression test + integration-harness step. (pan@c5ddca8) |
| 2 | pan `server.rs`/`wire.rs` | **Unknown `type` answered `bad_frame`, not `unknown_type`.** SOUL-PROTOCOL.md: "MUST reject unknown `type` values with an `error` (code `unknown_type`)". Serde collapsed unknown-variant and missing-field into one error. | Staged parse (`parse_envelope`): JSON → type discriminator (`MessageType::from_wire`) → envelope. (pan@c5ddca8) |
| 3 | pan `server.rs` | **Daemon `seq` not monotonic.** Parse-reject and handshake-failure replies hardcoded `seq: 0`, colliding with the `welcome`. The envelope contract says sender-local monotonically increasing. | Reject replies draw from the session counter (`Session::alloc_seq`). (pan@c5ddca8) |
| 4 | reachlock `trigger_dsl.gd` | **The in-engine DSL evaluator rejected every condition containing a space** — i.e. every real condition. The tokenizer compared the regex match start (which includes `\s*`-consumed whitespace) against a position already advanced past the whitespace; they can never both hold. In lenient runtime mode every authored condition silently evaluated `false` with a warning. Never caught because the evaluator had never been run against the battery. | Guard is now `m.get_start() != pos`. The new conformance bridge (`make dsl-bridge`, in CI) runs all 27 battery cases through the engine — 16 diverged before the fix, 0 after. |
| 5 | ragamuffin `embeddedstore/store.go` | **Vault isolation broken by table-name collisions.** `tableName` mapped every non-alphanumeric byte to `_`, so `soul-x`, `soul_x`, and `soul:x` — all constructible from legal vault names — merged into one SQLite table: silent cross-vault contamination, violating contract R4 #5. | Injective escaping (lowercase/digits pass; everything else, including `_` and uppercase because SQLite identifiers are case-insensitive, becomes `_xx` hex). `migrate()` renames legacy tables via the collection registry so existing databases (the live playtest vault) keep their data. Tests: injectivity, hyphen/underscore isolation end-to-end, legacy migration. |

## Recorded — design findings and dispositions

| # | where | finding | disposition |
|---|---|---|---|
| 6 | `universe.Advance` | The `inputs` slice is re-applied at the start of **every** tick of a batch (documented, deliberate: batch N == N single calls with the same args). Passing a one-shot input (a player trade) to a 480-tick undock advance applies it 480×. | Footgun, not a bug. The sim daemon (M6) applies one-shot inputs exactly once via `ApplyInput`/`EnqueueInput` and always passes `nil` inputs to `Advance`. Noted in SIM-PROTOCOL.md. |
| 7 | `universe.maybeReprice` | Prices are **universe-global** (one price per good, net supply/demand summed across all locations). M6/P2 requires prices that differ across stations. | Closed additively in M6: `PriceAt(location, good)` derives a per-location price from the location's own supply/demand around the global price. No state-shape change. |
| 8 | `universe` | No emitted-event journal: inputs/events are applied silently, so there is nothing for the EventFeed to render ("every item was a real simulation event"). | Closed additively in M6: `State.Journal` (bounded) records reprices, stance changes, trust threshold crossings, and fired events. Additive JSON field; old snapshots load with an empty journal. |
| 9 | `universe/http.go` + `cmd/reachlock-server` | `net/http` serves each request on its own goroutine but the Manager mutates shared `State` with no lock (`handleAdvance`, `handleLoad` vs concurrent reads) — a data race under concurrent requests. Documented as "driver concern" but the current driver doesn't lock either. | The MMO server is out of Sprint 02 scope; the new sim daemon is single-connection line-at-a-time (no concurrency by construction). Filed for the MMO sprint; do not ship the HTTP driver to players before fixing. |
| 10 | `embeddedstore` | `Search` is a full-table scan per query (documented, dev-scale by design); `filterMatches` implements `Must` only (Should/MustNot silently pass); `SetPayload`/`UpdateVectors` are stubs (pruner paths remain Qdrant-only). | Documented limitations, acceptable for SP dev scale. Revisit when the pruner moves to embedded. |
| 11 | `loader.go` | Duplicate mod ids and duplicate entity ids resolve last-loaded-wins with a warning — matches the engine-side loader; lenient by design. | No change. CI (`validate_mod_data.py`) is the strict layer. |
| 12 | `memory_store.gd` | No auth support, and nothing prevented an automated run from writing play vaults. | Closed in M5: bearer-key auth (env or dev-stack key file) + `REACHLOCK_VAULT_PREFIX` namespacing; dev stack runs api_key auth by default and provides an isolated `test` stack. |

## Method note

The integration harness (tests/soul-protocol-harness/) found #1–#3 on its
first run — before this review read a line of the daemon. The bridge found
#4 on its first run. The lesson stands: executable contracts find what
reading misses; reading finds what fixtures can't express (#5, #6, #9).
