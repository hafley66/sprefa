#!/usr/bin/env bash
# matrix.sh -- the ONE command that regenerates the whole type matrix.
#
#   1  gen_cells.mjs   enumerate the axes; one .dl6 + one schedule per cell
#   2  drive.mjs       both doors per cell (compile, oracle, emitter x 2 modes)
#   3  classify.mjs    the four verdicts -> MATRIX.md + out/matrix.json
#
# Hermetic: everything it writes lives under this directory's out/. No daemon,
# no network, no ~/.local/state/sprefa.
#
# Run: bash v6/prolog/labs/type_matrix/matrix.sh

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$HERE"

# The two symlinks an emitted cell module needs to resolve its own
# `../runtime/...` and bare `rxjs` imports. They are gitignored, so a fresh
# clone, a worktree or a merge that did not carry them leaves the whole matrix
# reading emitter_run_error at exit 0 -- that has now happened three times
# (this lab's own merged-main regrade, and the bench-cli lane). Recreated here
# rather than diagnosed again.
ln -sfn ../../../tsv2/runtime runtime
ln -sfn ../../../tsv2/node_modules node_modules
if [ ! -e node_modules/rxjs ]; then
  echo "matrix.sh: v6/tsv2/node_modules is empty -- run pnpm install in v6/tsv2 and v6/sprefa-store/js" >&2
  exit 1
fi

node gen_cells.mjs
node drive.mjs
node classify.mjs
