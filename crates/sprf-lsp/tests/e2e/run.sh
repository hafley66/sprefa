#!/bin/bash
# Run LSP E2E tests

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"

echo "Building sprf-lsp..."
cd "$PROJECT_ROOT"
cargo build -p sprefa_sprf_lsp 2>&1 | grep -E "(Compiling|Finished|error)" || true

echo ""
cd "$SCRIPT_DIR"
python3 test_lsp.py "$@"
