# ReachLock v2 — developer entry points.
export PATH := $(HOME)/.cargo/bin:$(PATH)

.PHONY: test check fmt clippy run run-debug server wasm determinism clean

test:
	cargo test --workspace

check: fmt clippy test wasm
	@echo "all gates green"

fmt:
	cargo fmt --all --check

clippy:
	cargo clippy --workspace -- -D warnings

# Launch the game (native).
run:
	cargo run -p reachlock-client

# Launch with Bevy's `debug` feature so ECS errors (e.g. B0001 query
# conflicts) print real component/system names instead of a placeholder.
run-debug:
	cargo run -p reachlock-client --features debug-names

# Launch the ledger server on 127.0.0.1:40711.
server:
	cargo run -p reachlock-server

# The spike gate: full plugin stack must compile for the web target.
wasm:
	cargo build -p reachlock-client --target wasm32-unknown-unknown

# Local determinism self-check (CI does the real cross-target compare).
determinism:
	cargo run -q -p reachlock-cli -- determinism emit > /tmp/reachlock-manifest.json
	cargo run -q -p reachlock-cli -- determinism check /tmp/reachlock-manifest.json

clean:
	cargo clean
