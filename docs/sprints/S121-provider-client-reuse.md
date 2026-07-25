# S121 — Provider Client Reuse (M24)

**Wave: Hotfix · Depends on:** None (server standalone fix, 1 file)

## Outcome

`http_client()` in `providers.rs` returns a new `reqwest::Client` on every call. The client is cached and reused across LLM requests, enabling connection pooling and TLS session reuse.

## Context

**File:** `reachlock-server/src/services/providers.rs` line 292

```rust
fn http_client(timeout: Duration) -> Result<reqwest::Client, ProviderError> {
    reqwest::Client::builder()
        .timeout(timeout.min(SERVER_TIMEOUT_CAP))
        .build()
        .map_err(|e| ProviderError::Provider(format!("client build: {e}")))
}
```

Every call to `http_client()` builds a new `reqwest::Client` from scratch. reqwest internally uses a connection pool — but only within the SAME client instance. Building a new client for each request means:
- No TCP connection reuse
- No TLS session reuse
- New DNS resolution per request
- Higher latency for every LLM call (~50-200ms overhead per call)

## Fix

Replace the function with a lazy static or a struct field:

**Option A — Lazy static (simplest, 5 lines):**

```rust
use std::sync::LazyLock;

static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(SERVER_TIMEOUT_CAP) // use the cap directly, or a reasonable default
        .build()
        .expect("failed to build HTTP client")
});

// Replace all calls to `http_client(timeout)` with:
// &HTTP_CLIENT
```

**Option B — Store on LlmService (cleaner):**

Add `client: reqwest::Client` to `LlmService` struct in `llm_proxy.rs`:

```rust
pub struct LlmService {
    pub client: reqwest::Client,   // <-- NEW
    pub fairplay: AnyProvider,
    pub spectrum: AnyProvider,
    // ... rest unchanged
}
```

Initialize in `LlmService::from_env()`:
```rust
fn from_env() -> Self {
    let client = reqwest::Client::builder()
        .timeout(SERVER_TIMEOUT_CAP)
        .build()
        .expect("failed to build HTTP client");
    Self {
        client,
        // ... rest
    }
}
```

Then update `http_client` to take `&reqwest::Client` instead of building one:
```rust
fn http_client(client: &reqwest::Client, timeout: Duration) -> &reqwest::Client {
    // Ignore timeout — use the pre-built client
    // (or validate timeout against the client's configured timeout)
    client
}
```

**Recommended:** Option A (LazyLock) is simplest — no struct changes, no call-site changes. Just delete `http_client()` and replace calls with `&HTTP_CLIENT`.

### Find all callers and replace

```bash
cd /home/c/git/chezgoulet/reachlock && rg "http_client" --include '*.rs'
```

Every call `http_client(timeout)` → `&HTTP_CLIENT`. If callers pass different timeouts, the LazyLock approach uses a fixed timeout (`SERVER_TIMEOUT_CAP`).

---

## Acceptance gates

```bash
cargo build -p reachlock-server
cargo test -p reachlock-server
cargo clippy -p reachlock-server -- -D warnings

make check
```

## Gotchas

- **`SERVER_TIMEOUT_CAP` must be a constant accessible at static init time.** `LazyLock::new(|| ...)` runs lazily so it's fine. If the constant is a `const`, use it directly. If it's computed at startup, use Option B (store on LlmService).
- **Different providers may want different timeouts.** Check if any provider passes a specific timeout to `http_client()`. If so, Option B is better — each provider can reference the same client with its own request-level timeout via `.timeout()` on the request builder.
- **`reqwest::Client` is `Send + Sync`.** It's safe to share across threads.
- **The `http_client` function may be called on every provider request (FairPlay, Spectrum, BYOK).** Verify all three paths use it. A single static client handles all three — connection pooling is per-host.
