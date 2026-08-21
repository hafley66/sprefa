#!/usr/bin/env bash
# @comment-ok: the wrapper contract, the single doc site for who writes the file.
# report-extract.sh -- run v6/dl/dataflow/report_extract.dl6 through `dl6 run` and
# write its report_markdown rel to docs/extract-dataflow.md.
#
# THE WRITE IS THIS SCRIPT'S, NOT THE PROGRAM'S. dl6 has no fs effect: a program
# derives text and a consumer decides where text lands. So the rail derives
# `report_markdown(section_ordinal, section_name, section_text)` and the python
# at the bottom of this file is the only thing that touches the disk.
#
# @comment-ok: the door contract, continued.
# THE DOOR IS `dl6 run`, which runs the hosts live against the linked executors
# named in report_extract.adapters.json:
#   - `files` routes to SoopyFilesExecutor       (hosts.rs executor_for_plan)
#   - the four extraction hosts route to SprefaExtractExecutor
#                                                (hosts.rs executor_for)
# Both are LINKED. No `git` child and no `extract` child is spawned, and
# DL_EXTRACT_BIN is deliberately left unset below so a subprocess spelling would
# fail loudly rather than pass by accident (the receipt live_hosts.rs
# `live_extract_runs_in_process_with_no_binary_configured` makes).
#
# `--final` reads the rel through the IR's own final_select AFTER the fold, so
# no consumer here re-folds a tick log to learn a rel's final state.
#
# Run: bash v6/dl/dataflow/report-extract.sh [glob] [output.md]

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
V6="$(cd "$HERE/../.." && pwd)"
ROOT="$(cd "$V6/.." && pwd)"
DL6="${DL6:-$V6/sprefa-engine-rs/target/release/dl6}"
RAIL="$HERE/report_extract.dl6"

GLOB="${1:-v6/sprefa-extract/src/*.rs}"
OUT="${2:-$ROOT/docs/extract-dataflow.md}"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/report-extract.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

say() { printf '%s\n' "$*"; }
fail() { printf 'FAIL  %s\n' "$*" >&2; exit 1; }

[ -x "$DL6" ] || fail "no dl6 at $DL6; cargo build --release --bin dl6 in $V6/sprefa-engine-rs"

# --root is the repo root: `files` resolves the pathspec against this tree and
# the extractor reads each answered path relative to it.
start="$SECONDS"
env -u DL_EXTRACT_BIN "$DL6" run "$RAIL" --root "$ROOT" \
  --arrive "want=$GLOB" --final --final-only --final-rels report_markdown \
  >"$WORK/final.jsonl" 2>"$WORK/run.err" \
  || fail "dl6 run: $(tail -5 "$WORK/run.err")"
say "PASS  ran on the rust door in $((SECONDS - start))s"

python3 - "$WORK/final.jsonl" "$OUT" <<'PYTHON'
import json
import sys

final_path, out_path = sys.argv[1], sys.argv[2]
document = json.loads(open(final_path).read().strip())
rows = [tuple(row) for row in document["rows"]]
if not rows:
    raise SystemExit("no report_markdown rows in the final read")
ordinals = [row[0] for row in rows]
if len(set(ordinals)) != len(ordinals):
    raise SystemExit(f"two rows share a section ordinal: {sorted(ordinals)}")
ordered = sorted(rows)
open(out_path, "w").write("\n".join(row[2] for row in ordered))
print(f"PASS  wrote {out_path}: {len(ordered)} sections")
for ordinal, name, text in ordered:
    print(f"      {ordinal} {name}: {len(text.splitlines())} lines")
PYTHON
