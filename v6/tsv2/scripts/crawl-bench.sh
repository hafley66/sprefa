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

  # ONE PROGRAM FOR THE WHOLE CORPUS.
  #
  # Until 2026-07-31 this program had no repo column at all: it read the
  # repository root out of `$DL_CRAWL_REPO`, which meant one served process,
  # one sqlite database and one program load PER REPOSITORY, driven by a shell
  # loop in run_v6_leg. Ruling repo_column_spelling = distinct_name_hosts made
  # the root an ordinary demand column on distinct-named hosts, so the loop is
  # gone: the repository set arrives as `want_repo` rows in ONE /arrivals post
  # and every fan-out below it is rows, not processes.
  #
  # Same hosts as v6/dl/fixtures/crawl_org.dl6, minus the `repos` host: the
  # bench needs `--max-repos` to select its corpus slice, so the repository set
  # is posted rather than discovered. crawl_org.dl6 is where the discovery leg
  # (repos on an interval bind) is graded.
  #
  # `repo_extract` mentions `{repo}/{path}`, which is what selects the
  # sprefa_extract_repo executor (registry.pl host_execution) and keeps the
  # applicative fold. The old `$DL_CRAWL_REPO/$path` spelling used shell
  # variable references, so it fell to the generic shell executor and folded
  # nothing.
  #
  # `>/dev/null && printf` IS DELIBERATE AND IS THE OLD PROGRAM'S SHAPE: this
  # host answers ONE row per extracted file, not the extractor's whole JSONL.
  # Measured, because the first draft did the latter: capturing every cst/type/
  # call/df record as an EDB arrival took the same 779-file corpus from 20.26s
  # to 62.97s and the database from 1.0MB to 595MB. That is a real and
  # interesting number about the extraction seam, and it is NOT this bench's
  # question -- the before/after here isolates the REPOSITORY LOOP, so the
  # extraction leg has to stay byte-for-byte the work it was.
  cat >"$V6_PROGRAM" <<'EOF'
sh repo_files(repo: text, glob: text) -> (path: text, digest: text) =
  `git -C '{repo}' ls-files -- '{glob}' | while IFS= read -r entry; do printf '%s %s\n' "$entry" "$(git -C '{repo}' hash-object -- "$entry")"; done`.

sh repo_extract(repo: text, path: text, digest: text) -> (done: text) =
  `"$DL_EXTRACT_BIN" --family cst,type,call,df {repo}/{path} >/dev/null && printf '%s\n' '{path}'`.

rel want_repo(repo: text, glob: text).
rel repo_file(repo: text, path: text, digest: text).
repo_file(repo, path, digest) <- want_repo(repo, glob), repo_files(repo, glob, path, digest).

rel extracted(repo: text, path: text).
extracted(repo, path) <- repo_file(repo, path, digest), repo_extract(repo, path, digest, done).
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

# Settle on repo_file reaching the corpus's own file count, then on `extracted`
# holding still. The two are separate conditions because they are separate
# claims: repo_file == expected says the ENUMERATION fan-out reached every
# repository, and a quiet `extracted` says the EXTRACTION fan-out drained. They
# are not equal counts -- `extracted` is a projection of a multi-row extractor
# answer, and a file the extractor reports nothing for contributes no row --
# so requiring equality (as the per-repository version did, with its
# `&& printf` guaranteeing one line per file) would hang on the first such file.
wait_for_rows() {
  local base="$1" expected="$2" deadline=$((SECONDS + 1800))
  local file_rows=0 extracted_rows=0 previous=-1 quiet=0
  while [ "$SECONDS" -lt "$deadline" ]; do
    file_rows="$(json_rows "$base/idb/repo_file")"
    extracted_rows="$(json_rows "$base/idb/extracted")"
    if [ "$file_rows" -eq "$expected" ]; then
      if [ "$extracted_rows" -eq "$previous" ]; then
        quiet=$((quiet + 1))
        if [ "$quiet" -ge 3 ]; then
          printf '%s\t%s\n' "$file_rows" "$extracted_rows"
          return 0
        fi
      else
        quiet=0
      fi
      previous="$extracted_rows"
    fi
    sleep 0.5
  done
  printf 'v6 did not settle: expected=%s repo_file=%s extracted=%s\n' \
    "$expected" "$file_rows" "$extracted_rows" >&2
  return 1
}

start_server() {
  local db="$1" port="$2" perf="$3" log="$4"
  (
    cd "$TSV2_DIR"
    DL_NO_FETCH=1 DL_PERF_LOG="$perf" DL_EXTRACT_BIN="$EXTRACT_BIN" \
      TSV2_DB="file:$db" TSV2_PORT="$port" \
      node --experimental-transform-types "$SERVE_MAIN"
  ) >"$log" 2>&1 &
  SERVER_PID=$!
  for _ in $(seq 1 200); do
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

# THE LEG THAT USED TO BE A LOOP.
#
# One server, one sqlite database, one program load, one /arrivals post. The
# repository set is DATA -- one `want_repo(root, glob)` row per repository per
# glob -- and every fan-out below it (per repository, then per file) is rows
# through the incremental emitter, not processes through bash.
#
# What died with the loop, and it is not only the process spawns: N program
# compilations, N sqlite files, N boot demand scans, N witness caches that
# could not share an answer, and a per-repository settle barrier that made the
# whole corpus as slow as the sum of its serial parts. The remaining serial
# step is `collect_repos`, which is the bench's own corpus SELECTION and not
# part of what is being measured.
run_v6_leg() {
  local result_file="$1" cap="$2" log_file="$WORK/v6.log"
  local selected=0 skipped=0 cap_excluded=0 total_files=0 total_repos=0 total_statements=0 total_ticks=0
  local repo db perf server_log base status port settled
  local started_at ended_at
  started_at="$(date +%s.%N)"
  : >"$log_file"
  : >"$WORK/v6-selected"
  # Idempotent, and it is what makes `crawl-bench.sh --v6-leg <result> <cap>`
  # runnable on its own against a prepared WORK directory -- the shape used to
  # measure this leg before and after the loop was removed.
  write_programs

  while IFS= read -r repo; do
    if [ "$cap" -ne 0 ] && [ "$selected" -ge "$cap" ]; then
      cap_excluded=$((cap_excluded + 1))
      continue
    fi
    selected=$((selected + 1))
    printf '%s\n' "$repo" >>"$WORK/v6-selected"
  done <"$WORK/usable-repos"
  total_repos="$selected"

  # The expected file count is the union across the whole selected corpus, so
  # a repository whose rows never arrived shows up as a settle failure rather
  # than as a quietly smaller number.
  total_files=0
  while IFS= read -r repo; do
    local repo_files
    repo_files="$(git -C "$repo" ls-files -- '**/*.go' '**/*.ts' '**/*.tsx' | LC_ALL=C sort -u | wc -l | tr -d ' ')"
    total_files=$((total_files + repo_files))
    printf 'v6 repo %s: files=%s\n' "$(basename "$repo")" "$repo_files" >>"$log_file"
  done <"$WORK/v6-selected"

  port=18001
  db="$WORK/v6-dbs/crawl.sqlite"
  perf="$WORK/v6-perf/crawl.jsonl"
  server_log="$WORK/v6-perf/crawl.server.log"
  rm -f "$db" "$db-wal" "$db-shm" "$perf" "$server_log"
  if ! start_server "$db" "$port" "$perf" "$server_log"; then
    stop_server
    fail "v6 server failed to boot"
  fi
  base="http://127.0.0.1:$port"
  status="$(curl -fsS -o "$WORK/load.json" -w '%{http_code}' \
    -X POST --data-binary @"$V6_PROGRAM" "$base/program")"
  [ "$status" = 200 ] || { tail -20 "$server_log" >&2; stop_server; fail "v6 program load failed"; }

  # One batch, every repository, every glob. This single post is what the loop
  # was.
  python3 - "$WORK/v6-selected" >"$WORK/v6-arrivals.json" <<'PY'
import json, sys
roots = [line.strip() for line in open(sys.argv[1]) if line.strip()]
globs = ["**/*.go", "**/*.ts", "**/*.tsx"]
batch = [{"rel": "want_repo", "sign": "add", "row": [root, glob]}
         for root in roots for glob in globs]
json.dump({"batch": batch}, sys.stdout)
PY
  curl -fsS -X POST -H 'content-type: application/json' \
    --data-binary @"$WORK/v6-arrivals.json" "$base/arrivals" >/dev/null

  if ! settled="$(wait_for_rows "$base" "$total_files")"; then
    tail -20 "$server_log" >&2
    stop_server
    fail "v6 crawl did not settle"
  fi
  printf 'v6 settled: repo_file=%s extracted=%s\n' \
    "$(printf '%s' "$settled" | cut -f1)" "$(printf '%s' "$settled" | cut -f2)" >>"$log_file"

  if [ -s "$perf" ]; then
    while IFS=$'\t' read -r statements ticks; do
      total_statements=$((total_statements + statements))
      total_ticks=$((total_ticks + ticks))
    done < <(jq -r 'select(.statements != null) | [.statements, 1] | @tsv' "$perf")
  fi
  stop_server

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

# BEFORE / AFTER for the change that removed the per-repository loop.
#
# The "before" is a real run of the previous v6 leg, recorded in
# scripts/crawl-bench-loop-baseline.tsv with the command that produced it. It
# is a pinned MEASUREMENT, not a target: the loop it came from no longer exists
# in this file, so it cannot be re-derived by running this script, and printing
# it beside today's number is the honest way to keep the comparison alive.
#
# The comparison is only meaningful at the SAME scope, so it is skipped, by
# name, whenever the cap or corpus differs from the baseline's.
report_loop_delta() {
  local result_file="$1" baseline="$SCRIPT_DIR/crawl-bench-loop-baseline.tsv"
  [ -f "$baseline" ] || return 0
  local now_scope now_wall now_fps now_db now_files
  IFS=$'\t' read -r _ now_files _ now_wall now_fps _ now_db now_scope _ <"$result_file"
  local base_row
  base_row="$(awk -F '\t' -v scope="$now_scope" \
    '$1 == "v6-loop" && $8 == scope { print; exit }' "$baseline")"
  if [ -z "$base_row" ]; then
    printf 'loop delta: N/A -- no v6-loop baseline row for scope %s (baseline scopes: %s)\n' \
      "$now_scope" \
      "$(awk -F '\t' '$1 == "v6-loop" { printf "%s ", $8 }' "$baseline")"
    return 0
  fi
  local base_wall base_fps base_db
  IFS=$'\t' read -r _ _ _ base_wall base_fps _ base_db _ _ <<<"$base_row"
  printf 'loop delta (%s, %s files):\n' "$now_scope" "$now_files"
  printf '  before (one served process per repository)  wall=%ss  %s files/s  db=%s bytes\n' \
    "$base_wall" "$base_fps" "$base_db"
  printf '  after  (ONE program, repo as a column)      wall=%ss  %s files/s  db=%s bytes\n' \
    "$now_wall" "$now_fps" "$now_db"
  awk -v b="$base_wall" -v a="$now_wall" \
    'BEGIN { if (a > 0) printf "  speedup %.2fx\n", b / a }'
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
  report_loop_delta "$WORK/v6-result"
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
