#!/usr/bin/env bash
# sweep.sh — Phase C driver (plans/2026-07-27-tsv2-compile-target-header.md,
# PHASE C CONTRACT). Runs the full fixture-corpus sweep in three stages:
#   1. v6/prolog/compile/sweep.pl: compiles every conformance/fixtures/*.pl
#      fixture, writes v6/prolog/compile/out/manifest.json (per-fixture
#      compiled|unsupported|crash bucket) plus, for every compiled fixture,
#      out/<name>.ts and out/<name>.schedule.json. Fanned out across
#      SWEEP_JOBS worker processes by scripts/sweep-stage1.sh, and skipping
#      any fixture whose digest is already in out/sweep.digests.
#   2. v6/prolog/compile/oracle_dump.pl: runs conformance/ticklog.pl
#      (unedited) over every fixture's OWN schedule, writing
#      out/<name>.oracle.jsonl (or printing ORACLE_THROW for the handful of
#      fixtures that test an engine rejection path). The jsonl files are
#      FROZEN SNAPSHOTS (conformance/rulings.pl, oracle_demoted_to_snapshots),
#      so a fixture is re-dumped only when its own program/initial/schedule or
#      the oracle engine under it changed.
#   3. copies every compiled fixture's emitted module into gen_emitted/ (so
#      its "../runtime/..." relative imports resolve inside this package),
#      then runs scripts/sweep.ts, which replays each schedule against the
#      emitted module and diffs the tick log against the oracle log. One node
#      process for the whole corpus, and a fixture is replayed only when its
#      emitted module, its schedule, either snapshot, or the runtime changed.
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
# meaningful multiple and the accurate claim is "still running after N minutes
# means stuck". Override per leg with SWEEP_COMPILE_BUDGET_S,
# SWEEP_ORACLE_BUDGET_S, SWEEP_DIFF_BUDGET_S, SWEEP_REASON_BUDGET_S.
#
# KNOBS (stage 1 only):
#   SWEEP_JOBS=<n>   worker processes. Default is the machine's performance
#                    core count. 1 runs the old single-process shape.
#   SWEEP_FORCE=1    spend every cache: drop the compiled outputs and the
#                    compile digest store, re-dump every oracle snapshot, and
#                    replay every fixture. Reaches all three stages.
# Stages 2-4 stay in one process each: stage 2 is one `dump_all` goal inside
# compile/oracle_dump.pl and stage 3 one `node scripts/sweep.ts` run over the
# whole corpus. Their cost is now paid per CHANGED fixture, so sharding them
# would buy the forced pass alone.
#
# TIMINGS. Each stage appends `fixture<TAB>stage<TAB>ms` rows for the work it
# actually did to out/sweep.timings.tsv (gitignored) and prints its own slowest
# ten. A cached pass writes no rows and says so.
set -euo pipefail
. "$(cd "$(dirname "$0")/../.." && pwd)/tools/run-capped.sh"
cd "$(dirname "$0")/.."

COMPILE_DIR="../prolog/compile"
COMPILE_OUT="$COMPILE_DIR/out"

# One pass, one ledger. Truncated here and appended to by every stage, so the
# rows are this pass's work and not an accumulation across runs.
mkdir -p "$COMPILE_OUT"
printf 'fixture\tstage\tms\n' > "$COMPILE_OUT/sweep.timings.tsv"

# Performance cores, not logical: the sweep is compute-bound prolog and the
# efficiency cores finish their slice long after the others. `sysctl` is the
# macOS spelling, `getconf` the portable fallback.
sweep_default_jobs() {
  local count
  count="$(sysctl -n hw.perflevel0.logicalcpu 2>/dev/null || true)"
  [ -n "$count" ] || count="$(getconf _NPROCESSORS_ONLN 2>/dev/null || true)"
  [ -n "$count" ] || count=4
  printf '%s' "$count"
}
SWEEP_JOBS="${SWEEP_JOBS:-$(sweep_default_jobs)}"

echo "=== stage 1: compile sweep (jobs=$SWEEP_JOBS) ==="
# GC stays ON. The corpus outgrew the `set_prolog_flag(gc,false)` workaround
# this stage carried for the swipl 10.0.2 "Mismatch in up phase" compaction
# abort: at 462 fixtures the collector-free heap never stops growing and the
# stage runs past its whole 900s budget, where with GC on it completes in ~75s
# at ~2.4 GB peak. The abort itself no longer reproduces on this corpus.
# Sharding cuts the peak with it: a worker only ever holds its own slice.
# Budget: 900s is ~12x the measured single-process wall; a compile sweep that
# has not finished in fifteen minutes has hit a cliff, not a corpus.
# DL6_DEBUG (comma topic list, or `all`) turns on the compiler's library(debug)
# topics for this stage; compile_messages:dl6_debug_from_env/0 is the one parser
# and sweep-stage1.sh forwards the variable to every worker.
capped "${SWEEP_COMPILE_BUDGET_S:-900}" "stage 1 compile sweep" \
  env "SWEEP_JOBS=$SWEEP_JOBS" "SWEEP_FORCE=${SWEEP_FORCE:-0}" "DL6_DEBUG=${DL6_DEBUG:-}" \
  bash scripts/sweep-stage1.sh "$SWEEP_JOBS"

echo ""
echo "=== stage 2: oracle dump ==="
# DEFAULT OFF (rulings.pl oracle_demoted_to_snapshots, 2026-08-20 amendment):
# SWEEP_ORACLE=1 mints or refreshes snapshots; the default diffs frozen ones.
if [ "${SWEEP_ORACLE:-0}" = "1" ]; then
  capped "${SWEEP_ORACLE_BUDGET_S:-900}" "stage 2 oracle dump" \
    env "SWEEP_FORCE=${SWEEP_FORCE:-0}" \
    swipl -q -l "$COMPILE_DIR/oracle_dump.pl" -g dump_all -g halt
else
  newest=$(ls -t "$COMPILE_DIR/out"/*.oracle.jsonl 2>/dev/null | sed -n 1p) || true
  stamp=$(stat -c %y "$newest" 2>/dev/null || stat -f %Sm "$newest" 2>/dev/null || echo none)
  echo "oracle=off snapshots=${stamp%% *} ($(basename "${newest:-none}"))"
  missing=0
  while IFS= read -r name; do
    if [ ! -f "$COMPILE_DIR/out/$name.oracle.jsonl" ] && [ ! -f "$COMPILE_DIR/out/$name.oracle.throw" ]; then
      echo "SNAPSHOT MISSING $name: mint it with SWEEP_ORACLE=1"
      missing=1
    fi
  done < <(node -e '
    const m = require(process.argv[1]);
    for (const row of m) if (row.bucket === "compiled") console.log(row.name);
  ' "$COMPILE_DIR/out/manifest.json")
  if [ "$missing" = "1" ]; then
    echo "stage 2 FAILED: compiled fixtures lack oracle snapshots"
    exit 1
  fi
fi

echo ""
echo "=== stage 3: copy compiled modules into gen_emitted/, run the diff ==="
mkdir -p gen_emitted
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
  env "SWEEP_FORCE=${SWEEP_FORCE:-0}" \
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
