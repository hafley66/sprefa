#!/usr/bin/env bash
# @comment-ok: the runner's usage contract, its two doors, and its env.
# run.sh -- compile crosswalk.dl6 against one fixture and fold it.
#
#   bash v6/dl/crosswalk/run.sh grafana              one-shot, prints the reads
#   bash v6/dl/crosswalk/run.sh grafana --socket P   resident, folds then serves
#
# THE PROGRAM IS crosswalk.dl6 CONCATENATED WITH <fixture>.entries.dl6, because
# `entry_point` / `repo_rev` / `repo_scope` are plain facts and a fact belongs in
# the source. `{CHECKOUT_ROOT}` in the entries file is filled with the fixture
# cache root; nothing else is substituted.
#
# `--socket` hands the folded seam to serve.rs and parks. The fold has already
# happened by then: `emit_rust_harness` runs the schedule first and only opens
# the socket afterwards (src/bin/emit_rust_harness.rs, the `if let Some(path)`
# tail), so a resident process answers reads over an already-settled db.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
V6="$(cd "$HERE/../.." && pwd)"
REPO="$(cd "$V6/.." && pwd)"
ENGINE="$V6/sprefa-engine-rs"
HARNESS="${DL_RUST_HARNESS:-$ENGINE/target/release/emit_rust_harness}"
EXTRACT="${DL_EXTRACT_BIN:-$V6/sprefa-extract/target/release/extract}"

FIXTURE="${1:?usage: run.sh <fixture> [harness flags...]}"
shift || true
ENTRIES="$HERE/fixtures/$FIXTURE.entries.dl6"
[ -f "$ENTRIES" ] || { printf 'no entries file at %s\n' "$ENTRIES" >&2; exit 1; }

WORK="${CROSSWALK_WORK:-$(mktemp -d "${TMPDIR:-/tmp}/crosswalk.XXXXXX")}"
mkdir -p "$WORK"
CHECKOUT_ROOT="$(bash "$HERE/fixtures/$FIXTURE.sh" --print-root)"

sed "s|{CHECKOUT_ROOT}|$CHECKOUT_ROOT|g" "$ENTRIES" \
  | cat "$HERE/crosswalk.dl6" - >"$WORK/crosswalk.dl6"

if [ ! -x "$HARNESS" ]; then
  timeout 900 cargo build --release --quiet --manifest-path "$ENGINE/Cargo.toml" \
    --bin emit_rust_harness >"$WORK/build.log" 2>&1 \
    || { printf 'harness build: %s\n' "$(tail -5 "$WORK/build.log")" >&2; exit 1; }
fi

timeout 300 bash "$REPO/v6/prolog/compile/scripts/dl6c.sh" "$WORK/crosswalk.dl6" \
  --target rust --out "$WORK" >"$WORK/compile.log" 2>&1 \
  || { printf 'compile: %s\n' "$(tail -20 "$WORK/compile.log")" >&2; exit 1; }

# `dep_gap` is every pin no checkout answers for and runs to four figures on a
# real go.mod, so it is off the default read and named by CROSSWALK_RELS.
RELS="${CROSSWALK_RELS:-repo_file_count,dep_edge,cross_edge_count,cross_path,cross_reach,skew,entry_unreached}"
exec env DL_EXTRACT_BIN="$EXTRACT" \
  "$HARNESS" "$WORK/crosswalk.rs" --live-hosts --arrive 'crosswalk_run='"$FIXTURE" \
  --final-only --final-tsv --final-rels "$RELS" "$@"
