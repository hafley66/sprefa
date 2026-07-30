#!/usr/bin/env bash
#
# receipts.sh : time-plane unification lab, the runnable half.
#
#   bash v6/prolog/labs/time_plane/receipts.sh
#
# Exit 0 means every receipt held.
#
# Four legs:
#
#   1. reference-engine baselines (0_receipts.pl, 13 receipts) -- what the
#      SHIPPED engine does, unmodified.
#   2. the PROTOTYPE leg: retention_minus.patch is applied to the real
#      engine.pl and the real 1_incremental.ts, the whole battery is re-run,
#      and the patch is reverted. This is how the blast radius is MEASURED
#      rather than estimated. The patch is reverted on every exit path.
#   3. two-door grading of retention_visible.dl6 under the patch: the
#      reference engine over .dl6 text vs the served tsv2 engine, byte-diffed.
#   4. the metadata/historicization cost measurement (metadata_cost.sh).
#
# Hermetic: SPREFA_CONFIG points at a path that does not exist, DL_NO_DAEMON=1,
# every server is :memory:, every db is under mktemp. Nothing reads or writes
# ~/.local/state and no daemon is contacted.
#
# NORMALIZATION, inherited from the rel-as-stream lab and for the same measured
# reason: tick NUMBERS are not compared, each REL's own signed-row sequence is.
# The served engine and the schedule-fed oracle place drain ticks differently
# (the runtime-bridge arc measured this). Receipt (s) is the proof the
# comparison still discriminates.

set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../../../.." && pwd)"
TSV2="$REPO/v6/tsv2"
ORACLE_DIR="$REPO/v6/prolog/compile/scripts"
PATCH="$HERE/retention_minus.patch"

export SPREFA_CONFIG=/nonexistent/time-plane.toml
export DL_NO_DAEMON=1

PASS=0
FAIL=0
PATCH_APPLIED=0

ok()  { printf 'PASS %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf 'FAIL %s\n' "$1"; FAIL=$((FAIL + 1)); }

command -v swipl   >/dev/null || { echo "swipl is required";   exit 2; }
command -v curl    >/dev/null || { echo "curl is required";    exit 2; }
command -v python3 >/dev/null || { echo "python3 is required"; exit 2; }
command -v sqlite3 >/dev/null || { echo "sqlite3 is required"; exit 2; }

revert_patch() {
  if [ "$PATCH_APPLIED" = "1" ]; then
    ( cd "$REPO" && git apply -R "$PATCH" ) && PATCH_APPLIED=0
  fi
}
trap revert_patch EXIT INT TERM

# ── the per-rel delta sequence, the thing actually compared ──────────────────

read -r -d '' SEQUENCE_PY <<'PY' || true
import json, sys
order, sequences = [], {}
for raw in sys.stdin:
    raw = raw.strip()
    if raw.startswith("data: "):
        raw = raw[6:]
    if not raw:
        continue
    tick = json.loads(raw)
    for rel in sorted(tick.get("deltas", {})):
        delta = tick["deltas"][rel]
        if rel not in sequences:
            order.append(rel)
            sequences[rel] = []
        for row in delta.get("del", []):
            sequences[rel].append("%s - %s" % (rel, json.dumps(row, separators=(",", ":"))))
        for row in delta.get("add", []):
            sequences[rel].append("%s + %s" % (rel, json.dumps(row, separators=(",", ":"))))
for rel in sorted(order):
    for line in sequences[rel]:
        print(line)
PY

sequence() { python3 -c "$SEQUENCE_PY"; }

oracle_log() {
  ( cd "$ORACLE_DIR" && swipl -q -l dl6_oracle.pl -g "oracle('$1','$2')" -g halt )
}

served_log() {
  local program="$1" schedule="$2"
  local scratch port pid capture batch count index
  scratch="$(mktemp -d)"
  ( cd "$TSV2" && node --experimental-transform-types cli/bop.ts \
      serve --port 0 --db ":memory:" >"$scratch/serve.log" 2>&1 ) &
  pid=$!
  port=""
  for _ in $(seq 1 200); do
    port="$(sed -n 's/.*serving on \([0-9]*\).*/\1/p' "$scratch/serve.log" | head -1)"
    [ -n "$port" ] && break
    sleep 0.1
  done
  if [ -z "$port" ]; then
    echo "SERVER NEVER LISTENED" >&2; cat "$scratch/serve.log" >&2
    kill "$pid" 2>/dev/null; rm -rf "$scratch"; return 1
  fi
  local base="http://127.0.0.1:$port" loaded
  loaded="$(curl -s -X POST --data-binary @"$program" "$base/program")"
  case "$loaded" in
    *'"loaded":true'*) : ;;
    *) echo "LOAD FAILED: $loaded" >&2
       kill "$pid" 2>/dev/null; rm -rf "$scratch"; return 1 ;;
  esac
  ( curl -sN "$base/ticks" >"$scratch/ticks" ) &
  capture=$!
  sleep 0.5
  count="$(python3 -c 'import json,sys; print(len(json.load(open(sys.argv[1]))))' "$schedule")"
  for (( index = 0; index < count; index++ )); do
    batch="$(python3 -c 'import json,sys; print(json.dumps(json.load(open(sys.argv[1]))[int(sys.argv[2])]))' "$schedule" "$index")"
    curl -s -X POST -d "{\"batch\":$batch}" "$base/arrivals" >/dev/null
    sleep 0.35
  done
  sleep 0.8
  kill "$capture" 2>/dev/null
  kill "$pid" 2>/dev/null
  wait "$pid" 2>/dev/null
  cat "$scratch/ticks"
  rm -rf "$scratch"
}

two_door() {
  local name="$1" program="$HERE/$2" schedule="$HERE/$3" expect_del="${4:-}"
  local scratch
  scratch="$(mktemp -d)"
  oracle_log "$program" "$schedule" | sequence >"$scratch/oracle"
  served_log "$program" "$schedule" | sequence >"$scratch/served"
  if [ ! -s "$scratch/oracle" ]; then
    bad "$name (oracle produced nothing)"
  elif ! diff -u "$scratch/oracle" "$scratch/served" >"$scratch/diff"; then
    bad "$name (doors disagree)"
    sed -n 1,30p "$scratch/diff"
  elif [ -n "$expect_del" ] && ! grep -qF -- "$expect_del" "$scratch/oracle"; then
    bad "$name (expected line '$expect_del' absent)"
    sed -n 1,20p "$scratch/oracle"
  else
    ok "$name ($(wc -l <"$scratch/oracle" | tr -d ' ') deltas, both doors)"
  fi
  rm -rf "$scratch"
}

# ═══ leg 1: reference-engine baselines, SHIPPED engine ══════════════════════

echo "── leg 1: reference-engine baselines (unmodified engine) ────────────────"
if swipl -q -l "$HERE/0_receipts.pl" -g go -g halt; then
  ok "0_receipts.pl (13 receipts)"
else
  bad "0_receipts.pl"
fi

# ═══ leg 2: the prototype, applied to the real files ════════════════════════

echo
echo "── leg 2: retention_minus prototype, blast radius measured ──────────────"

conformance_count() {
  ( cd "$REPO/v6/prolog/conformance" && swipl -q -l go.pl -g go -g halt 2>&1 ) \
    | grep -c '^PASS'
}
conformance_fails() {
  ( cd "$REPO/v6/prolog/conformance" && swipl -q -l go.pl -g go -g halt 2>&1 ) \
    | grep -c '^FAIL'
}

BASE_PASS="$(conformance_count)"
BASE_FAIL="$(conformance_fails)"

if ( cd "$REPO" && git apply "$PATCH" ); then
  PATCH_APPLIED=1
  ok "retention_minus.patch applies to the shipped tree"
else
  bad "retention_minus.patch does not apply (rebase it)"
  exit 1
fi

PROTO_PASS="$(conformance_count)"
PROTO_FAIL="$(conformance_fails)"

if [ "$PROTO_PASS" = "$BASE_PASS" ] && [ "$PROTO_FAIL" = "0" ] && [ "$BASE_FAIL" = "0" ]; then
  ok "conformance unchanged under the prototype ($BASE_PASS -> $PROTO_PASS, 0 fail)"
else
  bad "conformance moved: ${BASE_PASS}/${BASE_FAIL} -> ${PROTO_PASS}/${PROTO_FAIL}"
fi

# plunit is the compiler-side battery the prototype could plausibly disturb.
if ( cd "$REPO/v6/prolog/compile" && \
     swipl -q -l test/plunit_tests.pl -g run_tests -g halt >/dev/null 2>&1 ); then
  ok "plunit green under the prototype"
else
  bad "plunit red under the prototype"
fi

# ═══ leg 3: two-door grading under the prototype ════════════════════════════

echo
echo "── leg 3: two-door grading under the prototype ──────────────────────────"

# The whole Q3+Q4 claim in one case: keep(count(2)) prunes, the prune is now a
# MINUS on the log rel's own boundary, and finalize/1 over that log rel -- which
# fires nothing at all on the shipped engine (0_receipts.pl T5) -- collects the
# evicted rows one tick later. Graded on both doors.
two_door "(p) retention prune is a visible minus, finalize fires" \
         retention_visible.dl6 retention_visible.schedule.json 'ev - [1,"a"]'

# ═══ leg 4: cost measurement ════════════════════════════════════════════════

echo
echo "── leg 4: metadata plane + historicization cost ─────────────────────────"
if bash "$HERE/metadata_cost.sh"; then
  ok "metadata_cost.sh"
else
  bad "metadata_cost.sh"
fi

# ═══ leg 5: the sabotage receipt ════════════════════════════════════════════
#
# (s) Does the two-door comparison still discriminate? Feed the SAME program a
# schedule whose retention bound is exercised differently and confirm the
# oracle log actually changes. Without this, leg 3 could be passing vacuously.

echo
echo "── leg 5: sabotage ──────────────────────────────────────────────────────"
SABOTAGE_SCRATCH="$(mktemp -d)"
sed 's/keep(count(2))/keep(count(3))/' "$HERE/retention_visible.dl6" \
  >"$SABOTAGE_SCRATCH/wider.dl6"
oracle_log "$HERE/retention_visible.dl6" "$HERE/retention_visible.schedule.json" \
  | sequence >"$SABOTAGE_SCRATCH/tight"
oracle_log "$SABOTAGE_SCRATCH/wider.dl6" "$HERE/retention_visible.schedule.json" \
  | sequence >"$SABOTAGE_SCRATCH/wide"
if diff -q "$SABOTAGE_SCRATCH/tight" "$SABOTAGE_SCRATCH/wide" >/dev/null; then
  bad "(s) widening keep(count) changed nothing -- the grading is vacuous"
else
  ok "(s) widening keep(count(2)->3) changes the graded log (grading discriminates)"
fi
rm -rf "$SABOTAGE_SCRATCH"

revert_patch

echo
echo "════════════════════════════════════════════════════════════════════════"
printf 'time-plane lab: %d PASS %d FAIL\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ] || exit 1
