#!/usr/bin/env bash
# Deterministic RTK Query extraction golden. Four ast-grep patterns batch through
# one in-tree Rust extractor process per file digest; DL6 performs every union,
# capture join, and scope-containment join.
set -euo pipefail

TSV2="$(cd "$(dirname "$0")/.." && pwd)"
EXTRACT_BIN="${DL_EXTRACT_BIN:-$TSV2/../sprefa-extract/target/release/extract}"

if [ ! -x "$EXTRACT_BIN" ]; then
  printf 'missing release extractor: %s\nrun: (cd %s/../sprefa-extract && cargo build --release --features cli --bin extract)\n' "$EXTRACT_BIN" "$TSV2" >&2
  exit 1
fi

cd "$TSV2"
DL_EXTRACT_BIN="$EXTRACT_BIN" node --experimental-transform-types labs/1_rtkq-extraction-golden.ts
