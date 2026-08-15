#!/usr/bin/env bash
# @comment-ok: the wrapper contract, the single doc site for who writes the file.
# report-extract.sh -- run v6/dl/dataflow/report_extract.dl6 on the RUST door and
# write its report_markdown rel to docs/extract-dataflow.md.
#
# THE WRITE IS THIS SCRIPT'S, NOT THE PROGRAM'S. dl6 has no fs effect: a program
# derives text and a consumer decides where text lands. So the rail derives
# `report_markdown(section_ordinal, section_name, section_text)` and the four
# lines at the bottom of this file are the only thing that touches the disk.
#
# THE DOOR IS emit_rust_harness --live-hosts, and that is the point of the arc:
#   - `files` routes to SoopyFilesExecutor       (hosts.rs executor_for_plan)
#   - the four extraction hosts route to SprefaExtractExecutor
#                                                (hosts.rs executor_for)
# Both are LINKED. No `git` child and no `extract` child is spawned, and
# DL_EXTRACT_BIN is deliberately left unset below so a subprocess spelling would
# fail loudly rather than pass by accident (the receipt live_hosts.rs
# `live_extract_runs_in_process_with_no_binary_configured` makes).
#
# The harness prints the TICK LOG and nothing else, so the final state of a rel
# is the fold of its adds minus its dels across every tick.
#
# Run: bash v6/dl/dataflow/report-extract.sh [glob] [output.md]

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
V6="$(cd "$HERE/../.." && pwd)"
ROOT="$(cd "$V6/.." && pwd)"
ENGINE="$V6/sprefa-engine-rs"
RAIL="$HERE/report_extract.dl6"

GLOB="${1:-v6/sprefa-extract/src/*.rs}"
OUT="${2:-$ROOT/docs/extract-dataflow.md}"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/report-extract.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

say() { printf '%s\n' "$*"; }
fail() { printf 'FAIL  %s\n' "$*" >&2; exit 1; }

swipl -q -l "$V6/prolog/compile.pl" -l "$V6/prolog/emit_rust.pl" \
  -g "compile_dl6('$RAIL','$WORK/program.rs',[emitter(emit_rust:emit_program)])" -g halt \
  >"$WORK/compile.log" 2>&1 || fail "rust door compile: $(cat "$WORK/compile.log")"
say "PASS  compiled $RAIL through the rust door"

cargo build --release --quiet --manifest-path "$ENGINE/Cargo.toml" --bin emit_rust_harness \
  >"$WORK/build.log" 2>&1 || fail "cargo build emit_rust_harness: $(tail -5 "$WORK/build.log")"

python3 -c "
import json, sys
json.dump([[{'rel': 'want', 'sign': 'add', 'row': [sys.argv[1]]}]],
          open(sys.argv[2], 'w'))
" "$GLOB" "$WORK/schedule.json"

# cwd is the repo root: `files` resolves the pathspec against this tree and the
# extractor reads each answered path relative to it.
start="$SECONDS"
( cd "$ROOT" && env -u DL_EXTRACT_BIN \
    "$ENGINE/target/release/emit_rust_harness" "$WORK/program.rs" "$WORK/schedule.json" --live-hosts ) \
  >"$WORK/ticks.jsonl" 2>"$WORK/run.err" \
  || fail "harness: $(tail -5 "$WORK/run.err")"
say "PASS  ran on the rust door in $((SECONDS - start))s, $(wc -l <"$WORK/ticks.jsonl" | tr -d ' ') ticks"

python3 - "$WORK/ticks.jsonl" "$OUT" <<'PYTHON'
import json
import sys

# One tick carries the del of a stale section and the add of its replacement, so
# the fold is over WHOLE rows; keying it on (ordinal, name) drops the new text.
ticks_path, out_path = sys.argv[1], sys.argv[2]
rows = set()
for line in open(ticks_path):
    if not line.strip():
        continue
    deltas = json.loads(line)["deltas"].get("report_markdown")
    if not deltas:
        continue
    for row in deltas["add"]:
        rows.add(tuple(row))
    for row in deltas["del"]:
        rows.discard(tuple(row))
if not rows:
    raise SystemExit("no report_markdown rows in the tick log")
ordinals = [row[0] for row in rows]
if len(set(ordinals)) != len(ordinals):
    raise SystemExit(f"two rows share a section ordinal: {sorted(ordinals)}")
ordered = sorted(rows)
open(out_path, "w").write("\n".join(row[2] for row in ordered))
print(f"PASS  wrote {out_path}: {len(ordered)} sections")
for ordinal, name, text in ordered:
    print(f"      {ordinal} {name}: {len(text.splitlines())} lines")
PYTHON
