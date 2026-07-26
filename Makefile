# ReachLock v2 — developer entry points.
export PATH := $(HOME)/.cargo/bin:$(PATH)

.PHONY: test check fmt clippy run run-debug editor install-cli dev server server-db determinism clean \
	db db-down db-reset db-psql db-test dev-secrets check-features check-content \
	check-theme check-resources

test:
	cargo test --workspace

check: fmt clippy test check-purity check-features check-content check-theme check-resources
	@echo "all gates green"

# Whole-tree content integrity (iron rule #8: a system nobody can reach is
# not done). Per-file validation cannot catch a reference to an id that does
# not exist, because that is a property of the tree — which is how ten origins
# came to name eight ship templates, nine careers and ten souls that had never
# been authored. Each of those files was individually valid.
check-content:
	cargo run -q -p reachlock-cli -- content check mods/reachlock

# Every screen is styled from assets/ui/*.ron. A widget that builds its own
# TextColor/BackgroundColor/BorderColor is invisible to the stylesheet: editing
# the theme will not restyle it. The client had 89 such literals across 35
# files, no two quite the same shade. `--self-test` checks the guard itself
# still catches a violation.
check-theme:
	python3 scripts/check_theme.py --self-test
	python3 scripts/check_theme.py

# A Bevy system taking `Res<T>` fails at RUNTIME if nothing inserted `T`, and
# the default error handler turns that into a panic. Nothing in the compiler
# catches it: the resource, the system, and the system's registration all
# typecheck. `CulturePanelVisible` shipped declared, read, keybound, and
# registered nowhere — it panicked on the transition into InGame, and with
# `debug-names` off the message could not even name the system. Twelve more
# were in the same state behind it.
check-resources:
	python3 scripts/check_resources.py

# The `postgres` and `redis` features are off by default, so nothing in the
# default build ever compiles them — and they rotted unnoticed: PgSessionStore
# was missing a trait method added in S73, and sqlx had made PoolOptions fields
# private. No database is needed to type-check them.
check-features:
	cargo clippy -p reachlock-server --features postgres --all-targets -- -D warnings
	cargo clippy -p reachlock-server --features redis --all-targets -- -D warnings
	cargo clippy -p reachlock-server --features postgres,redis --all-targets -- -D warnings

fmt:
	cargo fmt --all --check

# --all-targets so tests and benches are linted too. Without it a test file
# can stop compiling and `make check` still reports green (that is exactly how
# reachlock-server/tests/content_distribution.rs rotted).
clippy:
	cargo clippy --workspace --all-targets -- -D warnings

# Launch the game (native). The winit/Wayland workaround is applied
# in-process by the binary — see the FIXME in reachlock-client/src/main.rs.
run:
	cargo run -p reachlock-client

# Launch with Bevy's `debug` feature so ECS errors (e.g. B0001 query
# conflicts) print real component/system names instead of a placeholder.
run-debug:
	cargo run -p reachlock-client --features debug-names

# Launch the content editor.
editor:
	cargo run -p reachlock-editor

# Put `reachlock` on PATH as a real command.
install-cli:
	cargo install --path reachlock-cli
	@echo "installed: $$(command -v reachlock || echo '~/.cargo/bin/reachlock — add it to PATH')"

# One command from clean checkout to a running server.
dev: db
	@[ -f .env ] || (cp .env.example .env && echo "created .env from .env.example")
	$(MAKE) server-db

# Launch the ledger server on 127.0.0.1:40711.
server:
	cargo run -p reachlock-server

# Bring the dev stack up and BLOCK until every healthcheck passes. The old
# target returned as soon as the container started, so `make db && make db-test`
# raced Postgres' init and failed on connection refused.
db:
	docker compose up -d --wait
	@echo
	@echo "  postgres  127.0.0.1:$${PGPORT:-5432}   reachlock / reachlock"
	@echo "            databases: reachlock (dev), reachlock_test (tests)"
	@echo "  redis     127.0.0.1:$${REDISPORT:-6379}"
	@echo "  mailpit   http://localhost:$${MAILPORT:-8025}  (SMTP on $${SMTPPORT:-1025})"
	@echo
	@echo "  cp .env.example .env, then: make server"

db-down:
	docker compose down

# Destroys the volumes — the next `make db` re-runs init and migrations from
# scratch. This is the one that throws away your data.
db-reset:
	docker compose down -v
	$(MAKE) db

db-psql:
	docker compose exec postgres psql -U reachlock -d reachlock

# Live-Postgres test battery (S49). Requires `make db` first.
#
# REACHLOCK_TEST_DB points at a SEPARATE database: these tests run migrations
# and write, so sharing one URL with REACHLOCK_DB (as this target used to) made
# the suite destroy whatever was in the dev database.
db-test:
	REACHLOCK_DB="postgres://reachlock:reachlock@127.0.0.1:$${PGPORT:-5432}/reachlock" \
	  REACHLOCK_TEST_DB="postgres://reachlock:reachlock@127.0.0.1:$${PGPORT:-5432}/reachlock_test" \
	  cargo test -p reachlock-server --features postgres

# Run the server against the dev stack. Reads .env if present.
server-db:
	set -a; [ -f .env ] && . ./.env; set +a; \
	  cargo run -p reachlock-server --features postgres,redis

# Fresh key material for a real deployment — never reuse the committed sample.
dev-secrets:
	@echo "REACHLOCK_SECRET_KEY=$$(head -c32 /dev/urandom | od -An -tx1 | tr -d ' \n')"
	@echo "REACHLOCK_BYOK_KEY=$$(head -c32 /dev/urandom | od -An -tx1 | tr -d ' \n')"
	@echo "REACHLOCK_ADMIN_KEY=$$(head -c24 /dev/urandom | od -An -tx1 | tr -d ' \n')"
	@echo "REACHLOCK_SERVER_SECRET=$$(head -c24 /dev/urandom | od -An -tx1 | tr -d ' \n')"

# Local determinism self-check (CI does the real cross-target compare).
determinism:
	cargo run -q -p reachlock-cli -- determinism emit > /tmp/reachlock-manifest.json
	cargo run -q -p reachlock-cli -- determinism check /tmp/reachlock-manifest.json

# S22 engine-purity guard (iron rule #1).
#
# Four checks:
#  0. The engine must not name specific content. v2 is a character-creation
#     game: the ship, the crew, and the story are things a player picks or a
#     modder authors. The engine shipped naming one authored ship and one
#     authored crew in flight, jump, combat, crisis, and character creation —
#     so every character flew the Loup-Garou and a fixed crew narrated it,
#     whatever you chose. Systems ask the roster for a ROLE ("pilot"),
#     never for a person.
#  1. Content must not import engine code.
#  2. Core must not reach OUTSIDE ITS OWN CRATE for data. Core embedding its
#     own fallbacks under `reachlock-core/src/data/` is fine and is what keeps
#     offline play working with zero IO; `include_str!("../../mods/...")` is
#     not, because it compiles one mod's canonical content into the engine and
#     no other mod can then replace it. Drift between core's fallback and the
#     authored file is caught by tests, not by this gate.
#  3. Core's dependency tree must stay free of rendering, async-runtime, and
#     HTTP crates. This replaces the old "core must compile to wasm32" gate:
#     that build was really a proxy for "core has no IO/rendering deps", and
#     with web distribution dropped the proxy went away while the rule did
#     not. Checking the dependency tree tests the rule directly instead of
#     inferring it from a target that no longer ships.
check-purity:
	@echo "Checking engine for hardcoded content identities..."
	@python3 scripts/check_decoupling.py
	@echo "Checking content for engine imports..."
	@! rg -n 'use bevy|use reachlock_client' mods/reachlock/ || \
	  (echo "PURITY VIOLATION: content imports engine code"; false)
	@echo "Checking core for cross-crate include_str!..."
	@! rg -n --multiline 'include_str!\s*\((?s).{0,120}?(mods/|CARGO_MANIFEST_DIR)' \
	  reachlock-core/src/ || \
	  (echo "PURITY VIOLATION: core embeds content from outside its own crate"; false)
	@echo "Checking core dependency tree for rendering/IO crates..."
	@deps=$$(cargo tree -p reachlock-core --edges normal --prefix none 2>/dev/null \
	  | awk '{print $$1}' | sort -u); \
	for banned in bevy bevy_ecs wgpu winit tokio reqwest hyper axum sqlx redis rfd eframe egui; do \
	  if echo "$$deps" | grep -qx "$$banned"; then \
	    echo "PURITY VIOLATION: reachlock-core depends on '$$banned'"; exit 1; \
	  fi; \
	done; \
	echo "purity OK"

.PHONY: check-purity

clean:
	cargo clean
