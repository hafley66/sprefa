#!/usr/bin/env bash
# v5-git-diags.sh — the receipt for ../dl/fixtures/v5-git-diags.dl6.
#
# It builds a scratch git repository with a REAL FOUR-COMMIT SEQUENCE and drives
# the served tsv2 engine across those commit boundaries, asserting that each of
# the three finding classes goes RED when the commit that breaks the rail lands
# and GREEN again when the commit that fixes it lands. Row-set equality at every
# stage, never "some diags exist".
#
# ── THE CORPUS AND THE SEQUENCE ─────────────────────────────────────────────
#   c0  src/existing.ts   one `eval(`, Owner header      -> the ratchet baseline
#       src/clean.ts      no eval, Owner header
#   c1  + src/new_bad.ts  one `eval(`, NO Owner header   -> classes (a)+(b)+(c)
#       + src/new_ok.ts   no eval, Owner header          -> the CONTROL: new,
#                                                           and still no finding
#   c2  - src/new_bad.ts                                 -> (a),(b),(c) retract
#   c3  (base advances)                                  -> new_file empties
#
# The control file is the part that makes this a test rather than a smoke run:
# a rail that fires on every new file would pass stages 1-3 and only `new_ok`
# catches it.
#
# ── ISOLATION ───────────────────────────────────────────────────────────────
# Everything lives in one mktemp directory: the git repository, the server's
# cwd, and an in-memory db. Nothing under ~/.local/state/sprefa is read or
# written and no daemon is contacted. `git -c user.*` is passed per-commit so
# the run does not depend on, or touch, global git config.
#
# ── SABOTAGE RECEIPT (run 2026-07-30, reverted) ─────────────────────────────
# Replacing the class-(b) rule's `not(owner_marked(path))` with a bare
# `owner_marked(path)` inverts it: the rail then reports the file that HAS an
# Owner header and stays silent about the one that does not. Stage 1 fails on
# row-set equality naming `src/new_ok.ts` as an unexpected `new-file-missing-
# owner` row and `src/new_bad.ts` as a missing one. Asserting a COUNT instead of
# a row set would have passed that sabotage: both spellings emit exactly one
# row at stage 1. That is why every assertion below compares sorted row sets.
set -uo pipefail

TSV2="$(cd "$(dirname "$0")/.." && pwd)"
REPO="$(cd "$TSV2/../.." && pwd)"
PROGRAM="$TSV2/../dl/fixtures/v5-git-diags.dl6"
SERVE_MAIN="$TSV2/serve/main.ts"
PORT="${TSV2_GIT_DIAGS_PORT:-17591}"
BASE="http://127.0.0.1:$PORT"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/tsv2-gitdiags.XXXXXX")"
ROOT="$WORK/root"
GLOB='src/*.ts'
SERVER_PID=""
FAILURES=0

export PARITY_GREP="$TSV2/scripts/parity-grep.py"

say()  { printf '%s\n' "$*"; }
fail() { printf 'FAIL  %s\n' "$*"; FAILURES=$((FAILURES + 1)); }
die()  { printf 'FAIL  %s\n' "$*"; [ -n "$SERVER_PID" ] && tail -20 "$WORK/server.log"; stop_server; exit 1; }
stop_server() {
  [ -n "$SERVER_PID" ] && kill -9 "$SERVER_PID" 2>/dev/null
  wait "$SERVER_PID" 2>/dev/null
  SERVER_PID=""
}
trap stop_server EXIT

commit() { git -C "$ROOT" add -A && git -C "$ROOT" -c user.email=rail@sprefa -c user.name=git-diags-rail commit -q -m "$1"; }

# ── build the corpus ────────────────────────────────────────────────────────
mkdir -p "$ROOT/src"
git -C "$ROOT" init -q .

cat >"$ROOT/src/existing.ts" <<'EOF'
// Owner: platform
export function existing(source: string) {
  return eval(source);
}
EOF
cat >"$ROOT/src/clean.ts" <<'EOF'
// Owner: platform
export const clean = 1;
EOF
commit "c0" || die "could not build c0"
C0="$(git -C "$ROOT" rev-parse HEAD)"

# The baseline ceiling is MEASURED off c0 rather than written as a literal, so
# the ratchet's committed number is the tree's own number and a corpus edit
# cannot silently make the ratchet vacuous.
BASELINE="$(cd "$ROOT" && python3 "$PARITY_GREP" "$C0" "$GLOB" '(?<![a-z_0-9])eval[ ]*[(]' | wc -l | tr -d ' ')"
[ "$BASELINE" = "1" ] || die "expected exactly 1 eval( hit at c0, measured $BASELINE"
say "PASS  c0 built: 2 files, ratchet baseline measured at $BASELINE hit"

# ── boot the server IN THE SCRATCH ROOT (git and the grep host run there) ───
cd "$ROOT"
TSV2_DB=":memory:" TSV2_PORT="$PORT" PARITY_GREP="$PARITY_GREP" \
  node --experimental-transform-types "$SERVE_MAIN" >"$WORK/server.log" 2>&1 &
SERVER_PID=$!
for _ in $(seq 1 60); do
  curl -s -o /dev/null "$BASE/ticks" 2>/dev/null && break
  kill -0 "$SERVER_PID" 2>/dev/null || die "server died on boot: $(tail -5 "$WORK/server.log")"
  sleep 0.2
done

status="$(curl -s -o "$WORK/load.json" -w '%{http_code}' -X POST --data-binary @"$PROGRAM" "$BASE/program")"
[ "$status" = "200" ] || die "program load returned $status: $(cat "$WORK/load.json")"
say "PASS  program loaded, hosts: $(sed 's/.*"hosts":\[//; s/\].*//' "$WORK/load.json")"

post() { curl -s -o /dev/null -X POST --data-binary "$1" "$BASE/edb/events"; }

# `sign: del` retracts the row that carried the old rev, so exactly one
# `want_at` row is live at a time and the file set is never a union of revs.
advance_want() { # advance_want OLD NEW  (OLD may be empty for the first post)
  [ -n "$1" ] && post "{\"batch\":[{\"rel\":\"want_at\",\"sign\":\"del\",\"row\":[\"$1\",\"$GLOB\"]}]}"
  post "{\"batch\":[{\"rel\":\"want_at\",\"sign\":\"add\",\"row\":[\"$2\",\"$GLOB\"]}]}"
}
advance_base() {
  [ -n "$1" ] && post "{\"batch\":[{\"rel\":\"base_at\",\"sign\":\"del\",\"row\":[\"$1\",\"$GLOB\"]}]}"
  post "{\"batch\":[{\"rel\":\"base_at\",\"sign\":\"add\",\"row\":[\"$2\",\"$GLOB\"]}]}"
}

# Settle = four consecutive identical readings of both graded rels. A single
# read lands mid-extraction and calls a half-finished tick the answer.
settle() {
  local last="" stable=0 now deadline=$((SECONDS + 120))
  while [ "$SECONDS" -lt "$deadline" ]; do
    now="$(curl -s "$BASE/idb/new_file")|$(curl -s "$BASE/idb/diag_v5")"
    if [ "$now" = "$last" ]; then
      stable=$((stable + 1))
      [ "$stable" -ge 3 ] && return 0
    else
      stable=0
    fi
    last="$now"
    sleep 1
  done
  return 1
}

# Row-set assertions. Both sides are sorted so the comparison is order-free;
# the engine makes no ordering promise and a rel is a set.
read_rel() { # read_rel REL COLUMN_INDEXES
  curl -s "$BASE/idb/$1" | python3 -c '
import json,sys
cols=[int(c) for c in sys.argv[1].split(",")]
rows=json.load(sys.stdin)["rows"]
print("\n".join(sorted("\t".join(str(r[c]) for c in cols) for r in rows)))' "$2"
}
expect() { # expect LABEL ACTUAL EXPECTED
  if [ "$2" = "$3" ]; then
    say "PASS  $1"
  else
    fail "$1"
    printf '      expected:\n%s\n      actual:\n%s\n' "${3:-  (empty)}" "${2:-  (empty)}"
  fi
}

# ═══ STAGE 0: base == head, nothing is new, everything green ════════════════
advance_want "" "$C0"
advance_base "" "$C0"
post "{\"batch\":[{\"rel\":\"baseline\",\"sign\":\"add\",\"row\":[\"ratchet-eval\",$BASELINE]}]}"
settle || die "stage 0 never settled"
expect "stage 0  new_file empty (base == head)" "$(read_rel new_file 0)" ""
expect "stage 0  diag_v5 empty (GREEN)"          "$(read_rel diag_v5 6,0,1)" ""

# ═══ THE WHITESPACE-DECODE COLLAPSE, now a REGRESSION GUARD ════════════════
# This block was written as a DEFECT WITNESS: c0 holds exactly TWO matching
# files and `files_at` declares exactly TWO output columns, and
# serve/1_hosts.ts `parseWhitespace` used to try the one-value-per-line reading
# FIRST whenever those two numbers matched, so a two-file answer -- and only a
# two-file answer -- decoded into ONE row whose `path` was the whole first line
# and whose `digest` was the whole second. Right at one file, right at three,
# silently wrong at two.
#
# 7c338827 fixed it by precedence: the GRID reading (every nonempty line splits
# into exactly N fields) is unambiguous, so it goes first. tests/hostDecode.
# test.ts pins all four cardinalities 0/1/2/3 at the decode seam.
#
# So the witness is inverted here rather than deleted, and stays for one
# reason: it is the only check that runs the collapse cardinality END TO END --
# a real git repository, a real subprocess host, two declarations differing
# ONLY in output encoding, through the served engine. A decode bug that is a
# function of ROW COUNT is invisible to any receipt whose corpus is the wrong
# size, which is exactly how this one shipped. The two encodings must now agree
# at every cardinality; stage 3 asserts the same equality at three files.
expect "decode   JSON host reads c0's two files as TWO ROWS (correct)" \
  "$(read_rel base_file 0)" "$(printf 'src/clean.ts\nsrc/existing.ts')"
expect "decode   whitespace host agrees at TWO files (the 7c338827 grid-first fix holds)" \
  "$(read_rel ws_file 0)" "$(read_rel base_file 0)"

# ═══ STAGE 1: c1 adds one offending file and one clean new file ════════════
cat >"$ROOT/src/new_bad.ts" <<'EOF'
export function danger(source: string) {
  return eval(source);
}
EOF
cat >"$ROOT/src/new_ok.ts" <<'EOF'
// Owner: platform
export const ok = 2;
EOF
commit "c1" || die "could not build c1"
C1="$(git -C "$ROOT" rev-parse HEAD)"
# The line the rail must report, computed from the file rather than counted by
# hand, and converted to v5's 0-based diag convention here so the assertion is
# checking the PROGRAM's `:=` bind rather than restating it.
BAD_LINE_1BASED="$(grep -n 'eval(' "$ROOT/src/new_bad.ts" | head -1 | cut -d: -f1)"
BAD_LINE=$((BAD_LINE_1BASED - 1))

advance_want "$C0" "$C1"
settle || die "stage 1 never settled"

expect "stage 1  new_file = the two files c1 added" \
  "$(read_rel new_file 0)" "$(printf 'src/new_bad.ts\nsrc/new_ok.ts')"
expect "stage 1  class (a) banned pattern, at a REAL 0-based line" \
  "$(read_rel diag_v5 6,0,1 | grep '^new-file-banned-eval' || true)" \
  "$(printf 'new-file-banned-eval\tsrc/new_bad.ts\t%s' "$BAD_LINE")"
expect "stage 1  class (b) missing Owner header, ONLY on the file lacking it" \
  "$(read_rel diag_v5 6,0,1 | grep '^new-file-missing-owner' || true)" \
  "$(printf 'new-file-missing-owner\tsrc/new_bad.ts\t0')"
expect "stage 1  class (c) ratchet rose above the baseline" \
  "$(read_rel diag_v5 6,0,1 | grep '^ratchet-eval' || true)" \
  "$(printf 'ratchet-eval\t(ratchet)\t0')"
say "RED   all three classes fire at c1; the control file src/new_ok.ts is new and carries ZERO findings"

# ═══ STAGE 2: c2 deletes the offending file ════════════════════════════════
rm "$ROOT/src/new_bad.ts"
commit "c2" || die "could not build c2"
C2="$(git -C "$ROOT" rev-parse HEAD)"
advance_want "$C1" "$C2"
settle || die "stage 2 never settled"

expect "stage 2  new_file = the surviving new file only" \
  "$(read_rel new_file 0)" "src/new_ok.ts"
expect "stage 2  classes (a) and (b) RETRACTED" \
  "$(read_rel diag_v5 6,0,1 | grep '^new-file-' || true)" ""
expect "stage 2  class (c) ratchet RETRACTED (count back to the baseline)" \
  "$(read_rel diag_v5 6,0,1 | grep '^ratchet-eval' || true)" ""
say "GREEN all three classes retracted on the fixing commit"

# ═══ STAGE 3: accept the tree by advancing the base ════════════════════════
advance_base "$C0" "$C2"
settle || die "stage 3 never settled"
expect "stage 3  new_file empty once the base advances to head" "$(read_rel new_file 0)" ""
expect "stage 3  diag_v5 empty (GREEN)" "$(read_rel diag_v5 6,0,1)" ""
# c2 holds THREE matching files against two declared columns. Three was always
# the cardinality the old collapse could not reach, so this leg passed before
# the fix and passes after it -- keeping it is what makes the pair a
# CARDINALITY sweep (2 and 3) rather than a single sample.
expect "decode   at THREE files the two encodings agree (cardinality sweep, 2 and 3)" \
  "$(read_rel ws_file 0)" "$(read_rel base_file 0)"

# ═══ the ratchet's own red leg, proven by lowering the ceiling ══════════════
# Stage 1 proved the ratchet fires when the COUNT rises. This proves the other
# half of the same inequality -- that the ceiling is really read from the row
# and not compiled in -- by dropping the ceiling under the unchanged count.
post "{\"batch\":[{\"rel\":\"baseline\",\"sign\":\"del\",\"row\":[\"ratchet-eval\",$BASELINE]}]}"
post "{\"batch\":[{\"rel\":\"baseline\",\"sign\":\"add\",\"row\":[\"ratchet-eval\",0]}]}"
settle || die "ratchet ceiling stage never settled"
expect "ratchet  lowering the committed ceiling to 0 fires the rail on an unchanged tree" \
  "$(read_rel diag_v5 6,0,1 | grep '^ratchet-eval' || true)" \
  "$(printf 'ratchet-eval\t(ratchet)\t0')"

stop_server
if [ "$FAILURES" -gt 0 ]; then
  say "GIT-FACT DIAGS RAIL: $FAILURES assertion(s) failed (artifacts: $WORK)"
  exit 1
fi
say "artifacts: $WORK"
say "GIT-FACT DIAGS RAIL HOLDS"
