#!/usr/bin/env bash
set -euo pipefail

lab_dir=$(cd "$(dirname "$0")" && pwd)
profile=${1:-smoke}
output=${2:-"$lab_dir/results/crossover-$profile.jsonl"}
postgres_prefix=${IVM_POSTGRES_PREFIX:-"$lab_dir/.work/postgres-18.6"}
run_root=$(mktemp -d "/tmp/pgx.XXXXXX")
active_cluster=""
deadline_seconds=$(date +%s)
deadline_epoch_ms=$((deadline_seconds * 1000 + 1200000))
overall_status=0

cleanup() {
  cleanup_status=$?
  if [[ "$cleanup_status" -ne 0 ]]; then
    for postgres_log in "$run_root"/postgres-*.log; do
      if [[ -f "$postgres_log" ]]; then
        printf 'PostgreSQL failure log: %s\n' "$postgres_log" >&2
        sed -n '1,240p' "$postgres_log" >&2
      fi
    done
  fi
  if [[ -n "$active_cluster" ]]; then
    "$postgres_prefix/bin/pg_ctl" -D "$active_cluster" -m immediate stop >/dev/null 2>&1 || true
  fi
  case "$run_root" in
    /tmp/pgx.*) rm -rf -- "$run_root" ;;
  esac
  exit "$cleanup_status"
}
trap cleanup EXIT

case "$profile" in
  smoke) budgets=(constrained) ;;
  full|diagnostic) budgets=(constrained roomy) ;;
  *) printf 'usage: %s [smoke|full|diagnostic] [output.jsonl]\n' "$0" >&2; exit 2 ;;
esac

if [[ ! -x "$postgres_prefix/bin/postgres" ]]; then
  printf 'missing task-local PostgreSQL at %s\n' "$postgres_prefix" >&2
  exit 2
fi
if [[ ! -d "$lab_dir/node_modules/pg" ]]; then
  printf 'missing task-local pg package at %s/node_modules/pg\n' "$lab_dir" >&2
  exit 2
fi

mkdir -p "$(dirname "$output")"
: > "$output"

for budget in "${budgets[@]}"; do
  cluster_dir="$run_root/cluster-$budget"
  socket_dir="$run_root/socket-$budget"
  part="$run_root/$budget.jsonl"
  mkdir -p "$socket_dir"
  "$postgres_prefix/bin/initdb" -D "$cluster_dir" --auth=trust --no-locale --encoding=UTF8 >/dev/null

  case "$budget" in
    constrained)
      export IVM_SHARED_BUFFERS=32MB
      export IVM_WORK_MEM=1MB
      export IVM_EFFECTIVE_CACHE_SIZE=64MB
      export IVM_MAINTENANCE_WORK_MEM=32MB
      ;;
    roomy)
      export IVM_SHARED_BUFFERS=256MB
      export IVM_WORK_MEM=32MB
      export IVM_EFFECTIVE_CACHE_SIZE=2GB
      export IVM_MAINTENANCE_WORK_MEM=256MB
      ;;
  esac
  export IVM_TEMP_FILE_LIMIT=2GB
  postgres_options="-c listen_addresses='' -c unix_socket_directories='$socket_dir' -c shared_preload_libraries='pg_ivm' -c fsync=on -c synchronous_commit=on -c full_page_writes=on -c statement_timeout=120000 -c track_io_timing=on -c max_connections=16 -c shared_buffers=$IVM_SHARED_BUFFERS -c work_mem=$IVM_WORK_MEM -c effective_cache_size=$IVM_EFFECTIVE_CACHE_SIZE -c maintenance_work_mem=$IVM_MAINTENANCE_WORK_MEM -c temp_file_limit=$IVM_TEMP_FILE_LIMIT"
  startup_before=$(node -p 'Date.now()')
  "$postgres_prefix/bin/pg_ctl" -D "$cluster_dir" -l "$run_root/postgres-$budget.log" -o "$postgres_options" start -w >/dev/null
  startup_after=$(node -p 'Date.now()')
  active_cluster="$cluster_dir"

  export IVM_SERVER_STARTUP_MS=$((startup_after - startup_before))
  export IVM_RUN_ROOT="$run_root/cases-$budget"
  export IVM_POSTMASTER_PID
  IVM_POSTMASTER_PID=$(sed -n '1p' "$cluster_dir/postmaster.pid")
  export PGHOST="$socket_dir"
  export PGPORT=5432
  export PGUSER
  PGUSER=$(id -un)
  export PGDATABASE_NATIVE_QUERY="crossover_query"
  export PGDATABASE_NATIVE_IVM="crossover_ivm"
  "$postgres_prefix/bin/createdb" "$PGDATABASE_NATIVE_QUERY"
  "$postgres_prefix/bin/createdb" "$PGDATABASE_NATIVE_IVM"

  runner_args=(--profile "$profile" --budget "$budget" --output "$part" --deadline-epoch-ms "$deadline_epoch_ms" --timeout-ms 120000)
  if [[ "$profile" == "full" ]]; then
    runner_args+=(--warmups 1 --repetitions 3)
  else
    runner_args+=(--warmups 0 --repetitions 1)
  fi
  set +e
  node "$lab_dir/12_crossover_runner.mjs" "${runner_args[@]}"
  runner_status=$?
  set -e
  sed -n 'p' "$part" >> "$output"
  if [[ "$runner_status" -ne 0 ]]; then
    printf 'runner returned %s for budget %s; failure rows retained\n' "$runner_status" "$budget" >&2
    overall_status=1
  fi

  "$postgres_prefix/bin/pg_ctl" -D "$cluster_dir" -m immediate stop >/dev/null
  active_cluster=""
done

printf 'wrote %s\n' "$output"
exit "$overall_status"
