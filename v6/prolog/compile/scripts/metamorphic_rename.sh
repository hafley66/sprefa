#!/usr/bin/env bash
# metamorphic_rename.sh — run the metamorphic rename pass.
#
# One deterministic swipl invocation (seed printed by the run itself); two runs
# produce identical counts. See metamorphic_rename.pl for what it measures.
#
# Run from anywhere:
#   v6/prolog/compile/scripts/metamorphic_rename.sh
set -euo pipefail
cd "$(dirname "$0")/../.."   # v6/prolog
swipl -q -s compile/scripts/metamorphic_rename.pl -g run -t halt
