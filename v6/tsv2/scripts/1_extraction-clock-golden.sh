#!/usr/bin/env bash
# Deterministic extraction clock golden. The lab drives the shipped watcher
# seam with a virtual scheduler and runs the in-tree RELEASE extractor.
#
#   v6/tsv2/scripts/1_extraction-clock-golden.sh
#
# It pins three ticks, final empty call/finding relations after an edit, and the
# statement count of the two-row host-response batch. It creates only ignored
# generated served modules and a temporary scratch corpus.
set -euo pipefail

TSV2="$(cd "$(dirname "$0")/.." && pwd)"
EXTRACT_BIN="${DL_EXTRACT_BIN:-$TSV2/../sprefa-extract/target/release/extract}"

if [ ! -x "$EXTRACT_BIN" ]; then
  printf 'missing release extractor: %s\nrun: (cd %s/../sprefa-extract && cargo build --release --features cli --bin extract)\n' "$EXTRACT_BIN" "$TSV2" >&2
  exit 1
fi

cd "$TSV2"
DL_EXTRACT_BIN="$EXTRACT_BIN" node --experimental-transform-types labs/0_extraction-clock-golden.ts
