#!/usr/bin/env bash
# receipt.sh -- DELIVERABLE 4: both dogfood programs on the REAL served engine
# over REAL files, the extraction-live.sh precedent. Nothing is faked: node's
# own fs.watch behind the bind seam, the in-tree release extractor, the real
# emitted SQL, and every row below read back out of SQLite through the
# program's own emitted SELECT.
#
# PROGRAM 1 (suppress-rail.dl6) -- v5 techniques 2 + 7
#   1  a file with an unguarded eval()          -> diag_v5 gains a no-eval row
#                                                  AT A REAL LINE NUMBER (the
#                                                  byte-span flattener; every
#                                                  prior v6 rail shipped 0)
#   2  add `// dl-disable-line no-eval` on that line
#                                               -> the diag is GONE
#   3  the same directive text inside a STRING literal
#                                               -> the diag COMES BACK, because
#                                                  the grammar witness join
#                                                  finds no comment node on
#                                                  that line. THIS IS THE
#                                                  STRING-LITERAL-SAFETY
#                                                  PROPERTY, live, in-language.
#   4  `// dl-disable-next-line no-eval` one line above
#                                               -> suppressed again (the `+ 1`
#                                                  is int arithmetic in the
#                                                  program, not in the host)
#   5  a directive guarding nothing             -> dl-suppress-unused warn
#                                                  (the antijoin)
#
# PROGRAM 2 (arch-rail.dl6) -- v5 technique 1
#   6  three ARCH markers + one inside a quoted atom
#                                               -> 3 arch_node rows, not 4
#   7  the hierarchy                            -> arch_edge / arch_root /
#                                                  arch_child_count as joins
#
# SABOTAGE RECEIPT (run, then reverted): deleting the `comment_node(...)` atom
# from `suppressed`'s two clauses in suppress-rail.dl6 -- i.e. trusting the
# scanner without the grammar -- passes phases 1, 2, 4 and 5 and FAILS PHASE 3
# with "phase 3: the directive inside a string literal suppressed the diag",
# which is the exact false positive string-safety.sh witnesses statically.
set -uo pipefail
LAB="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$LAB/../../../.." && pwd)"
TSV2="$ROOT/v6/tsv2"
PORT="${CN_RECEIPT_PORT:-17601}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/cn-receipt.XXXXXX")"
CORPUS="$WORK/corpus"
DB="file:$WORK/cn.sqlite"
BASE="http://127.0.0.1:$PORT"
SERVE_MAIN="$TSV2/serve/main.ts"
SERVER_PID=""
FAILED=0

export DL_CN="$LAB/cn.py"
export DL_EXTRACT_BIN="${DL_EXTRACT_BIN:-$ROOT/v6/sprefa-extract/target/release/extract}"

mkdir -p "$CORPUS/src"
cd "$CORPUS"
git init -q

say() { printf '%s\n' "$*"; }
bad() { printf 'FAIL  %s\n' "$*"; FAILED=1; }
stop_server() { [ -n "$SERVER_PID" ] && kill -9 "$SERVER_PID" 2>/dev/null; wait "$SERVER_PID" 2>/dev/null; SERVER_PID=""; }
trap stop_server EXIT

[ -x "$DL_EXTRACT_BIN" ] || { echo "FAIL no extract binary at $DL_EXTRACT_BIN"; exit 1; }

start_server() {
  TSV2_DB="$DB" TSV2_PORT="$PORT" TSV2_WATCH_COALESCE_MS=60 \
    node --experimental-transform-types "$SERVE_MAIN" >>"$WORK/server.log" 2>&1 &
  SERVER_PID=$!
  for _ in $(seq 1 60); do
    curl -s -o /dev/null "$BASE/ticks" 2>/dev/null && return 0
    kill -0 "$SERVER_PID" 2>/dev/null || { echo "server died: $(tail -20 "$WORK/server.log")"; exit 1; }
    sleep 0.2
  done
  echo "server never became ready"; exit 1
}

load_program() {
  local status
  status="$(curl -s -o "$WORK/load.json" -w '%{http_code}' -X POST --data-binary @"$1" "$BASE/program")"
  [ "$status" = "200" ] || { echo "FAIL program load $1 returned $status: $(cat "$WORK/load.json")"; exit 1; }
}

rows_of() { curl -s "$BASE/idb/$1" | tr -d ' \n'; }

# poll until rows_of $1 contains ($3=present) / stops containing ($3=absent) $2
await() {
  local rel="$1" needle="$2" mode="${3:-present}" limit=$((SECONDS + 40))
  while [ "$SECONDS" -lt "$limit" ]; do
    local rows; rows="$(rows_of "$rel")"
    case "$mode" in
      present) case "$rows" in *"$needle"*) return 0;; esac ;;
      absent)  case "$rows" in *"$needle"*) ;; *) return 0;; esac ;;
    esac
    sleep 0.25
  done
  return 1
}

start_server

# ═══ PROGRAM 1 ══════════════════════════════════════════════════════════════
load_program "$LAB/programs/suppress-rail.dl6"
say "loaded suppress-rail.dl6"

# ── phase 1: an unguarded eval, at a real line ──────────────────────────────
cat >"$CORPUS/src/a.ts" <<'EOF'
export function danger(source: string): unknown {
  return eval(source);
}
EOF
if await diag_v5 '"src/a.ts",2' present; then
  say "PASS  phase 1  no-eval diag at LINE 2 (a real line number, not 0): $(rows_of diag_v5)"
else
  bad "phase 1: no diag_v5 row at src/a.ts line 2 (rows: $(rows_of diag_v5))"
fi

# ── phase 2: dl-disable-line on the offending line ──────────────────────────
cat >"$CORPUS/src/a.ts" <<'EOF'
export function danger(source: string): unknown {
  return eval(source); // dl-disable-line no-eval
}
EOF
if await diag_v5 '"no-eval"' absent; then
  say "PASS  phase 2  dl-disable-line suppressed the live diag"
else
  bad "phase 2: the diag survived its dl-disable-line (rows: $(rows_of diag_v5))"
fi

# ── phase 3: THE STRING-LITERAL-SAFETY PROPERTY, live ───────────────────────
# The identical directive text, inside a string literal. The scanner host
# still reports it; the grammar witness join drops it; the diag returns.
cat >"$CORPUS/src/a.ts" <<'EOF'
export function danger(source: string): unknown {
  return eval(source); const hint = "dl-disable-line no-eval";
}
EOF
if await diag_v5 '"src/a.ts",2' present; then
  say "PASS  phase 3  a directive INSIDE A STRING LITERAL suppresses nothing (grammar witness held)"
else
  bad "phase 3: the directive inside a string literal suppressed the diag (rows: $(rows_of diag_v5))"
fi

# ── phase 4: dl-disable-next-line, the +1 done in-language ──────────────────
cat >"$CORPUS/src/a.ts" <<'EOF'
export function danger(source: string): unknown {
  // dl-disable-next-line no-eval
  return eval(source);
}
EOF
if await diag_v5 '"no-eval"' absent; then
  say "PASS  phase 4  dl-disable-next-line suppressed line+1 (int arithmetic in the rule)"
else
  bad "phase 4: dl-disable-next-line did not suppress (rows: $(rows_of diag_v5))"
fi

# ── phase 5: the unused-suppression antijoin ────────────────────────────────
cat >"$CORPUS/src/b.ts" <<'EOF'
export function safe(source: string): unknown {
  return JSON.parse(source); // dl-disable-line no-eval
}
EOF
if await diag_v5 '"dl-suppress-unused"' present; then
  say "PASS  phase 5  unused suppression warned (the antijoin): $(rows_of suppress_unused)"
else
  bad "phase 5: a directive guarding nothing produced no dl-suppress-unused (rows: $(rows_of diag_v5))"
fi

# ═══ PROGRAM 2 ══════════════════════════════════════════════════════════════
stop_server
rm -f "$WORK/cn.sqlite"
rm -rf "$CORPUS/src"
mkdir -p "$CORPUS/src"
start_server
load_program "$LAB/programs/arch-rail.dl6"
say "loaded arch-rail.dl6"

# Written AFTER the load, like every phase above: the watcher's live half is
# fs.watch, while its BOOT half is `git ls-files` (tracked paths only), so a
# file that exists before the load and was never `git add`ed is invisible by
# design. That asymmetry is the watch bind's stated contract, not a defect.
cat >"$CORPUS/src/arch_demo.pl" <<'EOF'
% ARCH {"url":"sprefa/compile/01-lower/00-entry","role":"spine"}
lower(X) :- entry(X).

% ARCH {"url":"sprefa/compile/01-lower/01-emit","role":"emit"}
emit(X) :- lower(X).

% ARCH {"url":"sprefa/compile/01-lower","role":"layer"}
layer(x).

% the same marker inside a quoted atom must never become a node:
noise('% ARCH {"url":"fake/not/real"}').
EOF

# ── phase 6: the grammar witness over a real JSON marker ────────────────────
if await arch_node '01-lower/01-emit' present; then
  nodes="$(rows_of arch_node)"
  case "$nodes" in
    *fake/not/real*) bad "phase 6: the marker inside a quoted atom became an arch_node: $nodes" ;;
    *) say "PASS  phase 6  3 markers -> 3 nodes, the quoted-atom marker antijoined: $nodes" ;;
  esac
else
  bad "phase 6: arch_node never gained the real markers (rows: $(rows_of arch_node))"
fi

# ── phase 7: the hierarchy, as ordinary joins ───────────────────────────────
if await arch_edge 'sprefa/compile/01-lower' present; then
  say "PASS  phase 7  hierarchy: edges $(rows_of arch_edge)"
  say "                roots      $(rows_of arch_root)"
  say "                children   $(rows_of arch_child_count)"
else
  bad "phase 7: arch_edge never derived a parent/child pair (rows: $(rows_of arch_edge))"
fi

stop_server
if [ "$FAILED" = 0 ]; then
  say ""
  say "COMMENT DOGFOOD RECEIPTS HOLD"
  exit 0
fi
say ""
say "COMMENT DOGFOOD RECEIPTS FAILED  (server log: $WORK/server.log)"
exit 1
