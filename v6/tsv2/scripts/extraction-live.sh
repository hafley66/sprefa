#!/usr/bin/env bash
# extraction-live.sh — THE GOLDEN PLAN'S PHASE 2 EXIT RECEIPT
# (plans/2026-07-29-v6-alpha-golden-plan.md: "an sg-rail-class diag rail runs
# end to end on v6 with a real file edit triggering the retick").
#
# One served tsv2 process, one .dl6 rail (../dl/fixtures/extraction-live.dl6),
# one scratch corpus, and REAL `vim`-shaped edits to real files. Nothing is
# faked: the watcher is node's own fs.watch behind the bind seam, the extractor
# is the in-tree RELEASE build of v6/sprefa-extract, and every row below came
# out of SQLite through the program's own emitted decode SELECT.
#
# Nine phases, each an assertion. 1-5 are the live loop, 6-7 are restart
# reconciliation, and 8-9 apply the endurance law to a real in-flight
# extraction:
#
#   1  write src/a.ts with an eval() call
#                                      -> banned_call gains (src/a.ts, eval)
#   2  ATOMIC SAVE of src/a.ts (write temp, rename over) with the eval gone
#                                       -> banned_call LOSES the row
#      (this is the editor case: the write-temp-then-rename that chokidar needs
#       an `atomic: true` option to reconstruct is, at this seam, just "the
#       real path's digest changed")
#   3  touch src/a.ts with IDENTICAL bytes
#                                      -> ZERO new ticks (content-addressed:
#      the digest row is unchanged, so the boundary delta is empty and neither
#      the engine nor the extractor does any work)
#   4  write src/b.ts with an eval() call
#                                      -> banned_call gains (src/b.ts, eval)
#   5  rm src/b.ts                      -> banned_call LOSES the row (the `-`
#      arrival's refCount retraction runs through the real emitted SQL)
#   6  restart on the same file db       -> ZERO re-extractions. Demand rows are
#      durable and deltas are not, so boot replays every demand row;
#      `__host_witness` is what makes an ANSWERED one a no-op. Exactly-once.
#   7  delete src/d.ts while server is DOWN
#                                      -> boot retracts the durable watch row
#      and its downstream banned_call finding
#   8  kill -9 MID-EXTRACTION of src/c.ts
#                                      -> the demand row is durable, its answer
#      is not (the shim sleeps, so the window is real, not simulated)
#   9  restart                           -> the unanswered witness re-runs ONCE
#      and its finding lands. At-least-once for the unanswered half; the killed
#      spawn already counted, which is the honest boundary goal-endurance states.
#
# SABOTAGE RECEIPT (run 2026-07-29, reverted): dropping the `del` half of
# GlobWatch.batchFor (serve/2_binds.ts) -- emitting only `add` rows -- passes
# phases 1 and 4 and FAILS phase 2 with "phase 2: banned_call still holds
# src/a.ts/eval after the eval was edited out", because the stale digest's row keeps
# its extraction alive. A second sabotage, returning `previous` unchanged from
# `digestOf` (so an edit looks like no change), fails phase 1 the same way.
#
# EXTRACT BINARY: resolved here, never in program text. The order is
# DL_EXTRACT_BIN, then the in-tree RELEASE build, then a build. v6/dl's
# 4_ingest.ts still defaults to a DEBUG build under an absolute path from
# another worktree; that is the known perf item this script does not copy.
# BUDGET (timeout-gun lane, 2026-07-31). Measured wall: 39s. Default 900s is
# ~23x that, with the extra headroom because this rail may build the release
# extractor on a cold tree (cargo, minutes) before any of its nine phases run.
# Whole-script cap: the cost is a node server, one extractor subprocess per
# demanded file digest, and a kill -9 phase that deliberately leaves a
# half-finished process behind. Every curl also carries
# EXTRACTION_HTTP_BUDGET_S. Override with EXTRACTION_LIVE_BUDGET_S.
set -uo pipefail
TSV2="$(cd "$(dirname "$0")/.." && pwd)"

. "$TSV2/../tools/run-capped.sh"
cap_self "${EXTRACTION_LIVE_BUDGET_S:-900}" extraction_live "$@"

PORT="${TSV2_EXTRACTION_PORT:-17571}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/tsv2-extract.XXXXXX")"
CORPUS="$WORK/corpus"
DB="file:$WORK/extract.sqlite"
PROGRAM="$TSV2/../dl/fixtures/extraction-live.dl6"
SERVE_MAIN="$TSV2/serve/main.ts"
BASE="http://127.0.0.1:$PORT"
SERVER_PID=""
mkdir -p "$CORPUS"
cd "$CORPUS"
git init -q
mkdir -p "$CORPUS/src"

fail() { printf 'FAIL  %s\n' "$*"; [ -n "$SERVER_PID" ] && tail -20 "$WORK/server.log"; stop_server; exit 1; }
say() { printf '%s\n' "$*"; }

stop_server() {
  [ -n "$SERVER_PID" ] && kill -9 "$SERVER_PID" 2>/dev/null
  wait "$SERVER_PID" 2>/dev/null
  SERVER_PID=""
}
trap stop_server EXIT

# ── the extractor: in-tree release build, resolved not hardcoded ─────────────
resolve_extract_bin() {
  if [ -n "${DL_EXTRACT_BIN:-}" ] && [ -x "$DL_EXTRACT_BIN" ]; then
    say "extract bin: $DL_EXTRACT_BIN (DL_EXTRACT_BIN)"
    return
  fi
  local crate release
  crate="$(cd "$TSV2/../sprefa-extract" && pwd)"
  release="$crate/target/release/extract"
  if [ ! -x "$release" ]; then
    say "building the in-tree release extractor (cargo build --release --features cli)"
    (cd "$crate" && cargo build --release --features cli --bin extract) >"$WORK/cargo.log" 2>&1 \
      || fail "cargo build failed: $(tail -5 "$WORK/cargo.log")"
  fi
  [ -x "$release" ] || fail "no extract binary at $release"
  export DL_EXTRACT_BIN="$release"
  say "extract bin: $DL_EXTRACT_BIN (in-tree release)"
}

# A SHIM AROUND THE REAL BINARY, so the endurance phases can count invocations
# and make one of them slow WITHOUT putting test scaffolding in the rail
# program. The .dl6 says `"$DL_EXTRACT_BIN" --family call {path}` and nothing
# else; which binary that names is the environment's business, which is the
# whole point of resolving it out here.
install_extract_shim() {
  REAL_EXTRACT_BIN="$DL_EXTRACT_BIN"
  MARKS="$WORK/extract-marks"
  : >"$MARKS"
  cat >"$WORK/extract-shim" <<SHIM
#!/bin/sh
printf 'x\n' >>"$MARKS"
[ "\${TSV2_EXTRACT_SLOW:-0}" = "0" ] || sleep "\$TSV2_EXTRACT_SLOW"
exec "$REAL_EXTRACT_BIN" "\$@"
SHIM
  chmod +x "$WORK/extract-shim"
  export DL_EXTRACT_BIN="$WORK/extract-shim"
}

marks_count() { grep -c . "$MARKS" 2>/dev/null || echo 0; }

# THE SERVED PROCESS'S CWD IS THE ROOT, the same convention v6/dl states as
# `DL_ROOT = process.cwd()`. Watch globs resolve against it, emitted paths are
# relative to it, and an `sh` host's child inherits it -- which is what lets the
# rail's template say `{path}` and mean the file the watcher just named. Every
# path the server itself needs (compile.pl, gen_served/, main.ts) is already
# absolute, so moving the cwd moves only the corpus.
start_server() {
  TSV2_DB="$DB" TSV2_PORT="$PORT" TSV2_WATCH_COALESCE_MS=60 \
    DL_EXTRACT_BIN="$DL_EXTRACT_BIN" TSV2_EXTRACT_SLOW="${1:-0}" \
    node --experimental-transform-types "$SERVE_MAIN" >>"$WORK/server.log" 2>&1 &
  SERVER_PID=$!
  for _ in $(seq 1 60); do
    capped_curl "${EXTRACTION_HTTP_BUDGET_S:-30}" -s -o /dev/null "$BASE/ticks" 2>/dev/null && return
    kill -0 "$SERVER_PID" 2>/dev/null || fail "server died on boot: $(tail -5 "$WORK/server.log")"
    sleep 0.2
  done
  fail "server did not become ready on port $PORT"
}

# rows of one rel, as one line of compact JSON
rows_of() { capped_curl "${EXTRACTION_HTTP_BUDGET_S:-30}" -s "$BASE/idb/$1" | tr -d ' \n'; }
tick_count() { grep -c '^{"tick"' "$WORK/server.log" 2>/dev/null || echo 0; }

# poll until `rows_of $1` contains (or, with `absent`, stops containing) $2
await_rows() {
  local rel="$1" needle="$2" mode="${3:-present}" deadline=$((SECONDS + 30))
  while [ "$SECONDS" -lt "$deadline" ]; do
    local rows; rows="$(rows_of "$rel")"
    case "$mode" in
      present) case "$rows" in *"$needle"*) return 0;; esac ;;
      absent)  case "$rows" in *"$needle"*) ;; *) return 0;; esac ;;
    esac
    sleep 0.2
  done
  return 1
}

resolve_extract_bin
install_extract_shim
start_server

status="$(capped_curl "${EXTRACTION_LOAD_BUDGET_S:-900}" -s -o "$WORK/load.json" -w '%{http_code}' -X POST --data-binary @"$PROGRAM" "$BASE/program")"
[ "$status" = "200" ] || fail "program load returned $status: $(cat "$WORK/load.json")"
grep -q '"watch"' "$WORK/load.json" || fail "loaded program declares no watch bind: $(cat "$WORK/load.json")"
say "PASS  program loaded, binds: $(cat "$WORK/load.json" | tr -d ' \n' | sed 's/.*"binds"://; s/,"hosts".*//')"

# ── phase 1: a new file with a banned call ──────────────────────────────────
cat >"$CORPUS/src/a.ts" <<'EOF'
export function danger(source: string): unknown {
  return eval(source);
}
EOF
await_rows banned_call '"src/a.ts","eval"' present \
  || fail "phase 1: banned_call never gained src/a.ts/eval (rows: $(rows_of banned_call))"
git add -- src/a.ts
say "PASS  phase 1  real file write -> extraction -> finding: $(rows_of banned_call)"

# ── phase 2: the editor atomic save (write temp, rename over) ───────────────
cat >"$CORPUS/src/.a.ts.swap" <<'EOF'
export function danger(source: string): unknown {
  return JSON.parse(source);
}
EOF
mv -f "$CORPUS/src/.a.ts.swap" "$CORPUS/src/a.ts"
await_rows banned_call '"src/a.ts","eval"' absent \
  || fail "phase 2: banned_call still holds src/a.ts/eval after the eval was edited out (rows: $(rows_of banned_call))"
say "PASS  phase 2  atomic save (write temp + rename) -> retick -> finding retracted"

# ── phase 3: identical bytes are not a change ───────────────────────────────
before="$(tick_count)"
touch "$CORPUS/src/a.ts"
cp -f "$CORPUS/src/a.ts" "$WORK/same.ts" && cp -f "$WORK/same.ts" "$CORPUS/src/a.ts"
sleep 1
after="$(tick_count)"
[ "$before" = "$after" ] \
  || fail "phase 3: an identical-bytes rewrite caused $((after - before)) tick(s); the digest row should be unchanged"
say "PASS  phase 3  identical-bytes rewrite -> 0 ticks (content-addressed, ticks still $after)"

# ── phase 4: a second file ──────────────────────────────────────────────────
cat >"$CORPUS/src/b.ts" <<'EOF'
export const run = (code: string): unknown => eval(code);
EOF
await_rows banned_call '"src/b.ts","eval"' present \
  || fail "phase 4: banned_call never gained src/b.ts/eval (rows: $(rows_of banned_call))"
git add -- src/b.ts
say "PASS  phase 4  second file -> finding: $(rows_of banned_call)"

# ── phase 5: deletion retracts through the real emitted SQL ─────────────────
rm -f "$CORPUS/src/b.ts"
await_rows banned_call '"src/b.ts","eval"' absent \
  || fail "phase 5: banned_call still holds src/b.ts/eval after the file was deleted (rows: $(rows_of banned_call))"
say "PASS  phase 5  file deleted -> '-' arrival -> finding retracted: $(rows_of banned_call)"

# The extractor really ran. After phase 5 the only file left is src/a.ts in its
# edited (eval-free) form, so the callee the real extractor found there is
# `parse` -- asserted by name, because "call_site is nonempty" alone would also
# pass on stale rows the retraction should have taken.
sites="$(rows_of call_site)"
case "$sites" in *'"src/a.ts"'*'"parse"'*) ;; *) fail "call_site does not carry src/a.ts/parse: $sites";; esac
case "$sites" in *eval*) fail "call_site still carries an eval row after both retractions: $sites";; esac
# NO EMPTY CALLEE. The extractor's JSONL interleaves `record=node` lines that
# carry no `callee` at all; if the named projection stopped filtering them, each
# would land as a row with an empty callee column rather than as no row. This is
# the receipt for that (tests/hostDecode.test.ts covers the same seam directly).
case "$sites" in *'""'*) fail "call_site carries an empty-callee row: the JSONL projection let a non-site record through: $sites";; esac
say "PASS  call_site carries exactly the surviving file's real extractor output, no empty projections"

# ── phase 6: an ANSWERED extraction does not re-run across a restart ────────
# The demand rows are durable SQLite rows and the deltas are not, so boot
# replays every live demand row; `__host_witness` is what turns the already
# answered ones into no-ops. Marks count SPAWNS, so "flat across a restart" is
# exactly the exactly-once half of the endurance law.
before_restart="$(marks_count)"
before_restart_ticks="$(tick_count)"
[ "$before_restart" -ge 3 ] || fail "expected at least 3 extractions before the restart, counted $before_restart"
stop_server
start_server 0
status="$(capped_curl "${EXTRACTION_LOAD_BUDGET_S:-900}" -s -o "$WORK/load2.json" -w '%{http_code}' -X POST --data-binary @"$PROGRAM" "$BASE/program")"
[ "$status" = "200" ] || fail "phase 6: program reload returned $status: $(cat "$WORK/load2.json")"
sleep 3
after_restart="$(marks_count)"
after_restart_ticks="$(tick_count)"
[ "$after_restart_ticks" = "$before_restart_ticks" ] \
  || fail "phase 6: zero-change restart caused $((after_restart_ticks - before_restart_ticks)) tick(s)"
[ "$after_restart" = "$before_restart" ] \
  || fail "phase 6: boot replay re-ran $((after_restart - before_restart)) answered extraction(s); answered is exactly-once"
say "PASS  phase 6  zero-change restart -> 0 ticks, 0 re-extractions (marks $after_restart)"

# ── phase 7: deletion while DOWN retracts on the next boot ──────────────────
# FAIL-FIRST 2026-07-29, before boot reconcile:
#   FAIL  phase 7: watch row survived deletion while server was down
#   (rows: {"rows":[["**/*.ts","src/d.ts","..."]]})
cat >"$CORPUS/src/d.ts" <<'EOF'
export const offline = (code: string): unknown => eval(code);
EOF
git add -- src/d.ts
await_rows banned_call '"src/d.ts","eval"' present \
  || fail "phase 7: banned_call never gained src/d.ts/eval before shutdown (rows: $(rows_of banned_call))"
stop_server
rm -f "$CORPUS/src/d.ts"
start_server 0
status="$(capped_curl "${EXTRACTION_LOAD_BUDGET_S:-900}" -s -o "$WORK/load3.json" -w '%{http_code}' -X POST --data-binary @"$PROGRAM" "$BASE/program")"
[ "$status" = "200" ] || fail "phase 7: program reload returned $status: $(cat "$WORK/load3.json")"
await_rows watch '"src/d.ts"' absent \
  || fail "phase 7: watch row survived deletion while server was down (rows: $(rows_of watch))"
await_rows banned_call '"src/d.ts","eval"' absent \
  || fail "phase 7: downstream finding survived deletion while server was down (rows: $(rows_of banned_call))"
say "PASS  phase 7  delete while down -> boot retracted watch row and downstream finding"

# ── phase 8: kill -9 MID-EXTRACTION; the unanswered witness finishes later ──
# The shim sleeps, so there is a real window in which the demand row is durable
# and its answer is not. Killing there is the endurance law's own scenario:
# at-least-once for the unanswered witness (its killed spawn already counted),
# and the finding must still land after the restart.
stop_server
start_server 6
status="$(capped_curl "${EXTRACTION_LOAD_BUDGET_S:-900}" -s -o "$WORK/load4.json" -w '%{http_code}' -X POST --data-binary @"$PROGRAM" "$BASE/program")"
[ "$status" = "200" ] || fail "phase 8: program reload returned $status: $(cat "$WORK/load4.json")"
before_kill="$(marks_count)"
cat >"$CORPUS/src/c.ts" <<'EOF'
export const late = (code: string): unknown => eval(code);
EOF
git add -- src/c.ts
deadline=$((SECONDS + 30))
while [ "$(marks_count)" = "$before_kill" ]; do
  [ "$SECONDS" -ge "$deadline" ] && fail "phase 8: the slow extraction never started (marks $(marks_count))"
  sleep 0.3
done
mid_kill="$(marks_count)"
kill -9 "$SERVER_PID" 2>/dev/null
wait "$SERVER_PID" 2>/dev/null
SERVER_PID=""
say "PASS  phase 8  killed -9 mid-extraction with src/c.ts demanded and unanswered (marks $mid_kill)"

# ── phase 9: the crashed extraction re-runs and its finding lands ───────────
start_server 0
status="$(capped_curl "${EXTRACTION_LOAD_BUDGET_S:-900}" -s -o "$WORK/load5.json" -w '%{http_code}' -X POST --data-binary @"$PROGRAM" "$BASE/program")"
[ "$status" = "200" ] || fail "phase 9: program reload returned $status: $(cat "$WORK/load5.json")"
await_rows banned_call '"src/c.ts","eval"' present \
  || fail "phase 9: the crashed extraction never finished (rows: $(rows_of banned_call), marks $(marks_count))"
final_marks="$(marks_count)"
[ "$final_marks" = "$((mid_kill + 1))" ] \
  || fail "phase 9: expected exactly one re-run of the unanswered witness (marks $mid_kill -> $final_marks)"
say "PASS  phase 9  restart -> the unanswered witness re-ran ONCE and its finding landed (marks $final_marks)"

stop_server
say "EXTRACTION LIVE HOLDS"
