#!/usr/bin/env bash
set -uo pipefail

runtime="${PG_REACH_RUNTIME:-pglite}"
label="pglite-pg_ivm"
if [[ "$runtime" == native ]]; then label="native-postgres-pg_ivm"; fi
printf 'STATUS|%s|unsupported|pg_ivm rejects recursive WITH RECURSIVE view definitions|no process started|not applicable\n' "$label" >&2
