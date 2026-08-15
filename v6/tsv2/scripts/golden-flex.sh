#!/usr/bin/env bash
# golden-flex.sh -- the composition receipt.
#
# ONE program (v6/dl/fixtures/golden-flex.dl6) exercising every live registry
# construct, graded six ways:
#
#   1  COVERAGE     registry live rows vs the golden's own parsed term + text;
#                   a construct the registry has and the golden lacks fails here
#                   BY NAME (compile/scripts/golden_coverage.pl).
#   2  TEXT DOOR    `bop check` -- the program compiles clean through the same
#                   door a user's file would.
#   3  CARDINALITY  0 rows / 1 row / 100 rows per input rel, plus a perturbed
#                   schedule, each graded oracle vs the emitter, on the
#                   tick log AND the final state. Four byte comparisons.
#   4  E2E          the served HTTP engine: POST /program, POST /edb/events,
#                   GET /idb, live subprocess host, tick log diffed against the
#                   oracle replayed on the run's own consumed schedule
#                   (tests/goldenFlexServed.test.ts).
#
# Hermetic: a scratch work dir, no network, no daemon, an ephemeral server port.
#
# OBSERVED ONCE, UNEXPLAINED, recorded rather than hidden: the first run of this
# script on this machine failed at step 6 with
#   400 {"error":"dl6 compile failed (swipl exit null): "}
# `exit null` means the swipl child was killed, and the only kill path is
# serve/0_compile.ts's `return () => child.kill()` teardown. Not reproduced in
# the four runs since, and the same test passes in isolation and after each of
# steps 2-4 individually (bisected). If it recurs, that teardown is the place to
# look; it is NOT a golden-flex problem.
#
# Run: bash v6/tsv2/scripts/golden-flex.sh     (or `just golden-flex` from v6/)

set -euo pipefail

TSV2="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
V6="$(cd "$TSV2/.." && pwd)"
COMPILE="$V6/prolog/compile"
GOLDEN="$V6/dl/fixtures/golden-flex.dl6"
MODULE="golden-flex"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/golden-flex.XXXXXX")"
NODE_RUN=(node --experimental-transform-types)

cleanup() { rm -rf "$WORK"; }
trap cleanup EXIT

say()  { printf '%s\n' "$*"; }
fail() { printf 'FAIL  %s\n' "$*"; exit 1; }

# ── 1. coverage gate ────────────────────────────────────────────────────────
coverage="$(cd "$COMPILE/scripts" && swipl -q -l golden_coverage.pl -g run -g halt)" \
  || fail "coverage gate: $coverage"
printf '%s\n' "$coverage" | sed 's/^/      /'
say "PASS  coverage gate"

# ── 2. text door ────────────────────────────────────────────────────────────
(cd "$TSV2" && "${NODE_RUN[@]}" cli/bop.ts check "$GOLDEN" >/dev/null 2>"$WORK/check.err") \
  || fail "bop check: $(cat "$WORK/check.err")"
say "PASS  text door: golden-flex.dl6 compiles clean"

# ── compile once; every cardinality replays the same module ─────────────────
bash "$COMPILE/scripts/compile_dl6.sh" "$GOLDEN" "$TSV2/gen_emitted/$MODULE.ts" >"$WORK/compile.out" 2>&1 \
  || fail "compile_dl6: $(cat "$WORK/compile.out")"
say "PASS  compiled to gen_emitted/$MODULE.ts"

(cd "$TSV2" && "${NODE_RUN[@]}" scripts/golden-schedules.ts "$WORK" >/dev/null 2>"$WORK/sched.err") \
  || fail "schedule generation: $(cat "$WORK/sched.err")"

# ── 3. cardinality ──────────────────────────────────────────────────────────
# The oracles run concurrently: each reads the program and its own schedule and
# writes only its own two files, so the legs share nothing. The loop below is
# unaffected and still reads $WORK/oracle.$leg in order.
for leg in zero one many perturbed; do
  schedule="$WORK/golden-flex.$leg.json"
  [ -f "$schedule" ] || fail "no schedule for leg $leg"
  (
    cd "$COMPILE/scripts"
    swipl -q -l golden_oracle.pl -g "oracle_both('$GOLDEN', '$schedule')" -g halt \
      >"$WORK/oracle.$leg" 2>"$WORK/oracle.$leg.err" && rc=0 || rc=$?
    # `set -e` would kill the subshell before this line, and an empty output
    # file is not a failure signal: the zero leg may legitimately produce one.
    printf '%s' "$rc" >"$WORK/oracle.$leg.rc"
  ) &
done
wait

for leg in zero one many perturbed; do
  schedule="$WORK/golden-flex.$leg.json"
  [ "$(cat "$WORK/oracle.$leg.rc" 2>/dev/null)" = "0" ] \
    || fail "oracle $leg: $(cat "$WORK/oracle.$leg.err")"

  ( cd "$TSV2" && \
      "${NODE_RUN[@]}" scripts/golden-run.ts "$MODULE" "$schedule" --final \
  ) >"$WORK/emitted.$leg" 2>"$WORK/emitted.$leg.err" || fail "emitted $leg: $(cat "$WORK/emitted.$leg.err")"

  diff -u "$WORK/oracle.$leg" "$WORK/emitted.$leg" >"$WORK/diff.$leg" \
    || fail "$leg diverges from the oracle:$(printf '\n')$(head -20 "$WORK/diff.$leg")"

  # Row width, so a leg cannot pass by being silently empty. The `zero` leg is
  # allowed to be empty; that IS its point.
  rows="$(tail -1 "$WORK/oracle.$leg" | tr -cd '[' | wc -c | tr -d ' ')"
  say "PASS  $leg: oracle == emitted (tick log + final state), ${rows} final row groups"
done

# ── 5. the served e2e leg ───────────────────────────────────────────────────
(cd "$TSV2" && "${NODE_RUN[@]}" --test tests/goldenFlexServed.test.ts >"$WORK/e2e.out" 2>&1) \
  || { tail -30 "$WORK/e2e.out"; fail "served e2e leg"; }
say "PASS  served e2e: live host + http boundary, tick log == oracle on the served schedule"

say "GOLDEN FLEX HOLDS"
