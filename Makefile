# ReachLock v2 — developer entry points.
export PATH := $(HOME)/.cargo/bin:$(PATH)

.PHONY: test check fmt clippy run run-debug server determinism clean

test:
	cargo test --workspace

check: fmt clippy test
	@echo "all gates green"

fmt:
	cargo fmt --all --check

clippy:
	cargo clippy --workspace -- -D warnings

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

.PHONY: web web-serve wasm-bindgen-cli

# Local determinism self-check (CI does the real cross-target compare).
determinism:
	cargo run -q -p reachlock-cli -- determinism emit > /tmp/reachlock-manifest.json
	cargo run -q -p reachlock-cli -- determinism check /tmp/reachlock-manifest.json

# S22 engine-purity guard: scan for known content ids in engine source.
# Runs on the current diff if available, else on the full source.
check-purity:
	@# Generate a list of content ids from mods/reachlock/ content files.
	@# Then check if any appear in core/src or client/src.
	@echo "checking engine purity..."
	@{ find mods/reachlock/souls mods/reachlock/stations mods/reachlock/hulls \
		mods/reachlock/systems -name '*.ron' 2>/dev/null; \
	   echo "mod.manifest.ron"; } | while read -r f; do \
		id=$$(grep -oP '^\s*id:\s*"\K[^"]+' "$$f" 2>/dev/null); \
		[ -n "$$id" ] && echo "$$id"; \
	done | sort -u > /tmp/content_ids.txt
	@# Check against core and client source.
	@result=0; \
	while read -r id; do \
		if grep -rq "$$id" reachlock-core/src reachlock-client/src 2>/dev/null; then \
			echo "WARN: content id '$$id' appears in engine source"; \
			result=1; \
		fi; \
	done < /tmp/content_ids.txt; \
	exit $$result

.PHONY: check-purity

clean:
	cargo clean
