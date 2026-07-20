# Adding a New Server Endpoint

1. Add route to `server/src/ws/mod.rs` router()
2. Handler in `server/src/ws/handler.rs` or new `services/` file
3. If new message variant: update `core/src/network/messages.rs` AND the wire-shape test
4. New store behind a trait: memory default, Postgres under `postgres` feature
