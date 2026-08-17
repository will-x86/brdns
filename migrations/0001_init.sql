-- 0001_init.sql - group-as-tenant schema.
--
-- Timestamps are unix epoch seconds (BIGINT)
-- Enum-like columns are TEXT with CHECK constraints so the schema stays portable

-- Tenants, identified by account number
CREATE TABLE IF NOT EXISTS accounts (
    id              BIGSERIAL PRIMARY KEY,
    account_number  TEXT NOT NULL UNIQUE,
    created_at      BIGINT NOT NULL,
    updated_at      BIGINT NOT NULL
);

-- Devices/API keys under an account
CREATE TABLE IF NOT EXISTS members (
    id          BIGSERIAL PRIMARY KEY,
    account_id  BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    created_at  BIGINT NOT NULL,
    UNIQUE (account_id, name)
);

-- Ordered rule list; first match wins
CREATE TABLE IF NOT EXISTS rules (
    id            BIGSERIAL PRIMARY KEY,
    account_id    BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    position      INTEGER NOT NULL,
    action        TEXT NOT NULL CHECK (action IN ('allow','deny','limit')),
    target_type   TEXT NOT NULL CHECK (target_type IN ('domain','wildcard','category')),
    target_value  TEXT NOT NULL,
    limit_count   BIGINT,
    limit_window  TEXT CHECK (limit_window IN ('hour','day','week','month')),
    enabled       BOOLEAN NOT NULL DEFAULT TRUE,
    created_at    BIGINT NOT NULL,
    updated_at    BIGINT NOT NULL,
    CONSTRAINT rules_limit_requires_window CHECK (
        action <> 'limit'
        OR (limit_count IS NOT NULL AND limit_window IS NOT NULL)
    )
);
CREATE INDEX IF NOT EXISTS idx_rules_account ON rules (account_id, position);

-- DNS upstreams. account_id NULL = global preset.
CREATE TABLE IF NOT EXISTS upstreams (
    id          BIGSERIAL PRIMARY KEY,
    account_id  BIGINT REFERENCES accounts(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    protocol    TEXT NOT NULL CHECK (protocol IN ('dot','doh','udp')),
    host        TEXT NOT NULL,
    port        INTEGER NOT NULL,
    addr        TEXT,
    is_preset   BOOLEAN NOT NULL DEFAULT FALSE,
    created_at  BIGINT NOT NULL,
    UNIQUE (account_id, name)
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_upstreams_preset_name
    ON upstreams (name) WHERE is_preset;

-- Per-rule, per-window usage counters
CREATE TABLE IF NOT EXISTS quota_counters (
    id            BIGSERIAL PRIMARY KEY,
    account_id    BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    rule_id       BIGINT NOT NULL REFERENCES rules(id) ON DELETE CASCADE,
    window_start  BIGINT NOT NULL,
    count         BIGINT NOT NULL DEFAULT 0,
    UNIQUE (rule_id, window_start)
);

-- Blocklist/category data
CREATE TABLE IF NOT EXISTS domains (
    domain      TEXT PRIMARY KEY,
    updated_at  BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS domain_categories (
    domain      TEXT NOT NULL REFERENCES domains(domain) ON DELETE CASCADE,
    category    TEXT NOT NULL,
    PRIMARY KEY (domain, category)
);
CREATE INDEX IF NOT EXISTS idx_domain_categories_category
    ON domain_categories (category);

-- Community blocklist feeds (OISD, Hagezi, StevenBlack, AdGuard...).
CREATE TABLE IF NOT EXISTS blocklist_feeds (
    id          BIGSERIAL PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    url         TEXT NOT NULL,
    format      TEXT NOT NULL DEFAULT 'plain',
    updated_at  BIGINT NOT NULL
);
