-- ABOUTME: Adds optimistic-concurrency revisions to Docker object accounting rows.
-- ABOUTME: Lets reconciliation avoid overwriting writes committed after its scan.
ALTER TABLE docker_objects
    ADD COLUMN revision BIGINT NOT NULL DEFAULT 0;
