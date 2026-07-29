#!/usr/bin/env bash
set -euo pipefail

if [ "${CRAWL_BENCH_NICED:-0}" != 1 ]; then
  exec nice -n 19 env CRAWL_BENCH_NICED=1 bash "$0" "$@"
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TSV2_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$TSV2_DIR/../.." && pwd)"
CORPUS="${CRAWL_BENCH_CORPUS:-$HOME/orgs/grafana}"
DEFAULT_V6_CAP=8
V6_CAP="$DEFAULT_V6_CAP"
BENCH_MODE=""

if [ "${1:-}" = --v5-leg ] || [ "${1:-}" = --v6-leg ]; then
  BENCH_MODE="$1"
  shift
fi

usage() {
  cat <<'EOF'
usage: crawl-bench.sh [--max-repos N]

The v5 leg always crawls the full configured Grafana org. The v6 leg uses the
first N usable repos in sorted path order; N=0 selects every usable repo.
EOF
}

if [ -z "$BENCH_MODE" ]; then
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --max-repos)
        [ "$#" -ge 2 ] || { usage >&2; exit 2; }
        V6_CAP="$2"
        shift 2
        ;;
      --help|-h)
        usage
        exit 0
        ;;
      *)
        usage >&2
        exit 2
        ;;
    esac
  done
fi

case "$V6_CAP" in
  ''|*[!0-9]*) printf 'max repos must be a non-negative integer: %s\n' "$V6_CAP" >&2; exit 2 ;;
esac

if [ -n "${CRAWL_BENCH_WORK:-}" ]; then
  WORK="$CRAWL_BENCH_WORK"
else
  WORK="$(mktemp -d "${TMPDIR:-/tmp}/sprefa-crawl-bench.XXXXXX")"
  trap 'rm -rf "$WORK"' EXIT
fi
mkdir -p "$WORK/v6-dbs" "$WORK/v6-perf"

V5_BIN="${DL_V5_BIN:-$REPO_ROOT/target/release/dl}"
EXTRACT_BIN="${DL_EXTRACT_BIN:-$REPO_ROOT/v6/sprefa-extract/target/release/extract}"
SERVE_MAIN="$TSV2_DIR/serve/main.ts"
V6_PROGRAM="$WORK/v6-crawl.dl6"
V5_PROGRAM="$WORK/v5-crawl.dl"
V5_CONFIG="$WORK/v5-config.toml"

fail() {
  printf 'FAIL  %s\n' "$*" >&2
  exit 1
}

sum_bytes() {
  local total=0 file bytes
  for file in "$@"; do
    [ -f "$file" ] || continue
    bytes="$(wc -c <"$file" | tr -d ' ')"
    total=$((total + bytes))
  done
  printf '%s\n' "$total"
}

ensure_v5_bin() {
  if [ ! -x "$V5_BIN" ]; then
    (cd "$REPO_ROOT" && cargo build --release --bin dl) >"$WORK/build-v5.log" 2>&1 \
      || fail "v5 release build failed: $(tail -5 "$WORK/build-v5.log")"
  fi
  [ -x "$V5_BIN" ] || fail "v5 binary not found: $V5_BIN"
}

ensure_v6_runtime() {
  if [ ! -x "$EXTRACT_BIN" ]; then
    (cd "$REPO_ROOT/v6/sprefa-extract" && cargo build --release --features cli --bin extract) \
      >"$WORK/build-extract.log" 2>&1 \
      || fail "v6 extractor build failed: $(tail -5 "$WORK/build-extract.log")"
  fi
  [ -x "$EXTRACT_BIN" ] || fail "v6 extractor not found: $EXTRACT_BIN"
  if [ ! -d "$TSV2_DIR/node_modules" ]; then
    (cd "$TSV2_DIR" && pnpm install --frozen-lockfile) >"$WORK/pnpm-install.log" 2>&1 \
      || fail "v6 dependency install failed: $(tail -10 "$WORK/pnpm-install.log")"
  fi
  [ -d "$TSV2_DIR/node_modules" ] || fail "v6 node_modules not found: $TSV2_DIR/node_modules"
  local engine_node_modules="$REPO_ROOT/v6/sprefa-store/js/node_modules"
  mkdir -p "$engine_node_modules"
  ln -sfn "$TSV2_DIR/node_modules/rxjs" "$engine_node_modules/rxjs"
  ln -sfn "$TSV2_DIR/node_modules/@libsql" "$engine_node_modules/@libsql"
}

write_programs() {
  cat >"$V5_PROGRAM" <<'EOF'
rel src(path: file, rev: text).
src(p, rev) <- scan(r, "HEAD", "**/*.{go,ts,tsx}", p, rev), repo(r, _, _).
EOF

  cat >"$V5_CONFIG" <<EOF
[[org]]
dir = "$CORPUS"
EOF

  cat >"$V6_PROGRAM" <<'EOF'
sh enumerate(glob: text) -> (path: text, digest: text) =
  `git -C "$DL_CRAWL_REPO" ls-files -- '{glob}' | while IFS= read -r entry; do printf '{"path":"%s","digest":"%s"}\n' "$entry" "$(git -C "$DL_CRAWL_REPO" hash-object -- "$entry")"; done`.

sh extract(path: text) -> (done: text) =
  `"$DL_EXTRACT_BIN" --family cst,type,call,df "$DL_CRAWL_REPO/$path" >/dev/null && printf '%s\n' "$path"`.

rel want(glob: text).
rel file(path: text, digest: text).
file(path, digest) <- want(glob), ? enumerate(glob, path, digest).

rel extracted(path: text).
extracted(path) <- file(path, digest), ? extract(path, done) @ salt(digest: digest).
EOF
}

collect_repos() {
  [ -d "$CORPUS" ] || fail "corpus directory not found: $CORPUS"
  find "$CORPUS" -mindepth 3 -maxdepth 3 -type d -name .git -print \
    | sed 's#/.git$##' | LC_ALL=C sort >"$WORK/all-repos"
  [ -s "$WORK/all-repos" ] || fail "no git repositories found under $CORPUS"

  : >"$WORK/usable-repos"
  : >"$WORK/skips"
  local target_paths
  while IFS= read -r repo; do
    if ! git -C "$repo" rev-parse --verify 'HEAD^{commit}' >/dev/null 2>&1; then
      printf '%s\t%s\n' "$repo" "missing-or-no-head" >>"$WORK/skips"
      continue
    fi
    if ! target_paths="$(git -C "$repo" ls-files -- '**/*.go' '**/*.ts' '**/*.tsx')"; then
      printf '%s\t%s\n' "$repo" "git-file-enumeration" >>"$WORK/skips"
      continue
    fi
    if [ -z "$target_paths" ]; then
      printf '%s\t%s\n' "$repo" "no-matching-source-files" >>"$WORK/skips"
      continue
    fi
    printf '%s\n' "$repo" >>"$WORK/usable-repos"
  done <"$WORK/all-repos"
}

relation_table() {
  local db="$1" rel="$2"
  sqlite3 "$db" "SELECT name FROM sqlite_master WHERE type='table' AND name GLOB 'rel_${rel}*' ORDER BY name LIMIT 1;"
}

relation_count() {
  local db="$1" rel="$2" table
  table="$(relation_table "$db" "$rel")"
  [ -n "$table" ] || fail "relation table not found for $rel in $db"
  sqlite3 "$db" "SELECT count(*) FROM \"$table\";"
}

time_seconds() {
  awk '/ real/{print $1; exit}' "$1"
}

max_rss_bytes() {
  awk '/maximum resident set size/ { for (i = 1; i <= NF; i++) if ($i ~ /^[0-9]+$/) { print $i; exit } }' "$1"
}

replace_rss() {
  local result_file="$1" time_file="$2" rss temp_file
  rss="$(max_rss_bytes "$time_file")"
  [ -n "$rss" ] || rss="n/a"
  temp_file="${result_file}.tmp"
  awk -F '\t' -v OFS='\t' -v rss="$rss" 'BEGIN { $0 = $0 } {$6 = rss; print}' "$result_file" >"$temp_file"
  mv -f "$temp_file" "$result_file"
}

run_v5_leg() {
  local result_file="$1" time_file="$WORK/v5-command-time" log_file="$WORK/v5.log"
  local db="$WORK/v5.sqlite" wall rss files repos db_bytes fps
  rm -f "$db" "$db-wal" "$db-shm"
  set +e
  {
    cd "$REPO_ROOT"
    SPREFA_CONFIG="$V5_CONFIG" DL_NO_DAEMON=1 DL_NO_FETCH=1 DL_STATE_DIR="$WORK/v5-state" \
      /usr/bin/time -l -o "$time_file" "$V5_BIN" "$V5_PROGRAM" --db "$db"
  } >"$log_file" 2>&1
  local time_status=$?
  set -e
  [ -f "$db" ] || fail "v5 did not create its scratch db (time status $time_status)"
  wall="$(time_seconds "$time_file")"
  rss="$(max_rss_bytes "$time_file")"
  [ -n "$rss" ] || rss="n/a"
  files="$(relation_count "$db" src)"
  repos="$(relation_count "$db" repo)"
  db_bytes="$(sum_bytes "$db" "$db-wal" "$db-shm")"
  fps="$(awk -v f="$files" -v w="$wall" 'BEGIN { if (w > 0) printf "%.2f", f / w; else print "n/a" }')"
  printf 'v5\t%s\t%s\t%s\t%s\t%s\t%s\tfull-org-389\tn/a\n' \
    "$files" "$repos" "$wall" "$fps" "$rss" "$db_bytes" >"$result_file"
  printf 'v5 completed: files=%s repos=%s wall=%ss rss=%s bytes db=%s bytes\n' \
    "$files" "$repos" "$wall" "$rss" "$db_bytes"
}

json_rows() {
  curl -fsS "$1" | python3 -c 'import json, sys; print(len(json.load(sys.stdin)["rows"]))'
}

wait_for_rows() {
  local base="$1" expected="$2" deadline=$((SECONDS + 600)) file_rows extracted_rows
  while [ "$SECONDS" -lt "$deadline" ]; do
    file_rows="$(json_rows "$base/idb/file")"
    extracted_rows="$(json_rows "$base/idb/extracted")"
    if [ "$file_rows" -eq "$expected" ] && [ "$extracted_rows" -eq "$expected" ]; then
      printf '%s\t%s\n' "$file_rows" "$extracted_rows"
      return 0
    fi
    sleep 0.25
  done
  printf 'v6 did not settle: expected=%s file=%s extracted=%s\n' \
    "$expected" "$file_rows" "$extracted_rows" >&2
  return 1
}

start_server() {
  local repo="$1" db="$2" port="$3" perf="$4" log="$5"
  (
    cd "$TSV2_DIR"
    DL_NO_FETCH=1 DL_PERF_LOG="$perf" DL_EXTRACT_BIN="$EXTRACT_BIN" DL_CRAWL_REPO="$repo" \
      TSV2_DB="file:$db" TSV2_PORT="$port" TSV2_WATCH_ROOT="$repo" TSV2_WATCH_COALESCE_MS=50 \
      node --experimental-transform-types "$SERVE_MAIN"
  ) >"$log" 2>&1 &
  SERVER_PID=$!
  for _ in $(seq 1 100); do
    if curl -s -o /dev/null "http://127.0.0.1:$port/ticks"; then return 0; fi
    if ! kill -0 "$SERVER_PID" 2>/dev/null; then
      tail -20 "$log" >&2
      return 1
    fi
    sleep 0.1
  done
  return 1
}

stop_server() {
  if [ -n "${SERVER_PID:-}" ]; then
    kill -9 "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
    SERVER_PID=""
  fi
}

run_v6_leg() {
  local result_file="$1" cap="$2" log_file="$WORK/v6.log"
  local selected=0 skipped=0 cap_excluded=0 total_files=0 total_repos=0 total_statements=0 total_ticks=0
  local repo expected repo_name index=0 port db perf server_log base status
  local started_at ended_at
  started_at="$(date +%s.%N)"
  : >"$log_file"
  : >"$WORK/v6-skips"
  while IFS= read -r repo; do
    if [ "$cap" -ne 0 ] && [ "$selected" -ge "$cap" ]; then
      cap_excluded=$((cap_excluded + 1))
      continue
    fi
    selected=$((selected + 1))
    repo_name="$(basename "$repo")"
    expected="$(git -C "$repo" ls-files -- '**/*.go' '**/*.ts' '**/*.tsx' | LC_ALL=C sort -u | wc -l | tr -d ' ')"
    index=$((index + 1))
    port=$((18000 + index))
    db="$WORK/v6-dbs/$index.sqlite"
    perf="$WORK/v6-perf/$index.jsonl"
    server_log="$WORK/v6-perf/$index.server.log"
    rm -f "$db" "$db-wal" "$db-shm" "$perf" "$server_log"
    if ! start_server "$repo" "$db" "$port" "$perf" "$server_log"; then
      stop_server
      fail "v6 server failed for $repo"
    fi
    base="http://127.0.0.1:$port"
    status="$(curl -fsS -o "$WORK/load-$index.json" -w '%{http_code}' \
      -X POST --data-binary @"$V6_PROGRAM" "$base/program")"
    [ "$status" = 200 ] || { tail -20 "$server_log" >&2; stop_server; fail "v6 program load failed for $repo"; }
    curl -fsS -X POST -H 'content-type: application/json' --data-binary \
      '{"batch":[{"rel":"want","sign":"add","row":["**/*.go"]},{"rel":"want","sign":"add","row":["**/*.ts"]},{"rel":"want","sign":"add","row":["**/*.tsx"]}]}' \
      "$base/arrivals" >/dev/null
    wait_for_rows "$base" "$expected" >>"$log_file"
    total_files=$((total_files + expected))
    total_repos=$((total_repos + 1))
    if [ -s "$perf" ]; then
      while IFS=$'\t' read -r statements ticks; do
        total_statements=$((total_statements + statements))
        total_ticks=$((total_ticks + ticks))
      done < <(jq -r 'select(.statements != null) | [.statements, 1] | @tsv' "$perf")
    fi
    printf 'v6 repo %s: files=%s\n' "$repo_name" "$expected" >>"$log_file"
    stop_server
  done <"$WORK/usable-repos"

  skipped="$(wc -l <"$WORK/skips" | tr -d ' ')"
  cap_excluded="${cap_excluded:-0}"
  local wall rss db_bytes fps stmts_tick scope
  ended_at="$(date +%s.%N)"
  wall="$(awk -v start="$started_at" -v end="$ended_at" 'BEGIN { printf "%.2f", end - start }')"
  rss="n/a"
  db_bytes="$(find "$WORK/v6-dbs" -type f -print0 \
    | while IFS= read -r -d '' file; do wc -c <"$file"; done \
    | awk '{s += $1} END {print s + 0}')"
  fps="$(awk -v f="$total_files" -v w="$wall" 'BEGIN { if (w > 0) printf "%.2f", f / w; else print "n/a" }')"
  if [ "$total_ticks" -gt 0 ]; then
    stmts_tick="$(awk -v s="$total_statements" -v t="$total_ticks" 'BEGIN { printf "%.2f", s / t }')"
  else
    stmts_tick="n/a"
  fi
  if [ "$cap" -eq 0 ]; then scope="full-org-$total_repos"; else scope="first-$total_repos-usable-of-389-cap-$cap"; fi
  printf 'v6\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$total_files" "$total_repos" "$wall" "$fps" "$rss" "$db_bytes" "$scope" "$stmts_tick" >"$result_file"
  printf 'v6 completed: files=%s repos=%s wall=%ss rss=%s bytes db=%s bytes stmts/tick=%s\n' \
    "$total_files" "$total_repos" "$wall" "$rss" "$db_bytes" "$stmts_tick"
  printf 'v6 skips: unusable=%s cap-excluded=%s\n' "$skipped" "$cap_excluded"
  printf '%s\n' "$skipped" >"$WORK/v6-skip-count"
  printf '%s\n' "$cap_excluded" >"$WORK/v6-cap-excluded"
}

main() {
  local bench_started bench_ended bench_wall v5_status v6_status
  bench_started="$(date +%s.%N)"
  ensure_v5_bin
  ensure_v6_runtime
  write_programs
  collect_repos
  local repo_count usable_count skip_count
  repo_count="$(wc -l <"$WORK/all-repos" | tr -d ' ')"
  usable_count="$(wc -l <"$WORK/usable-repos" | tr -d ' ')"
  skip_count="$(wc -l <"$WORK/skips" | tr -d ' ')"
  [ "$repo_count" -eq 389 ] || printf 'corpus repo count: %s (historical scope: 389)\n' "$repo_count"
  printf 'corpus repos: total=%s usable=%s skipped=%s v6-cap=%s\n' \
    "$repo_count" "$usable_count" "$skip_count" "$V6_CAP"

  set +e
  CRAWL_BENCH_WORK="$WORK" /usr/bin/time -l -o "$WORK/v5-time" bash "$0" --v5-leg "$WORK/v5-result" >"$WORK/v5-run.log" 2>&1
  v5_status=$?
  set -e
  if [ ! -f "$WORK/v5-result" ]; then
    cat "$WORK/v5-run.log" >&2
    cat "$WORK/v5.log" >&2 2>/dev/null || true
    exit "$v5_status"
  fi
  cat "$WORK/v5-run.log"
  set +e
  CRAWL_BENCH_WORK="$WORK" /usr/bin/time -l -o "$WORK/v6-time" bash "$0" --v6-leg "$WORK/v6-result" "$V6_CAP" >"$WORK/v6-run.log" 2>&1
  v6_status=$?
  set -e
  if [ ! -f "$WORK/v6-result" ]; then
    cat "$WORK/v6-run.log" >&2
    cat "$WORK/v6.log" >&2 2>/dev/null || true
    exit "$v6_status"
  fi
  replace_rss "$WORK/v6-result" "$WORK/v6-time"
  cat "$WORK/v6-run.log"
  cat "$WORK/v5-result" "$WORK/v6-result"
  printf 'skip counts: v5/v6 unusable=%s, v6 cap-excluded=%s\n' \
    "$skip_count" "$(cat "$WORK/v6-cap-excluded")"
  bench_ended="$(date +%s.%N)"
  bench_wall="$(awk -v start="$bench_started" -v end="$bench_ended" 'BEGIN { printf "%.2f", end - start }')"
  printf 'bench total wall: %ss\n' "$bench_wall"
  printf 'bench artifacts retained until exit: %s\n' "$WORK"
}

if [ "$BENCH_MODE" = --v5-leg ]; then
  run_v5_leg "$1"
  exit 0
fi
if [ "$BENCH_MODE" = --v6-leg ]; then
  run_v6_leg "$1" "$2"
  exit 0
fi

main
