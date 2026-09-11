#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)

exec swipl -q \
  -s "$REPO_ROOT/v7/bench/1_determinism_evidence.pl" \
  -g main -t halt -- "$@"
