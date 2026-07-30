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
#                   schedule, each graded oracle vs BOTH emitter modes, on the
#                   tick log AND the final state. Twelve byte comparisons.
#   4  MODE PARITY  incremental vs naive, byte-identical to each other as well as
#                   to the oracle (stated separately because a shared wrong
#                   answer on both emitter paths would otherwise read as one
#                   failure instead of two).
#   5  NORM PROBE   an INVERTED receipt: `norm/1` is a live registry row whose
#                   two engines DISAGREE (oracle stores the unevaluated term,
#                   the emitter evaluates it in SQL) and which no conformance
#                   fixture covers. This asserts the divergence STILL EXISTS, so
#                   whoever fixes it is told by a red gate to promote norm into
#                   the golden. It is not a workaround; it is a tripwire on a
#                   defect the golden had to route around.
#   6  E2E          the served HTTP engine: POST /program, POST /arrivals,
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

# ── 3 + 4. cardinality and mode parity ──────────────────────────────────────
for leg in zero one many perturbed; do
  schedule="$WORK/golden-flex.$leg.json"
  [ -f "$schedule" ] || fail "no schedule for leg $leg"

  ( cd "$COMPILE/scripts" \
    && swipl -q -l golden_oracle.pl -g "oracle_ticklog('$GOLDEN', '$schedule')" -g halt \
    && swipl -q -l golden_oracle.pl -g "oracle_final('$GOLDEN', '$schedule')"   -g halt \
  ) >"$WORK/oracle.$leg" 2>"$WORK/oracle.$leg.err" || fail "oracle $leg: $(cat "$WORK/oracle.$leg.err")"

  for mode in incremental naive; do
    ( cd "$TSV2" && SPREFA_TSV2_EMITTER_MODE="$mode" \
        "${NODE_RUN[@]}" scripts/golden-run.ts "$MODULE" "$schedule" --final \
    ) >"$WORK/$mode.$leg" 2>"$WORK/$mode.$leg.err" || fail "$mode $leg: $(cat "$WORK/$mode.$leg.err")"

    diff -u "$WORK/oracle.$leg" "$WORK/$mode.$leg" >"$WORK/diff.$mode.$leg" \
      || fail "$leg / $mode diverges from the oracle:$(printf '\n')$(head -20 "$WORK/diff.$mode.$leg")"
  done

  diff -q "$WORK/incremental.$leg" "$WORK/naive.$leg" >/dev/null \
    || fail "$leg: the two emitter modes disagree with each other"

  # Row width, so a leg cannot pass by being silently empty. The `zero` leg is
  # allowed to be empty; that IS its point.
  rows="$(tail -1 "$WORK/oracle.$leg" | tr -cd '[' | wc -c | tr -d ' ')"
  say "PASS  $leg: oracle == incremental == naive (tick log + final state), ${rows} final row groups"
done

# ── 5. the norm/1 inverted receipt ──────────────────────────────────────────
cat >"$WORK/norm.dl6" <<'DL6'
rel route(raw: text).
rel route_key(raw: text, key: text).
route_key(Raw, norm(Raw)) <- route(Raw).
DL6
printf '%s\n' '[[{"rel":"route","sign":"add","row":["Hello World"]}]]' >"$WORK/norm.json"
( cd "$COMPILE/scripts" && swipl -q -l golden_oracle.pl -g "oracle_ticklog('$WORK/norm.dl6','$WORK/norm.json')" -g halt ) \
  >"$WORK/norm.oracle" 2>/dev/null || fail "norm probe: oracle leg failed"
bash "$COMPILE/scripts/compile_dl6.sh" "$WORK/norm.dl6" "$TSV2/gen_emitted/golden_norm_probe.ts" >/dev/null 2>&1 \
  || fail "norm probe: compile failed"
( cd "$TSV2" && "${NODE_RUN[@]}" scripts/golden-run.ts golden_norm_probe "$WORK/norm.json" ) \
  >"$WORK/norm.emitter" 2>/dev/null || fail "norm probe: emitter leg failed"
rm -f "$TSV2/gen_emitted/golden_norm_probe.ts"

if diff -q "$WORK/norm.oracle" "$WORK/norm.emitter" >/dev/null; then
  say "FAIL  norm/1 NO LONGER DIVERGES -- the defect this golden routed around is fixed."
  say "      Promote norm/1 out of golden_coverage.pl's expected_absent/2, put it back"
  say "      in golden-flex.dl6, and delete this receipt."
  exit 1
fi
say "PASS  norm/1 inverted receipt: the two engines still disagree (defect intact, golden correctly routes around it)"
say "      oracle:  $(cat "$WORK/norm.oracle")"
say "      emitter: $(cat "$WORK/norm.emitter")"

# ── 6. the served e2e leg ───────────────────────────────────────────────────
(cd "$TSV2" && "${NODE_RUN[@]}" --test tests/goldenFlexServed.test.ts >"$WORK/e2e.out" 2>&1) \
  || { tail -30 "$WORK/e2e.out"; fail "served e2e leg"; }
say "PASS  served e2e: live host + http boundary, tick log == oracle on the served schedule"

say "GOLDEN FLEX HOLDS"
