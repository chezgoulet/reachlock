.PHONY: all godot godot-run godot-import server-build server-run server-test server-tidy validate architecture check clean

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

# Trigger-DSL reference evaluator self-test (the storyline condition language).
dsl:
	python3 scripts/trigger_dsl.py --self-test

# Cross-repo Soul Protocol integration harness: a real `pan serve` subprocess
# driven through the full lifecycle + every error path. Needs a ../pan
# checkout (built or buildable), which is why it's a separate CI job and not
# part of `check`. See tests/soul-protocol-harness/README.md.
harness:
	python3 tests/soul-protocol-harness/run_harness.py

# Full local pre-push gate.
check: server-test validate architecture protocol dsl

clean:
	rm -rf server/bin
	rm -rf godot/.godot
