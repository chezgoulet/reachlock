.PHONY: all godot godot-run godot-import server-build server-run server-test server-tidy validate architecture protocol ear weave share dsl dsl-bridge harness test check clean

GODOT := flatpak run org.godotengine.Godot
SERVER_BIN := ./server/bin/reachlock-server

all: check

# Open the Godot editor at the game project.
godot:
	$(GODOT) --editor --path godot/ &

# Run the Godot game (headless export, or editor play).
godot-run:
	$(GODOT) --path godot/ --quit-after 0

# Headless import — the same check CI runs to catch broken scenes/scripts.
godot-import:
	$(GODOT) --headless --path godot/ --import

# Build the Go MMO server binary.
server-build:
	mkdir -p server/bin
	cd server && go build -o bin/reachlock-server ./cmd/reachlock-server/

# Build the simulation daemon (the Sim Protocol sidecar, M6).
simd-build:
	mkdir -p server/bin
	cd server && go build -o bin/reachlock-simd ./cmd/reachlock-simd/

# Build the speech daemon (the Ear Protocol sidecar).
eard-build:
	mkdir -p server/bin
	cd server && go build -o bin/reachlock-eard ./cmd/reachlock-eard/

# Run the Go server locally.
server-run: server-build
	$(SERVER_BIN)

# Run Go tests.
server-test:
	cd server && go test ./...

# Tidy Go modules.
server-tidy:
	cd server && go mod tidy

# Validate mod data JSON (same check CI runs).
validate:
	python3 scripts/validate_mod_data.py

# Three-ring architecture guard: engine must contain zero content (#7).
architecture:
	python3 scripts/check_architecture.py --self-test
	python3 scripts/check_architecture.py

# Soul Protocol conformance: golden fixtures vs wire schema.
protocol:
	python3 scripts/check_soul_protocol.py

# Ear Protocol conformance: wire fixtures + the choice-matcher reference.
ear:
	python3 scripts/check_ear_protocol.py

# Weave contract conformance: allowlist clamping over golden + adversarial fixtures.
weave:
	python3 scripts/check_weave_contract.py

# Ship-Share conformance: multiplayer payload shapes + intent/state direction.
share:
	python3 scripts/check_ship_share.py

# Trigger-DSL reference evaluator self-test (the storyline condition language).
dsl:
	python3 scripts/trigger_dsl.py --self-test

# Trigger-DSL conformance bridge: the GDScript evaluator must match the
# Python reference battery. Needs a Godot binary (GODOT_BIN, PATH, or the
# Flatpak); runs in CI's godot job, so not part of `check`.
dsl-bridge:
	python3 scripts/check_dsl_bridge.py

# Cross-repo Soul Protocol integration harness: a real `pan serve` subprocess
# driven through the full lifecycle + every error path. Needs a ../pan
# checkout (built or buildable), which is why it's a separate CI job and not
# part of `check`. See tests/soul-protocol-harness/README.md.
harness:
	python3 tests/soul-protocol-harness/run_harness.py

# Run GUT unit tests headlessly.
test:
	$(GODOT) --headless --path godot/ -s addons/gut/gut_cmdln.gd -gdir=res://tests -gexit

# Full local pre-push gate.
check: server-test validate architecture protocol ear weave share dsl

clean:
	rm -rf server/bin
	rm -rf godot/.godot
