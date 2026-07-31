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

node gen_cells.mjs
node drive.mjs
node classify.mjs
