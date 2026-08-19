#!/usr/bin/env bash
# run.sh: every table in REPORT.md, in report order.
#
#   bash v6/labs/shared_frontier/run.sh            # Q1..Q5 against a compiled pokeapi
#   COMPILE=1 bash v6/labs/shared_frontier/run.sh  # recompile pokeapi first (526 s measured)
#
# node is the repo's node (v24.15.0), TS runs through --experimental-transform-types
# exactly as v6/tsv2/package.json runs its own scripts. The SQLite driver is
# @libsql/client, the one v6/tsv2/runtime/scratchStore.ts opens. A lab outside
# v6/tsv2 resolves that bare specifier through the symlink this script makes.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$HERE"
mkdir -p out out/prof
ln -sfn ../../tsv2/node_modules node_modules

NODE_RUN=(node --experimental-transform-types)
quiet() { "$@" 2>&1 | grep -v "ExperimentalWarning\|trace-warnings"; }

if [ "${COMPILE:-0}" = "1" ] || [ ! -f out/pokeapi_gen.ts ]; then
  echo "# compiling pokeapi_gen.dl6 through the text door" >&2
  time bash ../../prolog/compile/scripts/compile_dl6.sh \
    ../../tsv2/gen/pokeapi_gen.dl6 out/pokeapi_gen.ts
fi

quiet "${NODE_RUN[@]}" rig/q1_table_bill.ts  | tee out/q1.md
quiet "${NODE_RUN[@]}" rig/q1_sections.ts    | tee out/q1_sections.md
quiet "${NODE_RUN[@]}" rig/q1_ddl_split.ts   | tee out/q1_ddl_split.md
quiet "${NODE_RUN[@]}" rig/q1_index_owner.ts | tee out/q1_index_owner.md
quiet "${NODE_RUN[@]}" rig/q2_tick_cost.ts   | tee out/q2.md
quiet "${NODE_RUN[@]}" rig/q2_explain.ts     | tee out/q2_explain.md
quiet "${NODE_RUN[@]}" rig/q2d_keyorder.ts   | tee out/q2d.md
quiet "${NODE_RUN[@]}" rig/q3_boot_cost.ts   | tee out/q3.md
quiet "${NODE_RUN[@]}" rig/q4_contention.ts  | tee out/q4.md
quiet "${NODE_RUN[@]}" rig/q5_profile.ts     | tee out/q5.md

rm -f out/prof/*.cpuprofile
node --cpu-prof --cpu-prof-dir=out/prof --experimental-transform-types rig/q5_profile.ts >/dev/null
quiet "${NODE_RUN[@]}" rig/q5_summarize.ts   | tee out/q5b.md
