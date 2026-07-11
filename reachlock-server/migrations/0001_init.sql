-- ReachLock v2 initial schema (spec §11).
-- Note vs spec: UNIQUE(universe, system_id, COALESCE(object_id, '')) is not
-- valid Postgres DDL — constraints can't contain expressions. A generated
-- column (object_key) carries the coalesced value and the UNIQUE constraint
-- sits on that. Same semantics, legal SQL.

-- Universe tiers: architectural hook only. No billing, no subscriptions.
CREATE TYPE universe_tier AS ENUM ('classic', 'fair_play', 'spectrum', 'byok');

CREATE TABLE players (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username        VARCHAR(32) UNIQUE NOT NULL,
    display_name    VARCHAR(64),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_login      TIMESTAMPTZ
);
-- No tier column on players: tier is implicit from each character's
-- universe. When/if monetization returns, add it (spec §7 future hook).

CREATE TABLE characters (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id   UUID NOT NULL REFERENCES players(id),
    name        VARCHAR(64) NOT NULL,
    universe    universe_tier NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Seed ledger: first-write-wins (spec §4, adversarial finding #1).
CREATE TABLE seeds (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    discoverer_id   UUID NOT NULL,
    universe        universe_tier NOT NULL,
    system_id       VARCHAR(64) NOT NULL,
    object_id       VARCHAR(64),        -- NULL = whole system
    object_key      VARCHAR(64) GENERATED ALWAYS AS (COALESCE(object_id, '')) STORED,
    seed            BIGINT NOT NULL CHECK (seed >= 0 AND seed < 9007199254740992), -- 2^53
    discovered      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    modified        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    diffs           JSONB NOT NULL DEFAULT '{}',

    UNIQUE(universe, system_id, object_key)
);
CREATE INDEX idx_seeds_system ON seeds(universe, system_id);
CREATE INDEX idx_seeds_discoverer ON seeds(discoverer_id, universe);

-- Contract evaluation signatures (spec §6, online audit trail).
CREATE TABLE eval_signatures (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    character_id    UUID NOT NULL REFERENCES characters(id),
    contract_id     VARCHAR(64) NOT NULL,
    tick            BIGINT NOT NULL,
    action          JSONB NOT NULL,
    signature       VARCHAR(128) NOT NULL,
    prev_signature  VARCHAR(128),
    verified        BOOLEAN NOT NULL DEFAULT false,
    occurred_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_signatures_chain ON eval_signatures(character_id, contract_id, tick);

CREATE TABLE universe_events (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    universe    universe_tier NOT NULL,
    event_type  VARCHAR(64) NOT NULL,
    payload     JSONB,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE byok_keys (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id   UUID NOT NULL REFERENCES players(id),
    provider    VARCHAR(64) NOT NULL,
    api_key_encrypted TEXT NOT NULL,
    is_active   BOOLEAN NOT NULL DEFAULT true,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Authored content overrides (spec §10). Same generated-column fix for the
-- nullable universe in the uniqueness rule.
CREATE TABLE content_overrides (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    system_id     VARCHAR(64) NOT NULL,
    object_id     VARCHAR(64),
    universe      universe_tier,           -- NULL = all universes
    universe_key  VARCHAR(16) GENERATED ALWAYS AS (COALESCE(universe::text, 'all')) STORED,
    asset_type    VARCHAR(32) NOT NULL,
    seed          BIGINT NOT NULL,
    priority      SMALLINT NOT NULL DEFAULT 50,   -- 0 procedural, 50 curated, 75 event, 100 authoritative
    expires_at    TIMESTAMPTZ,              -- NULL = permanent
    content       JSONB NOT NULL,
    available_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(system_id, object_id, asset_type, universe_key)
);
CREATE INDEX idx_overrides_system ON content_overrides(system_id, universe);

CREATE TABLE contracts (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    character_id UUID NOT NULL REFERENCES characters(id),
    label        VARCHAR(128),
    contract     JSONB NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE content_deployments (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    override_id UUID NOT NULL REFERENCES content_overrides(id),
    deployed_by VARCHAR(64),
    version     INTEGER NOT NULL DEFAULT 1,
    checksum    VARCHAR(64) NOT NULL,
    deployed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
