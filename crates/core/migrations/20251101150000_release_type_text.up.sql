ALTER TABLE project_versions
    ALTER COLUMN release_type TYPE TEXT USING release_type::text;
