#!/bin/bash
# Seed database with test data after migrations have run
set -euo pipefail

echo "Waiting for Pkgly to complete migrations..."

# Wait for Pkgly to be healthy (which means migrations are done)
MAX_WAIT=60
ELAPSED=0

sleep 2
while [ $ELAPSED -lt $MAX_WAIT ]; do
    # Use pg_isready to check if we can connect, and psql to check if tables exist
    if PGPASSWORD=pkgly psql -h postgres -U pkgly -d pkgly_test -c "SELECT 1 FROM users LIMIT 1;" > /dev/null 2>&1; then
        echo "Pkgly migrations complete, seeding database..."
        break
    fi
    sleep 2
    ((ELAPSED+=2))

    if [ $((ELAPSED % 10)) -eq 0 ]; then
        echo "  Still waiting for migrations... (${ELAPSED}s / ${MAX_WAIT}s)"
    fi
done

if [ $ELAPSED -ge $MAX_WAIT ]; then
    echo "ERROR: Migrations did not complete within ${MAX_WAIT} seconds"
    exit 1
fi

# Apply seed data
echo "Applying seed data..."
PGPASSWORD=pkgly psql -h postgres -U pkgly -d pkgly_test -f /seed-data.sql

if [ $? -eq 0 ]; then
    echo "✓ Database seeded successfully"
    echo "📢 Pkgly needs to be restarted to load the seeded repositories"
    echo "   Run: docker compose restart pkgly"
    exit 0
else
    echo "✗ Failed to seed database"
    exit 1
fi
