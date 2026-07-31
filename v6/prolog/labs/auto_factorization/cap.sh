#!/usr/bin/env bash
# Budget wrapper for every lab invocation. Args: SECONDS LABEL CMD...
set -uo pipefail
LAB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$LAB_DIR/../../../.." && pwd)"
# shellcheck source=/dev/null
. "$REPO_ROOT/v6/tools/run-capped.sh"
capped "$@"
