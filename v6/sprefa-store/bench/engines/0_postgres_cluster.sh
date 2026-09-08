#!/usr/bin/env bash

# Sourced lifecycle helper for the opt-in PostgreSQL shootout arms.

pg_bench_start() {
  local bench_dir="$1"
  local lab_dir
  lab_dir=$(cd "$bench_dir/../../labs/exec_shootout/postgres_pglite_ivm" && pwd)
  local postgres_prefix="${IVM_POSTGRES_PREFIX:-$lab_dir/.work/postgres-18.6}"

  PG_BENCH_RUN_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/sprefa-store-pg.XXXXXX")
  export PG_BENCH_RUN_ROOT
  export PG_NATIVE_READY=0
  export PG_NATIVE_REASON="task-local PostgreSQL 18.6 executable is unavailable"
  export PG_DEPENDENCY_DIR="$lab_dir"

  if [[ ! -x "$postgres_prefix/bin/postgres" ]]; then
    return 0
  fi

  local cluster_dir="$PG_BENCH_RUN_ROOT/cluster"
  local socket_dir="$PG_BENCH_RUN_ROOT/socket"
  mkdir -p "$socket_dir"
  if ! "$postgres_prefix/bin/initdb" -D "$cluster_dir" --auth=trust \
      --no-locale --encoding=UTF8 >"$PG_BENCH_RUN_ROOT/initdb.log" 2>&1; then
    PG_NATIVE_REASON="task-local initdb failed; see $PG_BENCH_RUN_ROOT/initdb.log"
    export PG_NATIVE_REASON
    return 0
  fi

  local options
  options="-c listen_addresses='' -c unix_socket_directories='$socket_dir' -c fsync=on -c synchronous_commit=on -c full_page_writes=on -c statement_timeout=120000 -c shared_buffers=${PG_SHARED_BUFFERS:-128MB} -c work_mem=${PG_WORK_MEM:-16MB} -c temp_file_limit=${PG_TEMP_FILE_LIMIT:-2048MB}"
  if ! "$postgres_prefix/bin/pg_ctl" -D "$cluster_dir" \
      -l "$PG_BENCH_RUN_ROOT/postgres.log" -o "$options" start -w >/dev/null 2>&1; then
    PG_NATIVE_REASON="task-local PostgreSQL startup failed; see $PG_BENCH_RUN_ROOT/postgres.log"
    export PG_NATIVE_REASON
    return 0
  fi

  PG_NATIVE_STARTED=1
  export PGHOST="$socket_dir"
  export PGPORT=5432
  PGUSER=$(id -un)
  export PGUSER
  export PGDATABASE=sprefa_store_bench
  if ! "$postgres_prefix/bin/createdb" "$PGDATABASE" >"$PG_BENCH_RUN_ROOT/createdb.log" 2>&1; then
    PG_NATIVE_REASON="task-local database creation failed; see $PG_BENCH_RUN_ROOT/createdb.log"
    export PG_NATIVE_REASON
    return 0
  fi
  PG_POSTMASTER_PID=$(head -1 "$cluster_dir/postmaster.pid")
  export PG_POSTMASTER_PID
  export PG_NATIVE_READY=1
  export PG_NATIVE_REASON=""
  PG_NATIVE_BIN="$postgres_prefix/bin"
  PG_NATIVE_CLUSTER="$cluster_dir"
}

pg_bench_stop() {
  if [[ "${PG_NATIVE_STARTED:-0}" -eq 1 && -n "${PG_NATIVE_BIN:-}" && \
        -n "${PG_NATIVE_CLUSTER:-}" ]]; then
    "$PG_NATIVE_BIN/pg_ctl" -D "$PG_NATIVE_CLUSTER" -m immediate stop >/dev/null 2>&1 || true
  fi
  case "${PG_BENCH_RUN_ROOT:-}" in
    "${TMPDIR:-/tmp}"/sprefa-store-pg.*) rm -rf -- "$PG_BENCH_RUN_ROOT" ;;
  esac
}
