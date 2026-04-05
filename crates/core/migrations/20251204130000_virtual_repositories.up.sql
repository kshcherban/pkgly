-- Add virtual repository membership table for virtual NPM repositories

CREATE TABLE IF NOT EXISTS virtual_repository_members (
    id SERIAL PRIMARY KEY,
    virtual_repository_id UUID NOT NULL,
    member_repository_id UUID NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT fk_virtual_repository_members_virtual FOREIGN KEY (virtual_repository_id)
        REFERENCES repositories (id) ON DELETE CASCADE,
    CONSTRAINT fk_virtual_repository_members_member FOREIGN KEY (member_repository_id)
        REFERENCES repositories (id) ON DELETE CASCADE,
    CONSTRAINT unique_virtual_member UNIQUE (virtual_repository_id, member_repository_id)
);

CREATE INDEX IF NOT EXISTS idx_virtual_members_priority
    ON virtual_repository_members (virtual_repository_id, priority, member_repository_id);
