#!/usr/bin/env bash
# arm_census.sh — run the lower.pl arm + throw-site coverage census.
#
# One deterministic swipl invocation; the census is reproducible (two runs,
# same counts). See arm_census.pl for what it measures and how.
#
# Run from anywhere:
#   v6/prolog/compile/scripts/arm_census.sh
set -euo pipefail
cd "$(dirname "$0")/../.."   # v6/prolog
swipl -q -s compile/scripts/arm_census.pl -g census -t halt
