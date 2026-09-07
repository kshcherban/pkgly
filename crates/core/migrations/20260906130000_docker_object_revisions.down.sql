-- ABOUTME: Removes optimistic-concurrency revisions from Docker accounting rows.
-- ABOUTME: Restores the Docker accounting table to its pre-revision schema.
ALTER TABLE docker_objects
    DROP COLUMN revision;
