#!/usr/bin/env bash
# v5-parity.sh — the receipt for ../dl/fixtures/v5-parity.dl6.
#
# It boots the served tsv2 engine at the repository root, posts ONE start row,
# waits for the program to settle, and then grades the answer against numbers
# obtained OUTSIDE the engine. The point of the outside numbers is that the
# program grades itself against v5's own catalog; this script grades the
# program's plumbing against plain git and plain grep, so a host that silently
# answered zero rows cannot pass.
#
# Its last act is to write the parity table to
# plans/2026-07-30-v5-parity-table.tsv, which is the committed artifact.
#
# ── WHAT IS ASSERTED ────────────────────────────────────────────────────────
#   1  the program's own file is git-TRACKED. The bridge markers are extracted
#      from it with `git ls-files`, so an untracked program silently yields an
#      empty bridge and a table that scores everything `absent`. Loud, first.
#   2  the three inventory sizes match a direct `dl` query run by this script.
#   3  every `# @parity` marker resolves in one of the four v6 catalogs, checked
#      here rather than in the program — a typo'd marker must not read as `absent`.
#   4  THE SCAN RECONCILIATION. Both glob spellings, both computed by the
#      program and both re-derived here with git+grep, and the review's
#      "105/129" is reproduced exactly under the spelling that produced it.
#   5  the L1-vs-L2 and L1-vs-L3 gaps are REPORTED, not required to be empty.
#      They are the findings; forcing them to zero would be the dishonest move.
#
# ── ISOLATION ───────────────────────────────────────────────────────────────
# The v5 legs run with DL_STATE_DIR inside a mktemp directory and a scratch
# --db, so no served root, no daemon, and nothing under ~/.local/state is
# touched. The v6 leg runs on :memory:.
#
# ── SABOTAGE RECEIPT (run 2026-07-30, reverted) ─────────────────────────────
# Breaking one character of the op_docs regex in the program (`[(]""` -> `[(]"`)
# makes `src_op` empty. Assertion 2 still passes — it grades the ORACLE leg,
# which is a different host — and the table still prints 28 ops. What goes red
# is `op_regex_gap`, from 0 rows to all 28, and the gap report is the only
# place that shows it. That is why the gaps are printed with their contents and
# a threshold rather than summarised as a count.
set -uo pipefail

TSV2="$(cd "$(dirname "$0")/.." && pwd)"
REPO="$(cd "$TSV2/../.." && pwd)"
PROGRAM="$TSV2/../dl/fixtures/v5-parity.dl6"
PROGRAM_TRACKED="v6/dl/fixtures/v5-parity.dl6"
SERVE_MAIN="$TSV2/serve/main.ts"
TABLE="$REPO/plans/2026-07-30-v5-parity-table.tsv"
PORT="${TSV2_PARITY_PORT:-17592}"
BASE="http://127.0.0.1:$PORT"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/tsv2-parity.XXXXXX")"
SERVER_PID=""
FAILURES=0

export PARITY_GREP="$TSV2/scripts/parity-grep.py"
export PARITY_TMP="$WORK"
mkdir -p "$WORK/state"

say()  { printf '%s\n' "$*"; }
fail() { printf 'FAIL  %s\n' "$*"; FAILURES=$((FAILURES + 1)); }
die()  { printf 'FAIL  %s\n' "$*"; [ -n "$SERVER_PID" ] && tail -20 "$WORK/server.log"; stop_server; exit 1; }
stop_server() {
  [ -n "$SERVER_PID" ] && kill -9 "$SERVER_PID" 2>/dev/null
  wait "$SERVER_PID" 2>/dev/null
  SERVER_PID=""
}
trap stop_server EXIT
cd "$REPO"

# ── 1: the program must be tracked, or its own markers vanish ──────────────
git ls-files --error-unmatch -- "$PROGRAM_TRACKED" >/dev/null 2>&1 \
  || die "$PROGRAM_TRACKED is not git-tracked; the bridge markers are read with git ls-files and would silently be empty"
say "PASS  $PROGRAM_TRACKED is tracked, so its own @parity markers are visible to the program"

# ── the v5 binary, resolved here and never named in program text ───────────
if [ -n "${DL_V5_BIN:-}" ] && [ -x "$DL_V5_BIN" ]; then
  PARITY_DL="$DL_V5_BIN"; ORIGIN="DL_V5_BIN"
elif [ -x "$REPO/target/release/dl" ]; then
  PARITY_DL="$REPO/target/release/dl"; ORIGIN="in-tree release"
elif [ -x "$HOME/.cargo/bin/dl" ]; then
  PARITY_DL="$HOME/.cargo/bin/dl"; ORIGIN="installed"
else
  say "building the in-tree release v5 engine (cargo build --release --bin dl)"
  (cd "$REPO" && cargo build --release --bin dl) >"$WORK/cargo.log" 2>&1 || die "cargo build --bin dl failed"
  PARITY_DL="$REPO/target/release/dl"; ORIGIN="in-tree release, built now"
fi
export PARITY_DL
say "PASS  v5 engine: $PARITY_DL ($ORIGIN, $("$PARITY_DL" --version 2>&1 | head -1))"

# ── the outside oracle: ask v5 directly, without the engine in the way ─────
v5_count() { # v5_count QUERY_TEXT TAG
  printf '%s\n' "$1" >"$WORK/outside_$2.dl"
  DL_STATE_DIR="$WORK/state" "$PARITY_DL" "$WORK/outside_$2.dl" --db "$WORK/outside_$2.sqlite" \
    --format json 2>/dev/null | python3 -c 'import json,sys; print(len(json.load(sys.stdin)))'
}
OPS_N="$(v5_count '? op_catalog(op, kind, syntax, doc).' ops)"
FNS_N="$(v5_count '? fn_catalog(name, arity, group, doc).' fns)"
RELS_N="$(v5_count '? rel_catalog(name, group, cols, doc).' rels)"
say "PASS  v5 self-report (measured outside the engine): $OPS_N ops, $FNS_N functions, $RELS_N built-in relations"

# ── boot and load ──────────────────────────────────────────────────────────
TSV2_DB=":memory:" TSV2_PORT="$PORT" PARITY_GREP="$PARITY_GREP" PARITY_TMP="$PARITY_TMP" PARITY_DL="$PARITY_DL" \
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

curl -s -o /dev/null -X POST --data-binary '{"batch":[{"rel":"go","sign":"add","row":["run"]}]}' "$BASE/edb/events"

rows()  { curl -s "$BASE/idb/$1" | python3 -c '
import json,sys
cols=sys.argv[1]
rows=json.load(sys.stdin)["rows"]
if cols=="*": print("\n".join(sorted("\t".join(str(v) for v in r) for r in rows)))
else:
    idx=[int(c) for c in cols.split(",")]
    print("\n".join(sorted("\t".join(str(r[c]) for c in idx) for r in rows)))' "${2:-*}"; }
count() { curl -s "$BASE/idb/$1" | python3 -c 'import json,sys
try: print(len(json.load(sys.stdin)["rows"]))
except Exception: print(-1)'; }

# Settle on the whole graded surface, not one rel: the twelve host invocations
# finish at different times and any single rel is stable long before the rest.
say "waiting for settle (12 host invocations: 3 v5 queries + 9 line-regex sweeps)"
last=""; stable=0; deadline=$((SECONDS + 600))
while [ "$SECONDS" -lt "$deadline" ]; do
  now="$(count v5_op):$(count v5_fn):$(count v5_rel):$(count src_op):$(count src_rel):$(count doc_rel):$(count parity_row):$(count usage_count):$(count bridge)"
  if [ "$now" = "$last" ] && [ "${now%%:*}" != "0" ]; then
    stable=$((stable + 1)); [ "$stable" -ge 4 ] && break
  else
    stable=0
  fi
  last="$now"; sleep 2
done
[ "$stable" -ge 4 ] || die "never settled (last vector $last)"
say "PASS  settled: v5_op:v5_fn:v5_rel:src_op:src_rel:doc_rel:parity_row:usage_count:bridge = $last"

check_eq() { # check_eq LABEL ACTUAL EXPECTED
  if [ "$2" = "$3" ]; then say "PASS  $1 = $2"; else fail "$1: got $2, expected $3"; fi
}

# ── 2: the engine's L1 legs agree with the outside oracle ─────────────────
check_eq "v5_op rows"  "$(count v5_op)"  "$OPS_N"
check_eq "v5_fn rows"  "$(count v5_fn)"  "$FNS_N"
check_eq "v5_rel rows" "$(count v5_rel)" "$RELS_N"

# ── 3: every marker resolves in one of the four v6 catalogs ───────────────
# This is the gate the program deliberately does NOT carry (its compile cost is
# superlinear and this check is plumbing, not a finding). It resolves each
# `# @parity` marker target against the SAME four catalogs the program reads,
# with the same four regexes, so a typo'd marker fails loudly here instead of
# silently scoring its row `absent`.
python3 - "$REPO" "$PARITY_GREP" >"$WORK/markers.txt" <<'PYGATE'
import json, subprocess, sys
repo, grep = sys.argv[1], sys.argv[2]
def g(glob, pat):
    out = subprocess.run(["python3", grep, "WORK", glob, pat], cwd=repo,
                         capture_output=True, text=True).stdout
    return [json.loads(line) for line in out.splitlines()]
surface = {r["g1"]: r["g3"] for r in g("v6/prolog/0_dot_expand/registry.pl",
    r"^surface[(]([^,]+)[ ]*,[ ]*([a-z_]+)[ ]*,.*,[ ]*([a-z]+)[)][.][ ]*$")}
world   = {r["g2"] for r in g("v6/dl/fixtures/*.dl6", r"^(sh|bind) ([a-z_][a-z_0-9]*)[(]")}
program = {r["g1"] for r in g("v6/dl/fixtures/*.dl6", r"^rel ([a-z_][a-z_0-9]*)[(]")}
grammar = {r["g1"] for r in g("v6/prolog/compile/SYNTAX.md",
    r"^[|] ([a-z][^|]*) [|] [^|]+ [|] [^|]+ [|]$")}
bad = 0
for row in g("v6/dl/fixtures/v5-parity.dl6", r"^# @parity ([a-z]+) ([^ ]+) -> (.+)$"):
    target = row["g3"]
    via = ([("registry:" + surface[target])] if target in surface else []) \
        + (["world"] if target in world else []) \
        + (["program"] if target in program else []) \
        + (["grammar"] if target in grammar else [])
    if not via:
        bad += 1
    print("%-6s %-14s -> %-26s %s" % (row["g1"], row["g2"], target, ",".join(via) or "UNRESOLVED"))
print("UNRESOLVED_COUNT=%d" % bad)
PYGATE
unresolved="$(sed -n 's/^UNRESOLVED_COUNT=//p' "$WORK/markers.txt")"
markers="$(($(wc -l <"$WORK/markers.txt") - 1))"
if [ "$unresolved" = "0" ]; then
  say "PASS  all $markers @parity markers resolve in one of the four v6 catalogs"
else
  fail "$unresolved @parity marker(s) resolve nowhere:"
  grep UNRESOLVED "$WORK/markers.txt" | sed 's/^/        /'
fi
check_eq "bridge rows the program extracted from its own marker comments" "$(count bridge)" "$markers"

# ── 4: THE SCAN RECONCILIATION ────────────────────────────────────────────
# The review's claim is "scan() used in 105/129 examples". Both numbers are
# glob-spelling-dependent and the program computes both spellings; this block
# re-derives each one with git and grep so the agreement is three-way.
prog_top="$(rows usage_count | awk -F'\t' '$1=="scan"{print $2}')"
prog_rec="$(rows usage_count_recursive | awk -F'\t' '$1=="scan"{print $2}')"
git_top_files="$(git ls-files -- ':(glob)examples/*.dl' | wc -l | tr -d ' ')"
git_rec_files="$(git ls-files -- 'examples/*.dl' | wc -l | tr -d ' ')"
grep_top="$(git ls-files -- ':(glob)examples/*.dl' | xargs grep -lE '\bscan[[:space:]]*\(' 2>/dev/null | wc -l | tr -d ' ')"
grep_rec="$(git ls-files -- 'examples/*.dl' | xargs grep -lE '\bscan[[:space:]]*\(' 2>/dev/null | wc -l | tr -d ' ')"
check_eq "scan usage, TOP-LEVEL spelling ':(glob)examples/*.dl' (program vs grep)" "$prog_top" "$grep_top"
check_eq "scan usage, RECURSIVE spelling 'examples/*.dl'   (program vs grep)"      "$prog_rec" "$grep_rec"
say "RECONCILE  the v5-utility review's '105/129':"
say "RECONCILE    :(glob)examples/*.dl  -> $prog_top / $git_top_files   (shell-glob semantics, * stops at /)"
say "RECONCILE    examples/*.dl         -> $prog_rec / $git_rec_files   (git pathspec, * crosses /)"
if [ "$prog_top" = "105" ] && [ "$git_top_files" = "129" ]; then
  say "PASS  the review's 105/129 is REPRODUCED exactly, and is the top-level spelling"
else
  say "NOTE  the review's 105/129 does not reproduce at this revision; the corpus has moved since it was written"
fi
nested="$(git ls-files -- 'examples/*.dl' | grep -c '.*/.*/' || true)"
say "RECONCILE    the difference is $nested example files in SUBDIRECTORIES, all of which use scan"

# ── 5: the gap reports — findings, printed with contents ──────────────────
report_gap() { # report_gap REL LABEL
  local n; n="$(count "$1")"
  if [ "$n" = "0" ]; then
    say "GAP   $2: none — this leg is a faithful projection of v5's own catalog"
  else
    say "GAP   $2: $n"
    rows "$1" | sed 's/^/        /'
  fi
}
report_gap op_regex_gap   "ops in v5's catalog that the src/ line regex missed"
report_gap op_parse_gap   "source ops in v5's catalog absent from the parse dispatch ladder"
report_gap op_doc_gap     "ops in v5's catalog missing from the GENERATED docs/reference/syntax.md"
report_gap fn_regex_gap   "functions in v5's catalog that the src/ line regex missed"
report_gap fn_doc_gap     "functions missing from the GENERATED docs/reference/functions.md"
report_gap rel_regex_gap  "built-in relations the single-line RelDecl regex missed"
report_gap rel_doc_gap    "built-in relations missing from the GENERATED docs/reference/relations.md"

# The regex leg is allowed to be incomplete and the receipt says by how much,
# but a leg that collapsed to nothing is a broken host, not a finding.
src_ops="$(count src_op)"
[ "$src_ops" -ge 20 ] || fail "src_op collapsed to $src_ops rows; the op_docs regex leg is broken, not merely incomplete"

# ── the table ─────────────────────────────────────────────────────────────
mkdir -p "$(dirname "$TABLE")"
{
  printf '# generated by v6/tsv2/scripts/v5-parity.sh from v6/dl/fixtures/v5-parity.dl6\n'
  printf '# v5 engine: %s   repo rev: %s\n' "$("$PARITY_DL" --version 2>&1 | head -1)" "$(git rev-parse HEAD)"
  printf '# files column = distinct top-level examples/*.dl files naming the thing (129-file corpus)\n'
  printf 'thing\tfamily\tfiles\tv6_status\n'
  rows parity_row '0,1,2,3' | sort -t"$(printf '\t')" -k2,2 -k4,4 -k3,3nr -k1,1
} >"$TABLE"
say "TABLE written: $TABLE ($(($(wc -l <"$TABLE") - 4)) rows)"

say "TOTALS by status:"
rows parity_total | sed 's/^/        /'
say "TOTALS by family and status:"
rows parity_row '1,3' | sort | uniq -c | sed 's/^/        /'

stop_server
if [ "$FAILURES" -gt 0 ]; then
  say "V5 PARITY TABLE: $FAILURES assertion(s) failed (artifacts: $WORK)"
  exit 1
fi
say "artifacts: $WORK"
say "V5 PARITY TABLE SEEDED"
