#!/usr/bin/env bash
# hotpath-rails.sh -- compile the two rails in this directory through the rust
# door and print their findings, one `REL  row` line each.
#
# THE DOOR IS emit_rust_harness --live-hosts. prolog-hotpath-rails.dl6 routes
# its three extraction hosts to SprefaExtractExecutor through its adapters
# file, so no extract child is spawned for it; serde-default-rail.dl6 needs
# --ast-pattern, which the linked twin refuses by name, so it runs the binary
# and DL_EXTRACT_BIN has to be set for that one.
#
# Run: bash v6/dl/hotpath/hotpath-rails.sh [rev-worktree]
#      the optional argument is the tree to read; default is this one.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
V6="$(cd "$HERE/../.." && pwd)"
ROOT="$(cd "$V6/.." && pwd)"
ENGINE="$V6/sprefa-engine-rs"
TARGET="${1:-$ROOT}"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/hotpath-rails.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

fail() { printf 'FAIL  %s\n' "$*" >&2; exit 1; }

compile_rail() {
  swipl -q -l "$V6/prolog/compile.pl" -l "$V6/prolog/emit_rust.pl" \
    -g "compile_dl6('$HERE/$1.dl6','$WORK/$1.rs',[emitter(emit_rust:emit_program)])" -g halt \
    >"$WORK/$1.compile.log" 2>&1 || fail "compile $1: $(tail -5 "$WORK/$1.compile.log")"
}

cargo build --release --quiet --manifest-path "$ENGINE/Cargo.toml" --bin emit_rust_harness \
  >"$WORK/build.log" 2>&1 || fail "cargo build emit_rust_harness: $(tail -5 "$WORK/build.log")"

compile_rail prolog-hotpath-rails
compile_rail serde-default-rail

python3 -c "
import json, sys
json.dump([[{'rel': 'want', 'sign': 'add', 'row': ['v6/prolog/*.pl']}]], open(sys.argv[1], 'w'))
json.dump([[{'rel': 'want_source', 'sign': 'add', 'row': ['v6/sprefa-engine-rs/src/*.rs']},
            {'rel': 'want_snapshot', 'sign': 'add', 'row': ['v6/sprefa-engine-rs/tests/fixtures/*.program.rs']}]],
          open(sys.argv[2], 'w'))
" "$WORK/prolog.schedule.json" "$WORK/serde.schedule.json"

run_rail() {
  ( cd "$TARGET" && DL_ADAPTERS_DIR="$HERE" DL_EXTRACT_BIN="${DL_EXTRACT_BIN:-$V6/sprefa-extract/target/release/extract}" \
      "$ENGINE/target/release/emit_rust_harness" "$WORK/$1.rs" "$WORK/$2.schedule.json" --live-hosts ) \
    >"$WORK/$1.ticks.jsonl" 2>"$WORK/$1.err" || fail "$1: $(tail -5 "$WORK/$1.err")"
}

run_rail prolog-hotpath-rails prolog
run_rail serde-default-rail serde

python3 - "$WORK/prolog-hotpath-rails.ticks.jsonl" "$WORK/serde-default-rail.ticks.jsonl" <<'PYTHON'
import json
import sys

# The harness prints the tick log and nothing else, so a rel's final state is
# the fold of its adds minus its dels across every tick.
findings = 0
for path in sys.argv[1:]:
    rows = {}
    for line in open(path):
        if not line.strip():
            continue
        for rel, delta in json.loads(line)["deltas"].items():
            if not rel.startswith("rail_"):
                continue
            live = rows.setdefault(rel, {})
            for row in delta.get("add", []):
                live[json.dumps(row, sort_keys=True)] = row
            for row in delta.get("del", []):
                live.pop(json.dumps(row, sort_keys=True), None)
    for rel in sorted(rows):
        for row in sorted(rows[rel].values(), key=lambda r: json.dumps(r)):
            print(f"{rel}  {'  '.join(str(value) for value in row)}")
            findings += 1
print(f"findings={findings}")
PYTHON
