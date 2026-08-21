#!/usr/bin/env bash
# @comment-ok: the invocation contract, the single doc site for this rail's gate.
# Compile the eprintln ratchet through the rust door and grade it against a
# fixture whose every file is labelled with how many findings it owes. Argument
# 1 is the tree to read, argument 2 the pathspec; with NO arguments it runs the
# labelled fixture and asserts the table, with a pathspec it only reports.
#
#   bash v6/dl/rails/no-new-eprintln-rail.sh                       # graded
#   bash v6/dl/rails/no-new-eprintln-rail.sh . 'src/**/*.rs'       # report only
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
V6="$(cd "$HERE/../.." && pwd)"
ROOT="$(cd "$V6/.." && pwd)"
ENGINE="$V6/sprefa-engine-rs"
TARGET="${1:-$ROOT}"
GLOB="${2:-v6/dl/rails/fixtures/eprintln/*.rs}"
GRADED=0
[ "$#" -eq 0 ] && GRADED=1
TAB="$(printf '\t')"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/no-new-eprintln.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
rc=0
say() { printf '%-6s %-24s %s\n' "$1" "$2" "$3"; }
bad() { rc=1; say FAIL "$1" "$2"; }
fail() { printf 'FAIL  %s\n' "$*" >&2; exit 1; }

bash "$V6/prolog/compile/scripts/dl6c.sh" "$HERE/no-new-eprintln-rail.dl6" --target rust --out "$WORK" \
  >"$WORK/compile.log" 2>&1 && mv "$WORK/no-new-eprintln-rail.rs" "$WORK/rail.rs" \
  || fail "compile: $(tail -20 "$WORK/compile.log")"

cargo build --release --quiet --manifest-path "$ENGINE/Cargo.toml" --bin emit_rust_harness \
  >"$WORK/build.log" 2>&1 || fail "cargo build: $(tail -5 "$WORK/build.log")"

RELS='rail_eprintln_counted,rail_eprintln_new,rail_eprintln_exceeded,rail_hit_count,rail_waived_count'
( cd "$TARGET" && DL_ADAPTERS_DIR="$HERE" \
    DL_EXTRACT_BIN="${DL_EXTRACT_BIN:-$V6/sprefa-extract/target/release/extract}" \
    "$ENGINE/target/release/emit_rust_harness" "$WORK/rail.rs" --arrive "want=$GLOB" \
    --live-hosts --final-only --final-tsv --final-rels "$RELS" ) \
  >"$WORK/final.tsv" 2>"$WORK/err" || fail "run: $(tail -20 "$WORK/err")"

rows_of() { grep "^$1$TAB" "$WORK/final.tsv" | cut -f2- || true; }

printf '== rail_eprintln_counted (unwaived sites, against the baseline) ==\n'
rows_of rail_eprintln_counted | while IFS="$TAB" read -r file_path at; do
  printf '  %s  @%s\n' "$file_path" "$at"
done
printf '== rail_eprintln_new (no baseline row, one line per site) ==\n'
rows_of rail_eprintln_new | while IFS="$TAB" read -r file_path at; do
  printf '  %s  @%s\n' "$file_path" "$at"
done
printf '== rail_eprintln_exceeded (grandfathered file grew past its baseline) ==\n'
rows_of rail_eprintln_exceeded | while IFS="$TAB" read -r file_path hits allowed; do
  printf '  %s  %s > %s\n' "$file_path" "$hits" "$allowed"
done
printf 'hits=%s waived=%s new=%s exceeded=%s\n' \
  "$(rows_of rail_hit_count)" "$(rows_of rail_waived_count)" \
  "$(rows_of rail_eprintln_new | grep -c . || true)" \
  "$(rows_of rail_eprintln_exceeded | grep -c . || true)"

[ "$GRADED" = 0 ] && exit 0

# The label table. `n` is how many findings the file must contribute.
rows_of rail_eprintln_new | cut -f1 | sed 's|.*/||' | sort >"$WORK/found.set"

while read -r file want why; do
  [ -z "$file" ] && continue
  got="$(grep -cx "$file" "$WORK/found.set" || true)"
  if [ "$got" = "$want" ]; then
    say ok "$file" "$why"
  else
    bad "$file" "expected $want findings, got $got"
  fi
done <<LABELS
bare.rs             2   no baseline row and no waiver, one row per site
waived_above.rs     0   the comment-above form, v5's waiver_line == line - 1
waived_trailing.rs  0   the trailing form v5 needed a second rule to see
near_miss.rs        2   a marker neighbouring another statement waives nothing
clean.rs            0   tracing only, no print to find
multiline_waiver.rs 0   a marker on a multi-line call's closing line, which v5 missed
LABELS

# A rail that found nothing because the host answered nothing must not read as
# a rail over a clean tree.
[ "$(rows_of rail_hit_count)"    = "7" ] || bad "hit_count"    "expected 7, got $(rows_of rail_hit_count)"
[ "$(rows_of rail_waived_count)" = "3" ] || bad "waived_count" "expected 3, got $(rows_of rail_waived_count)"

[ "$rc" = 0 ] && echo "NO-NEW-EPRINTLN OK  findings=$(grep -c . <"$WORK/found.set" || true)"
exit "$rc"
