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
#
# Run from v6/tsv2/ (or anywhere; this script cds to its own directory
# first): scripts/sweep.sh
set -euo pipefail
cd "$(dirname "$0")/.."

COMPILE_DIR="../prolog/compile"
COMPILE_OUT="$COMPILE_DIR/out"

echo "=== stage 1: compile sweep ==="
swipl -q -l "$COMPILE_DIR/sweep.pl" -g sweep -g halt

echo ""
echo "=== stage 2: oracle dump ==="
swipl -q -l "$COMPILE_DIR/oracle_dump.pl" -g dump_all -g halt

echo ""
echo "=== stage 3: copy compiled modules into gen_emitted/, run the diff ==="
compiled_names=$(node -e '
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

node --experimental-transform-types scripts/sweep.ts
