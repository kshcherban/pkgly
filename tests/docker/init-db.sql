-- Integration test database initialization script
-- This file is intentionally minimal - it only creates the database
-- The Pkgly application will run migrations to create tables
-- Then seed-data.sql will be executed to insert test data

-- Database is created by POSTGRES_DB environment variable
-- This file exists to satisfy Docker's entrypoint requirements
SELECT 'Database pkgly_test is ready for migrations' as status;
