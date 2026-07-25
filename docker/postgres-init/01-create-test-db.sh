#!/bin/sh
# Runs once, on first init of an empty data directory.
#
# Creates a SEPARATE database for the live-Postgres test battery. The tests
# run migrations, insert, and delete; pointing REACHLOCK_TEST_DB at the same
# database as REACHLOCK_DB means `make db-test` silently destroys whatever
# you were playing with.
set -eu

psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<-EOSQL
    CREATE DATABASE reachlock_test OWNER $POSTGRES_USER;
EOSQL

echo "created database reachlock_test (owner: $POSTGRES_USER)"
