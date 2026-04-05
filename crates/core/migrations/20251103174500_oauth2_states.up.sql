CREATE TABLE IF NOT EXISTS oauth2_states (
    state TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    pkce_verifier TEXT NOT NULL,
    redirect TEXT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_oauth2_states_created_at
    ON oauth2_states (created_at);
