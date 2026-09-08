#!/usr/bin/env bash
set -euo pipefail

dir=$(cd "$(dirname "$0")" && pwd)
pglite=$($dir/3_postgres_ivm_unsupported.sh 2>&1 | grep '^STATUS|')
native=$(PG_REACH_RUNTIME=native $dir/3_postgres_ivm_unsupported.sh 2>&1 | grep '^STATUS|')
[[ "$pglite" == "STATUS|pglite-pg_ivm|unsupported|pg_ivm rejects recursive WITH RECURSIVE view definitions|no process started|not applicable" ]]
[[ "$native" == "STATUS|native-postgres-pg_ivm|unsupported|pg_ivm rejects recursive WITH RECURSIVE view definitions|no process started|not applicable" ]]
