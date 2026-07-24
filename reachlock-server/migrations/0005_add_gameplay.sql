-- S54: gameplay tables for careers, reputation, and criminal records.

CREATE TABLE IF NOT EXISTS careers (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id       UUID NOT NULL REFERENCES players(id),
    path_id         VARCHAR(128) NOT NULL,
    current_rank    INTEGER NOT NULL DEFAULT 1,
    total_prestige  BIGINT NOT NULL DEFAULT 0,
    progress        JSONB DEFAULT '{}',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(player_id, path_id)
);

CREATE TABLE IF NOT EXISTS reputation (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id   UUID NOT NULL REFERENCES players(id),
    faction_id  VARCHAR(128) NOT NULL,
    universe    universe_tier NOT NULL,
    standing    INTEGER NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(player_id, faction_id, universe)
);

CREATE TABLE IF NOT EXISTS criminal_records (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id       UUID NOT NULL REFERENCES players(id),
    universe        universe_tier NOT NULL,
    crime_type      VARCHAR(128),
    description     TEXT,
    bounty_amount   BIGINT,
    issuer_faction  VARCHAR(128),
    recorded_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    cleared_at      TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_criminal_player ON criminal_records(player_id);
