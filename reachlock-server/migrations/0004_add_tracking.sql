-- S54: tracking tables for audit, LLM calls, health, and offline tokens.

CREATE TABLE IF NOT EXISTS audit_log (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    admin_player_id TEXT REFERENCES players(id),
    action          VARCHAR(256) NOT NULL,
    target_type     VARCHAR(64),
    target_id       VARCHAR(128),
    details         JSONB DEFAULT '{}',
    occurred_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_audit_time ON audit_log(occurred_at DESC);

CREATE TABLE IF NOT EXISTS llm_calls (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider            VARCHAR(64),
    model               VARCHAR(128),
    player_id           TEXT REFERENCES players(id),
    contract_id         VARCHAR(128),
    tokens_input        INTEGER NOT NULL DEFAULT 0,
    tokens_output       INTEGER NOT NULL DEFAULT 0,
    latency_ms          INTEGER NOT NULL DEFAULT 0,
    success             BOOLEAN NOT NULL DEFAULT true,
    failure_reason      VARCHAR(256),
    cost_micro_credits  INTEGER NOT NULL DEFAULT 0,
    occurred_at         TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_llm_calls_time ON llm_calls(occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_llm_calls_player ON llm_calls(player_id);

CREATE TABLE IF NOT EXISTS health_checks (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    service     VARCHAR(64) NOT NULL,
    status      VARCHAR(32) NOT NULL,
    detail      TEXT,
    latency_ms  INTEGER NOT NULL DEFAULT 0,
    checked_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_health_time ON health_checks(checked_at DESC);

CREATE TABLE IF NOT EXISTS offline_entitlements (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id   TEXT NOT NULL REFERENCES players(id),
    token       VARCHAR(256) UNIQUE NOT NULL,
    tier        universe_tier NOT NULL,
    issued_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at  TIMESTAMPTZ NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_offline_token ON offline_entitlements(token);
