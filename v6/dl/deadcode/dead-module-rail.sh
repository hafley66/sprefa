#!/usr/bin/env bash
# @comment-ok: the script's usage contract, the one doc site for its arguments.
# dead-module-rail.sh -- print the rail's findings. Argument 1 is the tree to
# read, argument 2 the glob to seed, argument 3 a comma-separated root list;
# with none, a Cargo.toml at the target makes the PROGRAM derive its roots from
# `cargo metadata`.
#   bash v6/dl/deadcode/dead-module-rail.sh ~/projects/hafley-rs 'crates/*/src/*.rs' 'crates/boop/src/main.rs'
# In a git pathspec `*` crosses `/` but `**` demands a directory level, so
# `crates/*/src/*.rs` matches 82 files where `crates/*/src/**/*.rs` matches 17.
#
# Everything below the seeds is REPORT FORMATTING; the engine work is one `dl6 run`.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
V6="$(cd "$HERE/../.." && pwd)"
ROOT="$(cd "$V6/.." && pwd)"
DL6="${DL6:-$V6/sprefa-engine-rs/target/release/dl6}"
TARGET="${1:-$ROOT}"
GLOB="${2:-crates/boop-acp/src/*.rs}"
ROOTS="${3:-}"
TAB="$(printf '\t')"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/dead-module.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
fail() { printf 'FAIL  %s\n' "$*" >&2; exit 1; }

[ -x "$DL6" ] || fail "no dl6 at $DL6; cargo build --release --bin dl6 in $V6/sprefa-engine-rs"

# Seeds are runner flags. With no roots named and no Cargo.toml the crawl
# reaches nothing and rail_unreachable_module names every file, so say so.
SEEDS=(--arrive "want=$GLOB")
if [ -n "$ROOTS" ]; then
  IFS=',' read -r -a root_list <<<"$ROOTS"
  for root_path in "${root_list[@]}"; do
    [ -n "$root_path" ] && SEEDS+=(--arrive "root_file=$root_path")
  done
elif [ -f "$TARGET/Cargo.toml" ] && command -v cargo >/dev/null; then
  SEEDS+=(--arrive "cargo_manifest=$TARGET")
else
  printf 'WARN no roots: the crawl reaches nothing and rail_unreachable_module names every file\n' >&2
fi

# One TSV line per row, `rel<TAB>col...`, so no shell here parses JSON.
RELS='cargo_root_count,rail_root_not_a_source,rail_dead_module,rail_unreachable_module,rail_unproven_module,module_reach'
"$DL6" run "$HERE/dead-module-rail.dl6" --root "$TARGET" "${SEEDS[@]}" \
  --final-only --final-tsv --final-rels "$RELS" \
  >"$WORK/final.tsv" 2>"$WORK/err" || fail "run: $(tail -20 "$WORK/err")"

rows_of() { grep "^$1$TAB" "$WORK/final.tsv" | cut -f2- || true; }

roots_derived="$(rows_of cargo_root_count)"
[ -n "$roots_derived" ] && printf 'roots from cargo metadata: %s\n' "$roots_derived" >&2

rows_of rail_root_not_a_source | while IFS="$TAB" read -r root_path; do
  printf 'WARN root not in the glob: %s\n' "$root_path"
done

# The `?` reads carry `order by defs desc, path`, so the rows arrive ordered
# and nothing here sorts.
defs_table() {
  printf '== %s ==\n' "$2"
  rows_of "$1" | while IFS="$TAB" read -r module_path defs; do
    printf '  %4s defs  %s\n' "$defs" "$module_path"
  done
}
defs_table rail_dead_module        'rail_dead_module (defs>=5, zero called from another file)'
defs_table rail_unreachable_module 'rail_unreachable_module (the crawl, from declared roots)'
defs_table rail_unproven_module    'rail_unproven_module (reached only through ambiguous names)'

printf '== module_reach (defs / used-across) ==\n'
rows_of module_reach \
  | while IFS="$TAB" read -r module_path defs used ambiguous; do
      printf '  %4s / %-4s amb=%-4s %s\n' "$defs" "$used" "$ambiguous" "$module_path"
    done

printf 'findings=%s unproven=%s unreachable=%s\n' \
  "$(rows_of rail_dead_module | grep -c . || true)" \
  "$(rows_of rail_unproven_module | grep -c . || true)" \
  "$(rows_of rail_unreachable_module | grep -c . || true)"
