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

# S24: release-profile WASM build → wasm-bindgen → web/dist/.
# wasm-opt reduces binary size when binaryen is installed.
WEB_DIST := web/dist
WEB_WASM := target/wasm32-unknown-unknown/release/reachlock-client.wasm
web: wasm-bindgen-cli
	cargo build -p reachlock-client --target wasm32-unknown-unknown --release
	rm -rf $(WEB_DIST)
	mkdir -p $(WEB_DIST)
	wasm-bindgen --out-dir $(WEB_DIST) --target web $(WEB_WASM)
	cp web/index.html $(WEB_DIST)/
	@which wasm-opt >/dev/null && wasm-opt -Oz $(WEB_DIST)/reachlock_client_bg.wasm -o $(WEB_DIST)/reachlock_client_bg.wasm || true
	@echo "--- web build: $$(wc -c < $(WEB_DIST)/reachlock_client_bg.wasm) bytes (wasm)"
	@gzip -c $(WEB_DIST)/reachlock_client_bg.wasm | wc -c | awk '{printf "--- web build: %d bytes (gzip)\n", $$1}'
	@ls -lh $(WEB_DIST)/

# Serve the web build locally (COOP/COEP headers commented — needed for
# threading/atomics when that arrives; not needed for single-threaded mode).
web-serve:
	@echo "Serving on http://localhost:4080 (COOP/COEP headers not set)"
	python3 -m http.server 4080 --directory $(WEB_DIST)

# Install wasm-bindgen-cli at the version pinned in Cargo.lock (version skew
# fails with a cryptic schema error — S24 gotcha).
wasm-bindgen-cli:
	@WASM_BINDGEN_VER=$$(grep -Po '"wasm-bindgen" "(\K[^"]+)' Cargo.lock | head -1); \
	if [ -n "$$WASM_BINDGEN_VER" ]; then \
		cargo install wasm-bindgen-cli --version "$$WASM_BINDGEN_VER" 2>&1 | tail -1; \
	fi

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
