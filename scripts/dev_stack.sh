#!/usr/bin/env bash
# REACHLOCK dev stack — the two sidecars the living crew needs.
#
#   ./scripts/dev_stack.sh          start pan (mind) + ragamuffin (memory)
#   ./scripts/dev_stack.sh test     start an ISOLATED stack for automated runs
#   ./scripts/dev_stack.sh stop     stop everything (play + test stacks)
#
# Then run the game:  make godot
# The game reads REACHLOCK_PAN_PORT (default 40707), REACHLOCK_MEMORY_URL
# (default http://127.0.0.1:8000), and REACHLOCK_MEMORY_KEY (defaults to the
# key file this script generates); all match the defaults below.
#
# Security posture (M5): everything binds 127.0.0.1 and Ragamuffin runs with
# api_key auth ON by default. The key is generated once into
# $DATA/ragamuffin.key (0600); MemoryStore reads that file automatically, so
# `make godot` still needs zero setup.
#
# Vault hygiene (M5): automated runs must NEVER write play vaults. The
# `test` mode starts a second Ragamuffin on port 8001 with its own database
# under ~/.local/share/reachlock-test, and prints the env to export —
# including REACHLOCK_VAULT_PREFIX=test-, which MemoryStore applies to every
# vault name. Play data and test data cannot meet.
#
# Requirements: Ollama running locally with a chat model and an embedding
# model pulled (defaults: gemma4:e4b + nomic-embed-text), the pan binary
# built (cd ../pan && cargo build --release), and the ragamuffin binary
# built from its sprint01/raga branch (embedded vector store — no Qdrant,
# no Docker).
set -euo pipefail

CHEZ_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DATA="$HOME/.local/share/reachlock"
DATA_TEST="$HOME/.local/share/reachlock-test"
PAN_BIN="${PAN_BIN:-$CHEZ_ROOT/pan/target/release/pan}"
RAGA_BIN="${RAGA_BIN:-$DATA/bin/ragamuffin}"
PAN_PORT="${REACHLOCK_PAN_PORT:-40707}"
RAGA_PORT=8000
RAGA_TEST_PORT=8001

SIMD_BIN="${SIMD_BIN:-$DATA/bin/reachlock-simd}"
SIM_PORT="${REACHLOCK_SIM_PORT:-40708}"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

if [[ "${1:-}" == "stop" ]]; then
    pkill -f "$PAN_BIN serve" 2>/dev/null && echo "pan: stopped" || echo "pan: not running"
    pkill -f "$SIMD_BIN" 2>/dev/null && echo "simd: stopped" || echo "simd: not running"
    pkill -f "$RAGA_BIN" 2>/dev/null && echo "ragamuffin: stopped (play + test)" || echo "ragamuffin: not running"
    exit 0
fi

MODE="play"
if [[ "${1:-}" == "test" ]]; then
    MODE="test"
fi

mkdir -p "$DATA"/{vaults,db,logs,bin}

# Generate the shared api key once; 0600 so it stays user-private. Both the
# play and test stacks use the same key — isolation between them is by
# database/vault root and the vault prefix, not by credential.
KEY_FILE="$DATA/ragamuffin.key"
if [[ ! -f "$KEY_FILE" ]]; then
    umask 077
    head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n' > "$KEY_FILE"
    echo "ragamuffin: generated api key at $KEY_FILE"
fi
RAGA_KEY="$(cat "$KEY_FILE")"

# The chat model must actually FIT in memory, or every soul goes silent
# (pan logs "decide failed: model requires more system memory", the game
# shows "...loses the thread"). Probe it once — this also warms the model.
# On an out-of-memory failure, fall back to a smaller model when one is
# pulled (default gemma3:4b); the probe hits Ollama's native API with
# think off, same dialect pan and ragamuffin use.
OLLAMA_BASE="${PAN_LLM_BASE:-http://127.0.0.1:11434}"
CHAT_MODEL="${PAN_LLM_MODEL:-qwen3.5:4b}"
FALLBACK_MODEL="${REACHLOCK_LLM_FALLBACK_MODEL:-llama3.2:3b}"
probe_model() { # $1 = model; succeeds when a 1-token completion works
    local out
    out=$(curl -s -m 120 "$OLLAMA_BASE/api/chat" -d "{\"model\":\"$1\",\"messages\":[{\"role\":\"user\",\"content\":\"hi\"}],\"stream\":false,\"think\":false,\"options\":{\"num_predict\":1}}" 2>/dev/null)
    [[ -n "$out" ]] && ! grep -q '"error"' <<<"$out"
}
if ! curl -sf -m 3 "$OLLAMA_BASE/api/version" >/dev/null 2>&1; then
    echo "WARNING: no Ollama at $OLLAMA_BASE — souls and memory recall will be offline."
elif ! probe_model "$CHAT_MODEL"; then
    echo "WARNING: chat model $CHAT_MODEL failed to load (usually: not enough free RAM)."
    if probe_model "$FALLBACK_MODEL"; then
        echo "         using fallback model $FALLBACK_MODEL for this stack."
        CHAT_MODEL="$FALLBACK_MODEL"
    else
        echo "         SOULS WILL BE SILENT. Free some RAM, or pull a smaller model and:"
        echo "           export PAN_LLM_MODEL=<model>   # then restart the stack"
    fi
else
    echo "llm: $CHAT_MODEL loaded and warm"
fi

# Build ragamuffin into the stack dir if missing (from the sibling checkout).
if [[ ! -x "$RAGA_BIN" ]]; then
    RAGA_SRC="$CHEZ_ROOT/ragamuffin-raga"
    [[ -d "$RAGA_SRC" ]] || RAGA_SRC="$CHEZ_ROOT/ragamuffin"
    echo "building ragamuffin from $RAGA_SRC…"
    (cd "$RAGA_SRC" && go build -o "$RAGA_BIN" ./cmd/ragamuffin)
fi

start_ragamuffin() { # $1=port $2=data_root $3=label
    local port="$1" root="$2" label="$3"
    mkdir -p "$root"/{vaults,db,logs}
    if curl -sf -o /dev/null \
        -H "Authorization: Bearer $RAGA_KEY" "http://127.0.0.1:$port/v1/briefing" 2>/dev/null; then
        echo "ragamuffin[$label]: already running on 127.0.0.1:$port"
        return
    fi
    RAGAMUFFIN_HOST=127.0.0.1 \
    RAGAMUFFIN_PORT="$port" \
    RAGAMUFFIN_AUTH_MODE=api_key \
    RAGAMUFFIN_AUTH_READ_KEY="$RAGA_KEY" \
    RAGAMUFFIN_AUTH_WRITE_KEY="$RAGA_KEY" \
    RAGAMUFFIN_VECTOR_STORE=embedded \
    RAGAMUFFIN_EMBEDDED_DB_PATH="$root/db/memory.db" \
    RAGAMUFFIN_VAULTS_ROOT="$root/vaults" \
    RAGAMUFFIN_AUTO_PROVISION_VAULTS=true \
    RAGAMUFFIN_EMBEDDING_BASE_URL="${REACHLOCK_EMBED_BASE:-http://127.0.0.1:11434/v1}" \
    RAGAMUFFIN_EMBEDDING_MODEL="${REACHLOCK_EMBED_MODEL:-nomic-embed-text}" \
    RAGAMUFFIN_EMBEDDING_DIMS="${REACHLOCK_EMBED_DIMS:-768}" \
    RAGAMUFFIN_EMBEDDING_API_KEY=local \
    RAGAMUFFIN_LLM_PROVIDER=ollama \
    RAGAMUFFIN_LLM_BASE_URL="${REACHLOCK_LLM_BASE:-http://127.0.0.1:11434}" \
    RAGAMUFFIN_LLM_MODEL="${REACHLOCK_LLM_MODEL:-$CHAT_MODEL}" \
    RAGAMUFFIN_LLM_API_KEY=local \
        "$RAGA_BIN" >"$root/logs/ragamuffin.log" 2>&1 &
    echo "ragamuffin[$label]: started on 127.0.0.1:$port (auth: api_key, log: $root/logs/ragamuffin.log)"
}

if [[ "$MODE" == "test" ]]; then
    start_ragamuffin "$RAGA_TEST_PORT" "$DATA_TEST" "test"
    sleep 1
    curl -sf -H "Authorization: Bearer $RAGA_KEY" \
        "http://127.0.0.1:$RAGA_TEST_PORT/v1/briefing" >/dev/null \
        && echo "memory[test]: ok" || echo "memory[test]: NOT answering yet"
    echo
    echo "isolated test stack — export this before any automated run:"
    echo "  export REACHLOCK_MEMORY_URL=http://127.0.0.1:$RAGA_TEST_PORT"
    echo "  export REACHLOCK_MEMORY_KEY=$RAGA_KEY"
    echo "  export REACHLOCK_VAULT_PREFIX=test-"
    exit 0
fi

start_ragamuffin "$RAGA_PORT" "$DATA" "play"

if ! pgrep -f "$PAN_BIN serve" >/dev/null; then
    PAN_LLM_BASE="$OLLAMA_BASE" \
    PAN_LLM_MODEL="$CHAT_MODEL" \
        "$PAN_BIN" serve --port "$PAN_PORT" >"$DATA/logs/pan.log" 2>&1 &
    echo "pan: started on 127.0.0.1:$PAN_PORT (log: $DATA/logs/pan.log)"
else
    echo "pan: already running"
fi

# The simulation daemon (M6): the universe tick over the Sim Protocol.
# Built from this repo; state is in-memory (the game's save owns the
# universe snapshot and pushes it back on connect).
if [[ ! -x "$SIMD_BIN" ]]; then
    echo "building reachlock-simd…"
    (cd "$REPO_ROOT/server" && go build -o "$SIMD_BIN" ./cmd/reachlock-simd)
fi
if ! pgrep -f "$SIMD_BIN" >/dev/null; then
    "$SIMD_BIN" --port "$SIM_PORT" --mods "$REPO_ROOT/godot/mods" \
        >"$DATA/logs/simd.log" 2>&1 &
    echo "simd: started on 127.0.0.1:$SIM_PORT (log: $DATA/logs/simd.log)"
else
    echo "simd: already running"
fi

sleep 1
curl -sf -H "Authorization: Bearer $RAGA_KEY" "http://127.0.0.1:$RAGA_PORT/v1/briefing" >/dev/null \
    && echo "memory: ok (auth on)" || echo "memory: NOT answering yet (model load?)"
echo "ready — run: make godot"
