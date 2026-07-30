#!/usr/bin/env bash
# run-all.sh — every receipt behind plans/2026-07-30-effect-chain-batch-lab.md,
# in one command. Hermetic: no daemon, no ~/.local/state, ephemeral ports, and
# scratch dbs that are removed on the way out.
#
#   bash v6/tsv2/labs/effect-chain/run-all.sh
#
# Roughly 90 seconds; receipt 2's 100-demand cells and receipt 1c's 50-seed
# chain are the slow ones (both serialize real subprocesses through the host
# runner's concatMap, which is exactly what they are measuring).
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
TSV2="$(cd "$HERE/../.." && pwd)"
cd "$TSV2"

export SPREFA_CONFIG=/nonexistent/x.toml
export DL_NO_DAEMON=1

run_ts() {
  printf '\n════ %s ════\n' "$1"
  node --experimental-transform-types "$HERE/$1" 2>&1 \
    | grep -v 'ExperimentalWarning' \
    | grep -v 'trace-warnings'
}

run_ts 1_chain.ts
run_ts 2_batch.ts
printf '\n════ 3_v5_collect.sh ════\n'
bash "$HERE/3_v5_collect.sh"
run_ts 4_fanin_gap.ts
