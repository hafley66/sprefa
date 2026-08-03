#!/usr/bin/env bash
# lsp-diags.sh -- THE GOLDEN PLAN'S PHASE-4 LSP EXIT RECEIPT: live editor
# diagnostics with ZERO new LSP code. v5's `dl --lsp --diag-db <sqlite file>`
# (src/lsp.rs, M5.3) already polls a `diag_v5` view/table on a 500ms cadence
# and republishes it as `textDocument/publishDiagnostics`; this script proves
# a served tsv2 program (../dl/fixtures/diag-rail.dl6) writes exactly that
# shape into a real file-backed db, and that the real v5 binary reads it live
# over real LSP stdio.
#
# THE BRIDGE IS ENTIRELY IN-LANGUAGE, not a serve-side projection: v6's rel
# table naming is the bare rel name (v6/prolog/compile/lower.pl:156
# `table_name(Name/_Arity, Name)`, unlike v6/dl's `rel_<name>` convention),
# so a `.dl6` rel literally named `diag_v5` with v5's 9 columns in order
# compiles to a real SQLite table named `diag_v5` -- the exact identifier
# `poll_diag_v5_once` (src/lsp.rs:537) selects from. No CREATE VIEW, no
# COALESCE, no serve/*.ts touched: `v6/tsv2/serve/` is byte-unmodified by
# this arc. Confirmed empirically before writing the rail (see diag-rail.dl6's
# header) by compiling a one-rel probe program and reading its emitted DDL.
#
# LINE-NUMBER HONESTY, stated again because it is the one real gap here:
# every diag_v5 row this rail emits is WHOLE-FILE (line=col=end_line=end_col
# = 0). `sprefa-extract` emits byte spans as a nested json object; the
# sh-host decode only reads declared TOP-LEVEL columns (serve/1_hosts.ts
# decodeObjectItems), so a span cannot become an int column without
# `decode/2`, which stays refused (ARCH row decode_arc, same wall
# flagship-callgraph.dl6 hit). Nothing in this script or diag-rail.dl6
# computes a line number outside the engine to paper over that.
#
# TWO DIAGNOSTIC CLASSES, both real, both from proven rails:
#   no-eval      (error)  -- extraction-live.dl6's own eval-callsite finding
#   unused-def   (warn)   -- flagship-callgraph.dl6's `unused` rule, verbatim
#
# TWO RECEIPTS, both directions, in TWO DIFFERENT files so ordering can't
# smear one phase into the other:
#   a.ts  -- the ENGINE-SIDE receipt: curl the served `/idb/diag_v5` rel
#            directly (the same 9 columns, the same table) before any LSP
#            client exists. Proves the table.
#   b.ts  -- the LSP-SIDE receipt: `lsp_diag_driver.py` speaks real LSP
#            JSON-RPC over stdio to the real `dl --lsp --diag-db` binary,
#            watches b.ts get written (diagnostics appear) and then fixed
#            (diagnostics retract), entirely through publishDiagnostics
#            notifications -- never touching the db directly. Proves the
#            reader.
#
# `unused` IS A GLOBAL NAME CHECK, not per-file -- copied verbatim from v5's
# own rule (`unused(name) <- def(name,_,_), !call(name,_,_)`, no path scoping
# at all; flagship-callgraph.dl6's header calls this out too). b.ts's
# unused-def helper is named `helperUnusedB`, not `helperUnused`: by the time
# phase B starts, phase A has already CALLED `helperUnused` in a.ts, which
# makes that exact name read as globally used everywhere in the corpus. This
# is real v5-faithful semantics surfacing during test authorship, not a
# workaround for a defect.
#
# FOUND WHILE WRITING THIS RECEIPT, DISCLOSED NOT PATCHED (src/lsp.rs is
# read-only for this arc): `dl --lsp --diag-db` answers `shutdown` correctly
# ({"id":2,"result":null}) but then does not exit after receiving `exit` +
# stdin EOF -- confirmed with a STANDALONE probe outside this harness (same
# 5-message handshake, no tsv2 involved, against a nonexistent db path so
# diag polling never even fires) so this is not a tsv2-side artifact.
# lsp_diag_driver.py downgrades this to a NOTE (SIGKILL after a 10s grace)
# rather than failing the receipt, since it happens strictly after both
# diagnostic directions are already proven. `lsp-server` 0.7.9's own
# `handle_shutdown` (~/.cargo/registry/.../lsp-server-0.7.9/src/lib.rs:348)
# blocks on `self.receiver.recv_timeout(30s)` waiting for the exit
# notification on the SAME channel the reader thread feeds; the reader
# thread's own loop (stdio.rs) breaks the moment it reads and forwards an
# exit notification, so the hang is not an obviously-stuck read on either
# thread by inspection alone -- worth a closer look by whoever owns
# src/lsp.rs, not chased further here.
#
# SABOTAGE RECEIPT (actually run 2026-07-29 by hand while writing this
# script, reverted after, not part of the automated flow below -- same
# convention as extraction-live.sh's own header): renamed diag-rail.dl6's
# `path` column to `file_path` (a legal rel column name; recompiled clean,
# and phases A1-A4 -- the engine-side `/idb/diag_v5` receipt -- STILL PASSED,
# since curl reads the row by POSITION, blind to column names). Re-ran the
# full script: v5's `poll_diag_v5_once` issues `SELECT path, line, col, ...
# FROM diag_v5`; `path` no longer exists, sqlite returns "no such column:
# path"; the error propagates up through `poll_diag_v5_once`,
# `diag_db_poll_loop` logs a `tracing::warn` and retries forever -- no crash,
# but also no `publishDiagnostics` notification EVER arrives. Observed exit:
#   PASS  phase A1..A4  (unaffected, column-name-blind)
#   PASS  phase B0      dl --lsp --diag-db initialized over real stdio
#   FAIL  phase B1: the real LSP client never received both diagnostics for
#         b.ts: READY
# exit 1. This is the whole point of the column-contract framing: a v6-side
# rename that is completely legal DDL, and invisible to the engine-side
# `/idb/:rel` receipt, silently breaks the v5 reader -- only a live
# cross-process receipt like phase B catches it. Reverted the column name
# (`git diff` clean) and re-ran the full script clean before committing.
set -uo pipefail
TSV2="$(cd "$(dirname "$0")/.." && pwd)"
REPO="$(cd "$TSV2/../.." && pwd)"

PORT="${TSV2_LSP_DIAG_PORT:-17575}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/tsv2-lspdiag.XXXXXX")"
CORPUS="$WORK/corpus"
DB="file:$WORK/diag.sqlite"
DB_PATH="$WORK/diag.sqlite"
PROGRAM="$TSV2/../dl/fixtures/diag-rail.dl6"
SERVE_MAIN="$TSV2/serve/main.ts"
BASE="http://127.0.0.1:$PORT"
SERVER_PID=""
DRIVER_PID=""
mkdir -p "$CORPUS"
cd "$CORPUS"

fail() { printf 'FAIL  %s\n' "$*"; [ -n "$SERVER_PID" ] && tail -20 "$WORK/server.log"; [ -f "$WORK/driver.log" ] && { printf -- '--- driver log ---\n'; cat "$WORK/driver.log"; }; stop_all; exit 1; }
say() { printf '%s\n' "$*"; }

stop_all() {
  [ -n "$DRIVER_PID" ] && kill -9 "$DRIVER_PID" 2>/dev/null
  wait "$DRIVER_PID" 2>/dev/null
  DRIVER_PID=""
  [ -n "$SERVER_PID" ] && kill -9 "$SERVER_PID" 2>/dev/null
  wait "$SERVER_PID" 2>/dev/null
  SERVER_PID=""
}
trap stop_all EXIT

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
    fail "no release extractor. A gate does not build; run:
      cd $crate && cargo build --release --features cli --bin extract"
  fi
  [ -x "$release" ] || fail "no extract binary at $release"
  export DL_EXTRACT_BIN="$release"
  say "extract bin: $DL_EXTRACT_BIN (in-tree release)"
}

# ── the v5 binary: same in-tree release resolution as flagship-callgraph.sh ──
resolve_v5_bin() {
  if [ -n "${DL_V5_BIN:-}" ] && [ -x "$DL_V5_BIN" ]; then say "v5 bin: $DL_V5_BIN (DL_V5_BIN)"; return; fi
  local release="$REPO/target/release/dl"
  if [ ! -x "$release" ]; then
    fail "no v5 dl binary. A gate does not build; run:
      cd $REPO && cargo build --release --bin dl"
  fi
  [ -x "$release" ] || fail "no v5 dl binary at $release"
  DL_V5_BIN="$release"
  say "v5 bin: $DL_V5_BIN (in-tree release)"
}

start_server() {
  TSV2_DB="$DB" TSV2_PORT="$PORT" TSV2_WATCH_COALESCE_MS=60 \
    DL_EXTRACT_BIN="$DL_EXTRACT_BIN" \
    node --experimental-transform-types "$SERVE_MAIN" >>"$WORK/server.log" 2>&1 &
  SERVER_PID=$!
  for _ in $(seq 1 60); do
    curl -s -o /dev/null "$BASE/ticks" 2>/dev/null && return
    kill -0 "$SERVER_PID" 2>/dev/null || fail "server died on boot: $(tail -5 "$WORK/server.log")"
    sleep 0.2
  done
  fail "server did not become ready on port $PORT"
}

rows_of() { curl -s "$BASE/idb/$1" | tr -d ' \n'; }

await_rows() { # REL NEEDLE present|absent
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

await_log_line() { # LOGFILE NEEDLE TIMEOUT_S
  local log="$1" needle="$2" wait_s="$3"
  local deadline=$((SECONDS + wait_s))
  while [ "$SECONDS" -lt "$deadline" ]; do
    grep -qF "$needle" "$log" 2>/dev/null && return 0
    [ -n "$DRIVER_PID" ] && ! kill -0 "$DRIVER_PID" 2>/dev/null && return 1
    sleep 0.2
  done
  return 1
}

resolve_extract_bin
resolve_v5_bin
start_server

status="$(curl -s -o "$WORK/load.json" -w '%{http_code}' -X POST --data-binary @"$PROGRAM" "$BASE/program")"
[ "$status" = "200" ] || fail "program load returned $status: $(cat "$WORK/load.json")"
grep -q '"watch"' "$WORK/load.json" || fail "loaded program declares no watch bind: $(cat "$WORK/load.json")"
say "PASS  program loaded, binds: $(cat "$WORK/load.json" | tr -d ' \n' | sed 's/.*"binds"://; s/,"hosts".*//')"

# ══ PHASE A: engine-side receipt -- diag_v5 through the served rel directly ══
cat >"$CORPUS/a.ts" <<'EOF'
export function helperUnused(x: number): number {
  return x + 1;
}

export function danger(source: string): unknown {
  return eval(source);
}

danger("ok");
EOF
await_rows diag_v5 '"a.ts",0,0,0,0,"error","no-eval"' present \
  || fail "phase A1: diag_v5 never gained a.ts's no-eval row (rows: $(rows_of diag_v5))"
say "PASS  phase A1  a.ts eval() -> diag_v5 no-eval row: $(rows_of diag_v5)"
await_rows diag_v5 '"a.ts",0,0,0,0,"warn","unused-def"' present \
  || fail "phase A2: diag_v5 never gained a.ts's unused-def row (rows: $(rows_of diag_v5))"
say "PASS  phase A2  a.ts helperUnused -> diag_v5 unused-def row: $(rows_of diag_v5)"

cat >"$CORPUS/a.ts" <<'EOF'
export function helperUnused(x: number): number {
  return x + 1;
}

export function danger(source: string): unknown {
  return JSON.parse(source);
}

danger("ok");
helperUnused(1);
EOF
await_rows diag_v5 '"a.ts",0,0,0,0,"error","no-eval"' absent \
  || fail "phase A3: diag_v5 still holds a.ts's no-eval row after the eval was removed (rows: $(rows_of diag_v5))"
say "PASS  phase A3  eval removed -> no-eval row retracted"
await_rows diag_v5 '"a.ts",0,0,0,0,"warn","unused-def"' absent \
  || fail "phase A4: diag_v5 still holds a.ts's unused-def row after helperUnused was called (rows: $(rows_of diag_v5))"
say "PASS  phase A4  helperUnused called -> unused-def row retracted"

# ══ PHASE B: LSP-side receipt -- the real v5 binary over real stdio ══════════
DL_STATE_DIR="$WORK/state" python3 "$TSV2/scripts/lsp_diag_driver.py" \
  "$DL_V5_BIN" "$DB_PATH" "$CORPUS" b.ts no-eval,unused-def 30 \
  >"$WORK/driver.log" 2>&1 &
DRIVER_PID=$!
await_log_line "$WORK/driver.log" "READY" 15 \
  || fail "phase B0: lsp_diag_driver.py never became READY: $(cat "$WORK/driver.log")"
say "PASS  phase B0  dl --lsp --diag-db initialized over real stdio (DL_STATE_DIR=$WORK/state)"

# `helperUnusedB`, NOT `helperUnused`: `unused` is a GLOBAL name check, copied
# verbatim from v5's own rule (`unused(name) <- def(name,_,_), !call(name,_,_)`,
# no path scoping at all -- flagship-callgraph.dl6's own header calls this out).
# By the time phase B starts, phase A's a.ts already CALLS `helperUnused`
# (phase A4), so that name reads as globally used everywhere in the corpus,
# b.ts included -- a distinct name is what makes b.ts's own unused-def finding
# independently observable, not a workaround for a defect.
cat >"$CORPUS/b.ts" <<'EOF'
export function helperUnusedB(x: number): number {
  return x + 1;
}

export function danger(source: string): unknown {
  return eval(source);
}

danger("ok");
EOF
await_log_line "$WORK/driver.log" "PASS appeared" 30 \
  || fail "phase B1: the real LSP client never received both diagnostics for b.ts: $(cat "$WORK/driver.log")"
say "PASS  phase B1  $(grep 'PASS appeared' "$WORK/driver.log")"

cat >"$CORPUS/b.ts" <<'EOF'
export function helperUnusedB(x: number): number {
  return x + 1;
}

export function danger(source: string): unknown {
  return JSON.parse(source);
}

danger("ok");
helperUnusedB(1);
EOF
await_log_line "$WORK/driver.log" "PASS retracted" 30 \
  || fail "phase B2: the real LSP client's diagnostics for b.ts never retracted: $(cat "$WORK/driver.log")"
say "PASS  phase B2  $(grep 'PASS retracted' "$WORK/driver.log")"

wait "$DRIVER_PID"
driver_exit=$?
DRIVER_PID=""
[ "$driver_exit" = "0" ] || fail "phase B3: lsp_diag_driver.py exited $driver_exit: $(cat "$WORK/driver.log")"
say "PASS  phase B3  $(grep -E 'clean shutdown|^NOTE' "$WORK/driver.log")"

stop_all
say "artifacts: $WORK"
say "LSP DIAGS HOLDS"
