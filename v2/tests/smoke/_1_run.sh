#!/usr/bin/env bash
# Smoke 1: POST /run against v2/examples/g1_fns.sprf, rooted at ext/sem.
# Asserts the rule header line + at least one row with NAME=... captures.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
source ./_common.sh

require_bins
reset_state

# Dogfood: run sprefa against its own repo. The sprefa slug is the basename
# of REPO_ROOT (auto-detected by the workspace walker); on this machine
# that's "sprefa".
[ -d "$REPO_ROOT/.git" ] || { echo "SKIP: $REPO_ROOT is not a git repo"; exit 0; }
SLUG="$(basename "$REPO_ROOT")"

start_server

out="$("$SPREFA_CLI_BIN" run "$V2_DIR/examples/g1_fns.sprf" --root "$REPO_ROOT" 2>/dev/null)"
echo "$out" | tail -20

echo "$out" | grep -qE '^rule pub_fn — [0-9]+ rows$' \
    || { echo "missing rule header" >&2; exit 1; }
echo "$out" | grep -q 'NAME=' \
    || { echo "no NAME capture emitted" >&2; exit 1; }
echo "$out" | grep -qE "^\s+${SLUG}@HEAD " \
    || { echo "no ${SLUG}@HEAD row" >&2; exit 1; }

echo "OK"
