# S112 — Data Corruption Fixes: Inverted LLM Success Flag (H13, M23)

**Wave: Hotfix · Depends on:** None (standalone fix)

## Outcome

Two lines in `llm_proxy.rs` record `!success` when they should record `success`. Every successful LLM call is logged as a failure in the cost store and provider health tracker. Every failure is logged as a success. This is data corruption — fix both lines.

## Context

`reachlock-server/src/services/llm_proxy.rs` defines the LLM call pipeline. At line 235, `success` is set correctly:

```rust
// line 235 — correct
let success = result.is_ok();
self.metrics.record(latency, success);  // line 236 — correct
```

But the cost record and health record INVERT the flag:

```rust
// line 271 — BUG: !success means opposite of actual outcome
self.costs.record(super::cost::LlmCallRecord {
    // ... other fields ...
    success: !success,   // ← INVERTED
});

// line 274 — BUG: same inversion, different target
h.record(!success);     // ← INVERTED
```

**Effect:** 
- LLM call succeeds → `costs` records `success: false` (logged as failure)
- LLM call fails → `costs` records `success: true` (logged as success)
- Provider health shows the opposite of reality

### Key files

| File | Role |
|------|------|
| `reachlock-server/src/services/llm_proxy.rs` | Lines 271 and 274 — the two bugs |

## Deliverables

### 1. Fix line 271 — cost record success flag

**Before:**
```rust
            success: !success,
```

**After:**
```rust
            success,
```

### 2. Fix line 274 — health record success flag

**Before:**
```rust
            h.record(!success);
```

**After:**
```rust
            h.record(success);
```

### 3. Verify fix by reading the full block

The corrected block (lines 260-275) must read:

```rust
        self.costs.record(super::cost::LlmCallRecord {
            timestamp: format!("{ts}"),
            player_id: player_id.to_string(),
            universe: tier,
            provider: format!("{tier:?}"),
            model: format!("{tier:?}"),
            contract_id: contract_id.to_string(),
            latency_ms: latency,
            prompt_tokens,
            completion_tokens,
            estimated_cost_micros: cost,
            success,                             // ← was !success
        });
        if let Ok(ref mut h) = self.health.try_lock() {
            h.record(success);                   // ← was !success
        }
```

## Acceptance gates

```bash
cargo build -p reachlock-server
cargo clippy -p reachlock-server -- -D warnings
cargo test -p reachlock-server

# Manual verification (requires server running with LLM endpoint):
# 1. Trigger an LLM call (deliberation or dialogue)
# 2. Check cost store: success=true for a response, success=false for an error
# 3. Verify provider health: OK calls count as healthy, failures count as unhealthy

make check
```

## Non-goals

- Adding tests for cost recording (the fix is mechanical — the flag was inverted, restore correct logic)
- API for cost store inspection (existing cost store interface unchanged)
- Changing the `LlmCallRecord` struct

## Gotchas

- **`success` is a `bool`, not a `Result`.** The `!` operator inverts `bool`. `!true` = `false`. This is purely a typo-level bug.
- **Both lines must be fixed.** Line 271 writes to the cost store. Line 274 writes to the health tracker. Fix both or neither — fixing one and not the other creates a mismatch.
- **No test currently covers cost recording.** This is a known gap (see test coverage section of the review). The fix is mechanically correct by reading the variable assignment at line 235.
- **Double-check `self.metrics.record(latency, success)` at line 236.** This line is CORRECT — it uses `success`, not `!success`. Leave it as-is. Only lines 271 and 274 are wrong.
