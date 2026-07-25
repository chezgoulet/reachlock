# ReachLock v2 — developer entry points.
export PATH := $(HOME)/.cargo/bin:$(PATH)

.PHONY: test check fmt clippy run run-debug server determinism clean

test:
	cargo test --workspace

check: fmt clippy test check-purity
	@echo "all gates green"

fmt:
	cargo fmt --all --check

# --all-targets so tests and benches are linted too. Without it a test file
# can stop compiling and `make check` still reports green (that is exactly how
# reachlock-server/tests/content_distribution.rs rotted).
clippy:
	cargo clippy --workspace --all-targets -- -D warnings

# Launch the game (native).
# FIXME(winit-0.30.13): WAYLAND_DISPLAY= forces X11/XWayland to avoid
# a panic at winit/src/platform_impl/linux/wayland/window/state.rs:694
# where self.size.width is 0 because the Wayland compositor never sent a
# configure event. Remove the WAYLAND_DISPLAY= and WINIT_UNIX_BACKEND=
# overrides when bevy/winit upgrades past 0.30.13 (the built-in
# WINIT_UNIX_BACKEND=x11 alone is not sufficient on this system).
run:
	WAYLAND_DISPLAY= WINIT_UNIX_BACKEND=x11 cargo run -p reachlock-client

# Launch with Bevy's `debug` feature so ECS errors (e.g. B0001 query
# conflicts) print real component/system names instead of a placeholder.
# Same Wayland workaround as `run`.
run-debug:
	WAYLAND_DISPLAY= WINIT_UNIX_BACKEND=x11 cargo run -p reachlock-client --features debug-names

# Launch the ledger server on 127.0.0.1:40711.
server:
	cargo run -p reachlock-server

# Start Postgres via docker-compose (S03 infrastructure).
db:
	docker compose up -d postgres
	@echo "Postgres ready on 127.0.0.1:5432 (user/pass: reachlock)"

# Test with Postgres stores (S49). Requires `make db` first.
db-test:
	REACHLOCK_DB="postgres://reachlock:reachlock@127.0.0.1/reachlock" \
	  REACHLOCK_TEST_DB="postgres://reachlock:reachlock@127.0.0.1/reachlock" \
	  cargo test -p reachlock-server --features postgres --lib

# Local determinism self-check (CI does the real cross-target compare).
determinism:
	cargo run -q -p reachlock-cli -- determinism emit > /tmp/reachlock-manifest.json
	cargo run -q -p reachlock-cli -- determinism check /tmp/reachlock-manifest.json

# S22 engine-purity guard (iron rule #1).
#
# Three checks:
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
