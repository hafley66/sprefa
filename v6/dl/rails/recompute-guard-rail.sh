#!/usr/bin/env bash
# @comment-ok: the invocation contract, the single doc site for this rail's gate.
# Compile the rail through the rust door and grade it against a fixture whose
# every file is labelled with what the rail must say. Argument 1 is the tree to
# read, argument 2 the pathspec to seed; with NO arguments it runs the labelled
# fixture and asserts the label table, with a pathspec it only prints findings.
#
#   bash v6/dl/rails/recompute-guard-rail.sh                       # graded
#   bash v6/dl/rails/recompute-guard-rail.sh . 'src/**/*.rs'       # report only
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
V6="$(cd "$HERE/../.." && pwd)"
ROOT="$(cd "$V6/.." && pwd)"
ENGINE="$V6/sprefa-engine-rs"
TARGET="${1:-$ROOT}"
GLOB="${2:-v6/dl/rails/fixtures/recompute/*.rs}"
GRADED=0
[ "$#" -eq 0 ] && GRADED=1
TAB="$(printf '\t')"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/recompute-guard.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
rc=0
say() { printf '%-6s %-24s %s\n' "$1" "$2" "$3"; }
bad() { rc=1; say FAIL "$1" "$2"; }
fail() { printf 'FAIL  %s\n' "$*" >&2; exit 1; }

bash "$V6/prolog/compile/scripts/dl6c.sh" "$HERE/recompute-guard-rail.dl6" --target rust --out "$WORK" \
  >"$WORK/compile.log" 2>&1 && mv "$WORK/recompute-guard-rail.rs" "$WORK/rail.rs" \
  || fail "compile: $(tail -20 "$WORK/compile.log")"

cargo build --release --quiet --manifest-path "$ENGINE/Cargo.toml" --bin emit_rust_harness \
  >"$WORK/build.log" 2>&1 || fail "cargo build: $(tail -5 "$WORK/build.log")"

RELS='rail_unguarded_recompute,rail_recompute_count,rail_guarded_count,rail_waived_count'
( cd "$TARGET" && DL_ADAPTERS_DIR="$HERE" \
    DL_EXTRACT_BIN="${DL_EXTRACT_BIN:-$V6/sprefa-extract/target/release/extract}" \
    "$ENGINE/target/release/emit_rust_harness" "$WORK/rail.rs" --arrive "want=$GLOB" \
    --live-hosts --final-only --final-tsv --final-rels "$RELS" ) \
  >"$WORK/final.tsv" 2>"$WORK/err" || fail "run: $(tail -20 "$WORK/err")"

rows_of() { grep "^$1$TAB" "$WORK/final.tsv" | cut -f2- || true; }

printf '== rail_unguarded_recompute (fn calls embed_graph, no digest skip, no waiver) ==\n'
rows_of rail_unguarded_recompute | while IFS="$TAB" read -r file_path proc_name site_start; do
  printf '  %s  %s  @%s\n' "$file_path" "$proc_name" "$site_start"
done
printf 'recompute_sites=%s guarded_fns=%s waived_fns=%s findings=%s\n' \
  "$(rows_of rail_recompute_count)" "$(rows_of rail_guarded_count)" \
  "$(rows_of rail_waived_count)" "$(rows_of rail_unguarded_recompute | grep -c . || true)"

[ "$GRADED" = 0 ] && exit 0

# ── the label table ─────────────────────────────────────────────────────────
rows_of rail_unguarded_recompute | cut -f1,2 | sed 's|.*/||' | sort >"$WORK/found.set"

# file                proc              why this case exists
while read -r file proc why; do
  [ -z "$file" ] && continue
  want="$file${TAB}$proc"
  if [ "$proc" = "-" ]; then
    if grep -q "^$file$TAB" "$WORK/found.set"; then
      bad "$file" "reported, label says clean"
    else
      say ok "$file" "$why"
    fi
  elif grep -qx "$want" "$WORK/found.set"; then
    say ok "$file" "$why"
  else
    bad "$file" "expected finding on $proc, absent"
  fi
done <<LABELS
guarded.rs          -                 the digest skip and the recompute in one fn
waived.rs           -                 a doc-comment waiver clears the fn
unguarded.rs        rebuild_similarity  no guard and no waiver is the finding
closure_between.rs  -                 the v5 misfire: a closure between guard and recompute
nested_fn.rs        inner_recompute   a nested named fn owns its own marker
clean.rs            -                 no recompute marker at all
LABELS

# A rail that found nothing because the extractor answered nothing must not
# read as a rail over a clean tree.
[ "$(rows_of rail_recompute_count)" = "5" ] || bad "recompute_count" "expected 5, got $(rows_of rail_recompute_count)"
[ "$(rows_of rail_guarded_count)"   = "3" ] || bad "guarded_count"   "expected 3, got $(rows_of rail_guarded_count)"
[ "$(rows_of rail_waived_count)"    = "1" ] || bad "waived_count"    "expected 1, got $(rows_of rail_waived_count)"

[ "$rc" = 0 ] && echo "RECOMPUTE-GUARD OK  findings=$(grep -c . <"$WORK/found.set" || true)"
exit "$rc"
