#!/usr/bin/env bash
# @comment-ok: header banner (new-script doc header; the rail reads it as one run)
#
# dl6 budget gate (failure-modes rail-gap 45).
#
# Runs the grid-only dl6 bench and fails the gate if either measured metric
# breaches its ceiling. Closes the class of correctness-only green gates that
# let a 4.3x perf cliff with identical checksums ride 25 green PRs.
#
# RATCHET LAW: ceilings in budget.json move ONLY DOWN, or with a written
# receipt in the commit message. They never loosen silently.
#
# Metrics come from FACTS.json, written by bench.ts's DL6_BENCH_JSON writer:
#   cases[].fixpointMs  -> ms, the FACTS.md "fixpoint ms" column
#   cases[].peakRssKb   -> KB, the FACTS.md "peak RSS" MB column (div 1024)
# The bench itself runs under run-capped so a hung bench is killed, not waited
# on (failure-modes class 38).
#
# Exit: 0 when every graded metric is within budget; 2 on any breach, with the
# exact measured vs ceiling numbers printed.

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUN_CAPPED="$HERE/../../../tools/run-capped.sh"

. "$RUN_CAPPED"

BUDGET="$HERE/budget.json"
FACTS="$HERE/FACTS.json"

# DL6_BUDGET_SKIP_BENCH=1 skips the bench (does not touch FACTS.json) and
# grades the FACTS file in place. DL6_BUDGET_FACTS points at a specific one.
# Used to feed the gate doctored incident numbers for a fail-pre-fix receipt.
# @comment-ok: env-override prose block
if [ "${DL6_BUDGET_SKIP_BENCH:-0}" != "1" ]; then
  # grid-only default; DL6_BENCH_FULL=1 adds cases outside the budget map, so
  # this gate grades exactly the cell the budget.json declares.
  capped "${DL6_BUDGET_S:-300}" "dl6 budget bench" bash "$HERE/bench.sh"
  run_status=$?
  if [ "$run_status" -ne 0 ]; then
    echo "budget dl6: bench.sh exited $run_status" >&2
    exit "$run_status"
  fi
fi

FACTS="${DL6_BUDGET_FACTS:-$FACTS}"

if [ ! -f "$FACTS" ]; then
  echo "budget dl6: FACTS.json not found at $FACTS" >&2
  exit 2
fi

node -e '
const fs = require("node:fs");
const budget = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
const facts = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
let breached = false;
const lines = [];
for (const c of facts.cases) {
  const b = budget[c.name];
  if (!b) continue;
  const ms = c.fixpointMs;
  const mb = Math.round(c.peakRssKb / 1024);
  const msOk = ms <= b.fixpoint_ms_ceiling;
  const mbOk = mb <= b.peak_rss_mb_ceiling;
  lines.push(`budget ${c.name} fixpoint_ms ${ms} <= ${b.fixpoint_ms_ceiling} ${msOk ? "OK" : "BREACH"}`);
  lines.push(`budget ${c.name} peak_rss_mb ${mb} <= ${b.peak_rss_mb_ceiling} ${mbOk ? "OK" : "BREACH"}`);
  if (!msOk || !mbOk) breached = true;
}
process.stdout.write(lines.join("\n") + "\n");
process.exit(breached ? 2 : 0);
' "$BUDGET" "$FACTS"
