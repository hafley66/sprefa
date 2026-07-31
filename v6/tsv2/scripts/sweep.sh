#!/usr/bin/env bash
# sweep.sh — Phase C driver (plans/2026-07-27-tsv2-compile-target-header.md,
# PHASE C CONTRACT). Runs the full fixture-corpus sweep in three stages:
#   1. v6/prolog/compile/sweep.pl: compiles every conformance/fixtures/*.pl
#      fixture, writes v6/prolog/compile/out/manifest.json (per-fixture
#      compiled|unsupported|crash bucket) plus, for every compiled fixture,
#      out/<name>.ts and out/<name>.schedule.json.
#   2. v6/prolog/compile/oracle_dump.pl: runs conformance/ticklog.pl
#      (unedited) over every fixture's OWN schedule, writing
#      out/<name>.oracle.jsonl (or printing ORACLE_THROW for the handful of
#      fixtures that test an engine rejection path).
#   3. copies every compiled fixture's emitted module into gen_emitted/ (so
#      its "../runtime/..." relative imports resolve inside this package),
#      then runs scripts/sweep.ts, which replays each schedule against the
#      emitted module and diffs the tick log against the oracle log.
#   4. scripts/manifest-reason-diff.ts: diffs the freshly written manifest's
#      refusal REASONS against git HEAD's copy. Stages 1-3 are all bucket/count
#      gates, so a fixture that stays `unsupported` while changing WHICH refusal
#      it hits moves no number any of them read. Informational (exit 0) unless
#      MANIFEST_DIFF_STRICT=1.
#
# Run from v6/tsv2/ (or anywhere; this script cds to its own directory
# first): scripts/sweep.sh
#
# BUDGETS (timeout-gun lane, 2026-07-31). Four legs, four separate caps, so a
# failure names the stage instead of the script. Measured walls are recorded
# against each `capped` call below; every default is at least 10x the measured
# leg and never under 300s, because a stage that takes seconds today has no
# meaningful multiple and the honest claim is "still running after N minutes
# means stuck". Override per leg with SWEEP_COMPILE_BUDGET_S,
# SWEEP_ORACLE_BUDGET_S, SWEEP_DIFF_BUDGET_S, SWEEP_REASON_BUDGET_S.
set -euo pipefail
. "$(cd "$(dirname "$0")/../.." && pwd)/tools/run-capped.sh"
cd "$(dirname "$0")/.."

COMPILE_DIR="../prolog/compile"
COMPILE_OUT="$COMPILE_DIR/out"

echo "=== stage 1: compile sweep ==="
# gc(false): swipl 10.0.2 aborts this batch under -g with
#   "system error: Mismatch in up phase" (GC compaction bug, deterministic
#   at the 88th collection once the corpus reached 163 fixtures; the same
#   goal typed at the toplevel completes). One-shot process, peak RSS is
#   tens of MB without GC, so disabling it is free. Receipt 2026-07-29.
# Budget: the whole four-stage sweep measures 8.3s on this machine (196
# fixtures) and this is its largest leg. 900s is ~100x the whole run, chosen
# because a compile sweep that has not finished in fifteen minutes has hit a
# cliff, not a corpus -- and because the leg's honest wall is small enough that
# a tight multiple of it would be measuring machine load rather than the sweep.
capped "${SWEEP_COMPILE_BUDGET_S:-900}" "stage 1 compile sweep" \
  swipl -q -l "$COMPILE_DIR/sweep.pl" -g 'set_prolog_flag(gc,false), sweep' -g halt

echo ""
echo "=== stage 2: oracle dump ==="
capped "${SWEEP_ORACLE_BUDGET_S:-900}" "stage 2 oracle dump" \
  swipl -q -l "$COMPILE_DIR/oracle_dump.pl" -g dump_all -g halt

echo ""
echo "=== stage 3: copy compiled modules into gen_emitted/, run the diff ==="
compiled_names=$(capped "${SWEEP_MANIFEST_BUDGET_S:-300}" "stage 3 manifest read" node -e '
  const fs = require("node:fs");
  const manifest = JSON.parse(fs.readFileSync("'"$COMPILE_OUT"'/manifest.json", "utf8"));
  for (const entry of manifest) if (entry.bucket === "compiled") console.log(entry.name);
')
# gen_emitted/ can also contain checked-in modules that are not fixture
# outputs. Remove only the fixture module immediately before rewriting it.
while IFS= read -r name; do
  [ -z "$name" ] && continue
  rm -f "gen_emitted/$name.ts"
  cp -f "$COMPILE_OUT/$name.ts" "gen_emitted/$name.ts"
done <<< "$compiled_names"

# The replay leg: 196 emitted modules each replayed against their schedule,
# inside the 8.3s the whole sweep costs. Budget 900s.
capped "${SWEEP_DIFF_BUDGET_S:-900}" "stage 3 emitted-vs-oracle replay" \
  node --experimental-transform-types scripts/sweep.ts

echo ""
echo "=== stage 4: refusal-reason diff vs HEAD (informational) ==="
# Stage 1 just rewrote the manifest. Every gate above this line reads BUCKETS;
# this reads REASONS, so a fixture that stays `unsupported` while switching
# WHICH refusal it hits stops being invisible. Informational by default (exit 0
# even on movement) so regen-with-intent stays one command; set
# MANIFEST_DIFF_STRICT=1 to make an unexplained restatement fail the run.
# `env` rather than an assignment prefix: `capped` is a shell function, and a
# variable assignment in front of a function call is scoped to the shell, not
# cleanly to the command the function eventually execs.
capped "${SWEEP_REASON_BUDGET_S:-300}" "stage 4 refusal-reason diff" \
  env "MANIFEST_DIFF_STRICT=${MANIFEST_DIFF_STRICT:-0}" \
  node --experimental-transform-types scripts/manifest-reason-diff.ts
