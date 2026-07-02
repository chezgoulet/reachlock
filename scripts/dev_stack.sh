#!/usr/bin/env bash
# REACHLOCK dev stack — the two sidecars the living crew needs.
#
#   ./scripts/dev_stack.sh          start pan (mind) + ragamuffin (memory)
#   ./scripts/dev_stack.sh stop     stop both
#
# Then run the game:  make godot
# The game reads REACHLOCK_PAN_PORT (default 40707) and REACHLOCK_MEMORY_URL
# (default http://127.0.0.1:8000); both match the defaults below.
#
# Requirements: Ollama running locally with a chat model and an embedding
# model pulled (defaults: gemma4:e4b + nomic-embed-text), the pan binary
# built (cd ../pan && cargo build --release), and the ragamuffin binary
# built from its sprint01/raga branch (embedded vector store — no Qdrant,
# no Docker).
set -euo pipefail

CHEZ_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DATA="$HOME/.local/share/reachlock"
PAN_BIN="${PAN_BIN:-$CHEZ_ROOT/pan/target/release/pan}"
RAGA_BIN="${RAGA_BIN:-$DATA/bin/ragamuffin}"
PAN_PORT="${REACHLOCK_PAN_PORT:-40707}"

if [[ "${1:-}" == "stop" ]]; then
    pkill -f "$PAN_BIN serve" 2>/dev/null && echo "pan: stopped" || echo "pan: not running"
    pkill -f "$RAGA_BIN" 2>/dev/null && echo "ragamuffin: stopped" || echo "ragamuffin: not running"
    exit 0
fi

mkdir -p "$DATA"/{vaults,db,logs,bin}

# Build ragamuffin into the stack dir if missing (from the sibling checkout).
if [[ ! -x "$RAGA_BIN" ]]; then
    RAGA_SRC="$CHEZ_ROOT/ragamuffin-raga"
    [[ -d "$RAGA_SRC" ]] || RAGA_SRC="$CHEZ_ROOT/ragamuffin"
    echo "building ragamuffin from $RAGA_SRC…"
    (cd "$RAGA_SRC" && go build -o "$RAGA_BIN" ./cmd/ragamuffin)
fi

if ! pgrep -f "$RAGA_BIN" >/dev/null; then
    RAGAMUFFIN_VECTOR_STORE=embedded \
    RAGAMUFFIN_EMBEDDED_DB_PATH="$DATA/db/memory.db" \
    RAGAMUFFIN_VAULTS_ROOT="$DATA/vaults" \
    RAGAMUFFIN_AUTO_PROVISION_VAULTS=true \
    RAGAMUFFIN_EMBEDDING_BASE_URL="${REACHLOCK_EMBED_BASE:-http://127.0.0.1:11434/v1}" \
    RAGAMUFFIN_EMBEDDING_MODEL="${REACHLOCK_EMBED_MODEL:-nomic-embed-text}" \
    RAGAMUFFIN_EMBEDDING_DIMS="${REACHLOCK_EMBED_DIMS:-768}" \
    RAGAMUFFIN_EMBEDDING_API_KEY=local \
    RAGAMUFFIN_PORT=8000 \
        "$RAGA_BIN" >"$DATA/logs/ragamuffin.log" 2>&1 &
    echo "ragamuffin: started (embedded store, log: $DATA/logs/ragamuffin.log)"
else
    echo "ragamuffin: already running"
fi

if ! pgrep -f "$PAN_BIN serve" >/dev/null; then
    PAN_LLM_BASE="${PAN_LLM_BASE:-http://127.0.0.1:11434}" \
    PAN_LLM_MODEL="${PAN_LLM_MODEL:-gemma4:e4b}" \
        "$PAN_BIN" serve --port "$PAN_PORT" >"$DATA/logs/pan.log" 2>&1 &
    echo "pan: started on 127.0.0.1:$PAN_PORT (log: $DATA/logs/pan.log)"
else
    echo "pan: already running"
fi

sleep 1
curl -sf "http://127.0.0.1:8000/v1/briefing" >/dev/null && echo "memory: ok" || echo "memory: NOT answering yet (model load?)"
echo "ready — run: make godot"
