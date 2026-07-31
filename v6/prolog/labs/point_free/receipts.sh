#!/usr/bin/env bash
#
# receipts.sh : point-free lab, the runnable half.
#
#   bash v6/prolog/labs/point_free/receipts.sh
#
# Exit 0 means every receipt held. Nonzero means at least one did not.
#
# Six legs:
#
#   1. SUGAR VS TODAY. Each sugar file is expanded by expand.pl, printed to
#      `.dl6` by the SHIPPED printer, and its tick log is diffed against a
#      today-spelling program written by hand and independently (different
#      variable names, different arm order, literal seeds). Byte identity is
#      the grading law; a difference REFUTES the move.
#   2. THE M3 NORMALIZATION, and its proof. `|>` mints rels the today spelling
#      names differently, so only the SHARED rels can be compared. Receipt (S2)
#      shows that comparison still catches a wrong answer.
#   3. TWO-DOOR. The emitted programs run on the served tsv2 engine and are
#      compared to the oracle per rel, the same normalization and the same
#      justification as csp_idioms/receipts.sh.
#   4. BREAK RULES. Each break/ sugar file earns its named refusal, and the
#      skipped-refusal expansion is shown producing a DIFFERENT answer than the
#      program it claims to be sugar for.
#   5. SURFACE PROBES. Three measurements about spellings, not semantics.
#   6. SABOTAGE.
#
# Hermetic: SPREFA_CONFIG points at a path that does not exist, DL_NO_DAEMON=1,
# every server is :memory: on an ephemeral port. Nothing reads or writes
# ~/.local/state and no daemon is spoken to.

set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../../../.." && pwd)"
TSV2="$REPO/v6/tsv2"
ORACLE_DIR="$REPO/v6/prolog/compile/scripts"

export SPREFA_CONFIG=/nonexistent/point-free.toml
export DL_NO_DAEMON=1

PASS=0
FAIL=0
ok()  { printf 'PASS %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf 'FAIL %s\n' "$1"; FAIL=$((FAIL + 1)); }

command -v swipl   >/dev/null || { echo "swipl is required";   exit 2; }
command -v python3 >/dev/null || { echo "python3 is required"; exit 2; }
command -v curl    >/dev/null || { echo "curl is required";    exit 2; }

# ── the per-rel delta sequence (shared with csp_idioms/receipts.sh) ──────────

read -r -d '' SEQUENCE_PY <<'PY' || true
import json, sys
keep = sys.argv[1:] if len(sys.argv) > 1 else None
order, sequences = [], {}
for raw in sys.stdin:
    raw = raw.strip()
    if raw.startswith("data: "):
        raw = raw[6:]
    if not raw:
        continue
    tick = json.loads(raw)
    for rel in sorted(tick.get("deltas", {})):
        if keep and rel not in keep:
            continue
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

sequence() { python3 -c "$SEQUENCE_PY" "$@"; }

oracle_log() {
  ( cd "$ORACLE_DIR" && swipl -q -l dl6_oracle.pl \
      -g "oracle('$1','$2')" -g halt 2>/dev/null )
}

emit_sugar() {
  ( cd "$HERE" && swipl -q -l emit.pl -g "emit('sugar/$1.sugar.pl','out/$1.dl6')" -g halt 2>&1 )
}

refusal_of() {
  ( cd "$HERE" && swipl -q -l emit.pl -g "show_refusal('break/$1.sugar.pl')" -g halt 2>&1 | tail -1 )
}

check_out() {
  ( cd "$TSV2" && npm run --silent bop -- check "$1" 2>&1 )
}

echo "== leg 1: sugar expanded, then diffed against the today spelling =========="

sugar_matches_today() {
  local name="$1"
  local scratch problems
  scratch="$(mktemp -d)"
  problems="$(emit_sugar "$name")"
  if [ -n "$problems" ]; then
    bad "(1) $name -- expansion reported: $problems"
    rm -rf "$scratch"
    return
  fi
  oracle_log "$HERE/today/$name.dl6"  "$HERE/today/$name.schedule.json" | sequence >"$scratch/today"
  oracle_log "$HERE/out/$name.dl6"    "$HERE/today/$name.schedule.json" | sequence >"$scratch/sugar"
  if [ ! -s "$scratch/today" ]; then
    bad "(1) $name (today spelling produced nothing)"
  elif diff -u "$scratch/today" "$scratch/sugar" >"$scratch/diff"; then
    ok "(1) $name -- $(wc -l <"$scratch/today" | tr -d ' ') deltas, sugar == today, byte for byte"
  else
    bad "(1) $name -- SUGAR DIVERGES FROM TODAY"
    sed -n 1,30p "$scratch/diff"
  fi
  rm -rf "$scratch"
}

sugar_matches_today counter
sugar_matches_today running_average
sugar_matches_today retry_backoff
sugar_matches_today buffer_count

echo
echo "== leg 2: M3, the shared-rel comparison and why it is not free ==========="

# `|>` mints `stage_alert_2_1` / `stage_alert_2_2` where the today spelling
# names `scaled` / `shifted`. Those rels are in the tick log on both sides with
# the same rows under different names, so the comparison is over the rels the
# two programs SHARE. Stating it is not enough -- (S2) below sabotages the
# expansion and shows the shared-rel comparison goes red.
m3_receipt() {
  local scratch
  scratch="$(mktemp -d)"
  emit_sugar sensor_pipeline >/dev/null
  oracle_log "$HERE/today/sensor_pipeline.dl6" "$HERE/today/sensor_pipeline.schedule.json" \
    | sequence reading alert >"$scratch/today"
  oracle_log "$HERE/out/sensor_pipeline.dl6" "$HERE/today/sensor_pipeline.schedule.json" \
    | sequence reading alert >"$scratch/sugar"
  if [ -s "$scratch/today" ] && diff -u "$scratch/today" "$scratch/sugar" >"$scratch/diff"; then
    ok "(2a) sensor_pipeline -- reading+alert identical, same ticks, no latency added"
  else
    bad "(2a) sensor_pipeline shared rels diverge"
    sed -n 1,20p "$scratch/diff"
  fi
  # The seam rows are equal too; only the NAMES differ. Pinned so that a change
  # in what the stages compute is caught even though the names never match.
  local today_seam sugar_seam
  today_seam="$(oracle_log "$HERE/today/sensor_pipeline.dl6" "$HERE/today/sensor_pipeline.schedule.json" \
    | sequence scaled shifted | sed 's/^[a-z_0-9]* //')"
  sugar_seam="$(oracle_log "$HERE/out/sensor_pipeline.dl6" "$HERE/today/sensor_pipeline.schedule.json" \
    | sequence stage_alert_2_1 stage_alert_2_2 | sed 's/^[a-z_0-9]* //')"
  if [ -n "$today_seam" ] && [ "$today_seam" = "$sugar_seam" ]; then
    ok "(2b) the two minted seam rels carry exactly the today rels' rows, name aside"
  else
    bad "(2b) minted seam rows differ from the named seam rows"
  fi
  rm -rf "$scratch"
}
m3_receipt

# M2's minted cursor is NOT boundary-invisible: it is an ordinary rel and it
# appears in the tick log. The leg-1 buffer_count receipt only passes because
# the today spelling was made to name its cursor the same thing. Renaming it
# makes the logs differ, which is the cost card 1b predicted, measured.
minted_visibility_receipt() {
  local scratch
  scratch="$(mktemp -d)"
  sed 's/seq_numbered_1/cursor/g' "$HERE/today/buffer_count.dl6" >"$scratch/renamed.dl6"
  oracle_log "$scratch/renamed.dl6" "$HERE/today/buffer_count.schedule.json" | sequence >"$scratch/renamed"
  oracle_log "$HERE/out/buffer_count.dl6" "$HERE/today/buffer_count.schedule.json" | sequence >"$scratch/sugar"
  if [ -s "$scratch/renamed" ] && ! diff -q "$scratch/renamed" "$scratch/sugar" >/dev/null; then
    ok "(2c) M2's minted cursor IS in the tick log: renaming it alone breaks byte identity"
  else
    bad "(2c) the minted cursor turned out invisible -- re-grade card 1b's cost"
  fi
  rm -rf "$scratch"
}
minted_visibility_receipt

echo
echo "== leg 3: two-door, the emitted programs on the served engine ============"

served_log() {
  local program="$1" schedule="$2"
  local scratch port pid capture batch count index
  scratch="$(mktemp -d)"
  ( cd "$TSV2" && node --experimental-transform-types cli/bop.ts \
      serve --port 0 --db ':memory:' >"$scratch/serve.log" 2>&1 ) &
  pid=$!
  port=""
  for _ in $(seq 1 200); do
    port="$(sed -n 's/.*serving on \([0-9]*\).*/\1/p' "$scratch/serve.log" | head -1)"
    [ -n "$port" ] && break
    sleep 0.1
  done
  if [ -z "$port" ]; then
    echo "SERVER NEVER LISTENED" >&2
    kill "$pid" 2>/dev/null; rm -rf "$scratch"; return 1
  fi
  local base="http://127.0.0.1:$port" loaded
  loaded="$(curl -s -X POST --data-binary @"$program" "$base/program")"
  case "$loaded" in
    *'"loaded":true'*) : ;;
    *) echo "LOAD FAILED: $loaded" >&2; kill "$pid" 2>/dev/null; rm -rf "$scratch"; return 1 ;;
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
  local name="$1"
  local scratch
  scratch="$(mktemp -d)"
  oracle_log "$HERE/out/$name.dl6" "$HERE/today/$name.schedule.json" | sequence >"$scratch/oracle"
  served_log "$HERE/out/$name.dl6" "$HERE/today/$name.schedule.json" | sequence >"$scratch/served"
  if [ ! -s "$scratch/oracle" ]; then
    bad "(3) $name (oracle produced nothing)"
  elif diff -u "$scratch/oracle" "$scratch/served" >"$scratch/diff"; then
    ok "(3) $name -- $(wc -l <"$scratch/oracle" | tr -d ' ') deltas, both doors"
  else
    bad "(3) $name -- the two engines disagree on the expansion"
    sed -n 1,30p "$scratch/diff"
  fi
  rm -rf "$scratch"
}

two_door counter
two_door buffer_count
two_door sensor_pipeline

echo
echo "== leg 4: break rules ===================================================="

expect_refusal() {
  local name="$1" want="$2"
  local got
  got="$(refusal_of "$name")"
  if [ "$got" = "$want" ]; then
    ok "(4) $name -> $want"
  else
    bad "(4) $name -> got '$got', wanted '$want'"
  fi
}

expect_refusal scan_on_log_head   'scan_head_not_keyed_on_group(trail/2,[1])'
expect_refusal scan_in_level      'scan_in_level_rule'
expect_refusal pipe_in_edge       'pipe_in_edge_rule'
expect_refusal pipe_aggregate_head 'pipe_head_is_aggregate(count/1)'

# (4a) M1 on a log head does not merely differ -- the fold FORKS, because
#      `pre` over a log rel matches every accumulated row. One `hit` produces
#      two `trail` rows at tick 3. Pinned as the wrong behaviour the refusal
#      exists to stop.
if oracle_log "$HERE/break/scan_on_log_head_unsafe.dl6" "$HERE/break/scan_on_log_head.schedule.json" \
     | sequence trail | grep -qxF 'trail + ["home",5]' \
&& oracle_log "$HERE/break/scan_on_log_head_unsafe.dl6" "$HERE/break/scan_on_log_head.schedule.json" \
     | sequence trail | grep -qxF 'trail + ["home",7]'; then
  ok '(4a) unrefused log-headed fold FORKS: one hit, two trail rows (WRONG, pinned)'
else
  bad '(4a) the forking receipt no longer reproduces -- re-grade break rule M1-1'
fi

# (4b) `|>` in an edge rule whose head is a LOG rel is caught by an existing
#      named refusal, loudly, on both doors.
if check_out "$HERE/break/pipe_in_edge_unsafe.dl6" | grep -q 'log_on_level_headed_rel(logged/2)'; then
  ok '(4b) the level-cut expansion of an edge pipe hits log_on_level_headed_rel (loud)'
else
  bad '(4b) the loud half of break rule M3-1 no longer reproduces'
fi

# (4c) The SILENT half. Same expansion with a keyed head loads clean on both
#      doors and gives a different answer: the level version RETRACTS when the
#      trigger row departs, the edge version does not.
silent_receipt() {
  local scratch
  scratch="$(mktemp -d)"
  oracle_log "$HERE/break/pipe_edge_silent_today.dl6"  "$HERE/break/pipe_in_edge.schedule.json" \
    | sequence seen >"$scratch/edge"
  oracle_log "$HERE/break/pipe_edge_silent_unsafe.dl6" "$HERE/break/pipe_in_edge.schedule.json" \
    | sequence seen >"$scratch/level"
  if [ -s "$scratch/edge" ] && [ -s "$scratch/level" ] \
     && ! diff -q "$scratch/edge" "$scratch/level" >/dev/null \
     && grep -qxF 'seen - ["cli",9]' "$scratch/level" \
     && ! grep -qxF 'seen - ["cli",9]' "$scratch/edge"; then
    ok '(4c) silent half: the level-cut expansion RETRACTS the head, the edge rule does not'
  else
    bad '(4c) the silent divergence no longer reproduces -- re-grade break rule M3-1'
  fi
  rm -rf "$scratch"
}
silent_receipt

echo
echo "== leg 5: surface probes ================================================="

# (5a) The engine's own minting convention is not writable. `__`-prefixed rel
#      names do not parse, so any hand-written desugar must use a legal name --
#      which is what slot_stage_naming has to answer.
if check_out "$HERE/probe/underscore_rel.dl6" | grep -q 'broken: parse_failed'; then
  ok '(5a) a `__`-prefixed rel name does not parse: minted names are term-plane only'
else
  bad '(5a) `__` rel names now parse -- re-grade slot_stage_naming'
fi

# (5b) and (5c) Both candidate glyphs are unclaimed, and both fail the same
#      unreadable way (the csp lab's finding E2a, in the wild again).
if check_out "$HERE/probe/pipe_glyph.dl6" | grep -q 'dl_parse_error(statement,\[[0-9]'; then
  ok '(5b) `|>` is unclaimed today; the failure is the char-code dump (E2a class)'
else
  bad '(5b) `|>` behaves differently now -- re-grade slot_pipe_word'
fi

if check_out "$HERE/probe/head_last.dl6" | grep -q 'dl_parse_error(statement,\[[0-9]'; then
  ok '(5c) `|->` outside a match block is unclaimed; same char-code dump'
else
  bad '(5c) head-last `|->` behaves differently now -- re-grade Q4'
fi

echo
echo "== leg 6: sabotage, proving the comparisons discriminate ================="

# (S1) Leg 1 compares whole tick logs. One changed seed in the counter sugar
#      must make it go red.
s1_receipt() {
  local scratch
  scratch="$(mktemp -d)"
  sed 's/scan(Prev, 0, Prev + Delta)/scan(Prev, 1, Prev + Delta)/' \
      "$HERE/sugar/counter.sugar.pl" >"$scratch/sabotaged.sugar.pl"
  ( cd "$HERE" && swipl -q -l emit.pl \
      -g "emit('$scratch/sabotaged.sugar.pl','$scratch/sabotaged.dl6')" -g halt >/dev/null 2>&1 )
  oracle_log "$HERE/today/counter.dl6"  "$HERE/today/counter.schedule.json" | sequence >"$scratch/good"
  oracle_log "$scratch/sabotaged.dl6"   "$HERE/today/counter.schedule.json" | sequence >"$scratch/bad"
  if [ -s "$scratch/bad" ] && ! diff -q "$scratch/good" "$scratch/bad" >/dev/null; then
    ok '(S1) sabotage: one changed scan seed makes leg 1 differ'
  else
    bad '(S1) sabotage did not register -- leg 1 is not discriminating'
  fi
  rm -rf "$scratch"
}
s1_receipt

# (S2) Leg 2 compares only the rels the two programs share, which is a real
#      weakening. One changed multiplier inside a MINTED stage must still make
#      the shared-rel comparison go red.
s2_receipt() {
  local scratch
  scratch="$(mktemp -d)"
  emit_sugar sensor_pipeline >/dev/null
  sed 's/Doubled := Raw \* 2/Doubled := Raw * 5/' "$HERE/out/sensor_pipeline.dl6" >"$scratch/sabotaged.dl6"
  oracle_log "$HERE/today/sensor_pipeline.dl6" "$HERE/today/sensor_pipeline.schedule.json" \
    | sequence reading alert >"$scratch/good"
  oracle_log "$scratch/sabotaged.dl6" "$HERE/today/sensor_pipeline.schedule.json" \
    | sequence reading alert >"$scratch/bad"
  if [ -s "$scratch/bad" ] && ! diff -q "$scratch/good" "$scratch/bad" >/dev/null; then
    ok '(S2) sabotage: a wrong multiplier inside a minted stage still reaches the shared rels'
  else
    bad '(S2) the shared-rel comparison is not discriminating -- leg 2 proves nothing'
  fi
  rm -rf "$scratch"
}
s2_receipt

echo
printf '%s PASS %s FAIL\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
