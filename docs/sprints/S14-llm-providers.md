# S14 — Real LLM Providers Behind the Proxy

**Spec:** §7 (routing, tiers, BYOK), §8 (LLM proxy) · **Wave 4 ·
Depends on:** S03

## Outcome

`llm.call` reaches real models: FairPlay routes to a local llama.cpp/Ollama
endpoint, Spectrum to an OpenAI-compatible cloud endpoint, BYOK to the
player's own key — with rate limiting, timeouts that produce clean
`llm.failed` (never hangs), and the stub retained as the test/dev provider.
Tier gating stays exactly as it is.

## Context

- `reachlock-server/src/services/llm_proxy.rs` has the tier gate
  (`inference_grant`) and the deterministic stub. Its fn signature is the
  contract: swap the guts, keep the shape.
- The BYOK table exists in the migration (`byok_keys`,
  `api_key_encrypted`). S03 gives you Postgres and sessions.
- v1 lesson (memory, don't rediscover): Ollama's OpenAI-compat endpoint
  ignored `think:false` on reasoning models — prefer the native `/api/chat`
  for Ollama, and strip reasoning traces from responses.

## Freeze first

`Provider` trait in the server: `async fn complete(&self, req:
InferenceRequest) -> Result<InferenceResponse, ProviderError>` where
`InferenceRequest { system_prompt, context_json, max_tokens, timeout }`.
Providers: `Stub` (existing behavior), `OpenAiCompat { base_url, api_key,
model }` (covers Ollama-compat, OpenRouter, most BYOK), `OllamaNative
{ base_url, model }`. Config from env: `REACHLOCK_FAIRPLAY_URL/MODEL`,
`REACHLOCK_SPECTRUM_URL/KEY/MODEL`; unset → Stub (dev default, CI default).

## Deliverables

- [ ] The three providers, with hard timeouts (contract's `timeout_ms`
      bounded by a server cap) and error taxonomy: Timeout / RateLimited /
      ProviderError / BadResponse → all become `llm.failed { reason }`.
- [ ] Response shaping: the model is asked (via a wrapper system prompt) for
      `{ action, reasoning }` JSON matching the contract's action verbs;
      unparseable output = BadResponse (which IS the spec §18
      model-collapse failure mode — S15 consumes the taxonomy).
- [ ] BYOK: `POST /byok` stores a player's provider+key encrypted at rest
      (real crypto — e.g. age/chacha20poly1305 with a server key from env,
      not base64); Byok-tier calls decrypt and route. Keys never appear in
      logs.
- [ ] Rate limiting on the proxy: token-bucket per player per universe
      (in-memory now, SessionStore-style trait so Redis can back it later).
      Exceeded = `llm.failed { reason: "rate_limited" }` — the crew "is
      overwhelmed", a valid fiction.
- [ ] Deliberation timing telemetry: per-call latency recorded (tracing +
      an in-memory histogram endpoint `/metrics` text format).
- [ ] Integration test against a fake HTTP server (spawn an axum stub in the
      test) proving: happy path, timeout→failed, garbage→BadResponse, rate
      limit trips. CI runs with Stub only — no network in CI.

## Acceptance gates

```
cargo test -p reachlock-server llm       # fake-provider battery
# manual, with ollama running locally:
REACHLOCK_FAIRPLAY_URL=http://127.0.0.1:11434 make server
# → in-client X anomaly (online) returns a real model's reasoning
make check
```

## Non-goals

Failure-probability gameplay model (S15). Client changes (S02 already
renders responses). Model quality tiers/param-cap enforcement beyond config
(the ≤8B cap is a deployment choice, not code — document it in the config).
Prompt engineering for souls (S16).

## Gotchas

- The WS handler awaits the LLM call inline per connection; long calls
  block that player's message loop — move the call to a spawned task
  replying via the session's out_tx (the plumbing already supports it).
- Never log context_json at info level — it will contain player content.
- `reqwest` with rustls (not openssl) to keep builds portable.
