#!/usr/bin/env bash
# Smoke 0: server starts, /status round-trips over unix socket, server stops.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
source ./_common.sh

require_bins
reset_state
start_server

out="$("$SPREFA_CLI_BIN" status)"
echo "status: $out"

# Shape assertion.
echo "$out" | grep -q '"version":"0.1.0"'           || { echo "missing version field" >&2; exit 1; }
echo "$out" | grep -q '"lsp_doc_count":0'           || { echo "missing lsp_doc_count field" >&2; exit 1; }
echo "$out" | grep -q '"workspaces":0'              || { echo "missing workspaces field" >&2; exit 1; }

echo "OK"
