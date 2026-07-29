#!/usr/bin/env bash
# parity.sh -- DELIVERABLE 5: the v5 rail vs the v6 route over ONE pinned
# corpus, every diff row classified. The rig is flagship-callgraph.sh's, reused
# rather than reinvented:
#
#   * the corpus is a LITERAL LIST of this repository's own files, copied into
#     a scratch tree that both legs use as cwd, so "the pinned corpus" is a
#     fact in this file and not a function of what the tree contains;
#   * the v5 leg runs `target/release/dl` with DL_STATE_DIR and --db both
#     pointing INSIDE the scratch tree, so nothing under ~/.local/state/sprefa
#     is read or written and the daemon is untouched;
#   * `std/arch.dl` and `std/suppress.dl` are COPIED BYTE-FOR-BYTE (sha
#     asserted below) -- neither rail is edited. Only the importer, which those
#     files' own headers say the importer must supply ("this file scans
#     nothing; the importer's scan rules decide which files feed
#     comment_node"), is written here.
#
# TWO ARTIFACTS, because the base fact and the technique fail differently:
#
#   comment_node   path, line, col, end_line, end_col, kind, text
#                  v5: the builtin rel.   v6: the cst family through cn.py.
#                  This is the ONE fact all seven techniques ride, so grading
#                  it grades the route rather than one rail.
#
#   arch_node      path, line, url
#                  v5: std/arch.dl's spine.   v6: arch-rail.dl6's, computed
#                  here through the same host template the served program uses.
#
# THE HONESTY RULE (flagship's): a non-empty diff with every row classified is
# an honest result. The corpus is never shrunk to empty a diff.
set -uo pipefail
LAB="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$LAB/../../../.." && pwd)"
DL="${DL_BIN:-$ROOT/target/release/dl}"
EX="${DL_EXTRACT_BIN:-$ROOT/v6/sprefa-extract/target/release/extract}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/cn-parity.XXXXXX")"
CORPUS="$WORK/corpus"
OUT="$LAB/out"
mkdir -p "$OUT" "$CORPUS/src" "$CORPUS/std"

[ -x "$DL" ] || { echo "FAIL no v5 dl binary at $DL (cargo build --release --bin dl)"; exit 1; }
[ -x "$EX" ] || { echo "FAIL no extract binary at $EX"; exit 1; }

# ── THE PINNED CORPUS ───────────────────────────────────────────────────────
# Rust, because that is where BOTH engines have a comment grammar. v5's
# `lang_label_for_path` (src/cst.rs) maps 17 extensions and `pl` is not one of
# them, so this repo's own prolog sources -- the dogfood target the user named
# -- are extractable by v6 and INVISIBLE to v5. That asymmetry is itself a
# graded finding, recorded below rather than worked around.
FILES="src/main.rs src/lower.rs src/cst.rs src/parse_mod.rs src/strata.rs"
cp "$ROOT/src/main.rs"          "$CORPUS/src/main.rs"
cp "$ROOT/src/lower.rs"         "$CORPUS/src/lower.rs"
cp "$ROOT/src/cst.rs"           "$CORPUS/src/cst.rs"
cp "$ROOT/src/parse/mod.rs"     "$CORPUS/src/parse_mod.rs"
cp "$ROOT/src/engine/strata.rs" "$CORPUS/src/strata.rs"
cp "$ROOT/std/arch.dl" "$CORPUS/std/arch.dl"
cp "$ROOT/std/suppress.dl" "$CORPUS/std/suppress.dl"

for rail in arch suppress; do
  a="$(shasum -a 256 "$ROOT/std/$rail.dl" | cut -d' ' -f1)"
  b="$(shasum -a 256 "$CORPUS/std/$rail.dl" | cut -d' ' -f1)"
  [ "$a" = "$b" ] || { echo "FAIL std/$rail.dl was modified in the copy"; exit 1; }
done
echo "corpus: $(echo $FILES | wc -w | tr -d ' ') rust files; std/arch.dl + std/suppress.dl byte-identical to the repo"

cd "$CORPUS"
git init -q
git add -A >/dev/null 2>&1
git -c user.email=lab@local -c user.name=lab commit -qm pin >/dev/null 2>&1

# ── the importer: the ONLY .dl written here ─────────────────────────────────
cat >"$CORPUS/probe.dl" <<'EOF'
# The importer std/arch.dl's own header requires: it scans, the module derives.
use "std/arch.dl".
rel seen(p: file).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev).
? comment_node(path, line, col, end_line, end_col, text, kind).
EOF

cat >"$CORPUS/probe_arch.dl" <<'EOF'
use "std/arch.dl".
rel seen(p: file).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev).
? arch_node(path, line, url).
EOF

run_v5() {
  DL_STATE_DIR="$WORK/state" "$DL" "$CORPUS/$1" \
    --db "$WORK/$1.sqlite" --no-daemon 2>"$WORK/$1.err"
}

# ── v5 leg ──────────────────────────────────────────────────────────────────
run_v5 probe.dl > "$WORK/v5-comment.raw"
run_v5 probe_arch.dl > "$WORK/v5-arch.raw"

# v5 prints tab-separated rows in the query's column order, wrapped in a `? rel
# => cols` header and an `(N rows)` footer. Both are stripped here; nothing
# else about the output is touched.
strip_v5() { grep -v '^?' "$1" | grep -vE '^ *\([0-9]+ rows?\) *$' | grep -v '^ *$'; }
strip_v5 "$WORK/v5-comment.raw" | LC_ALL=C sort -u > "$OUT/v5-comment.tsv"
strip_v5 "$WORK/v5-arch.raw"    | LC_ALL=C sort -u > "$OUT/v5-arch.tsv"

# ── v6 leg: the same route the two receipt programs run ─────────────────────
: > "$WORK/v6-comment.raw"
: > "$WORK/v6-arch.raw"
for file in $FILES; do
  nice -n 19 "$EX" --family cst "$file" 2>/dev/null \
    | python3 "$LAB/cn.py" comments "$file" >> "$WORK/v6-comment.raw"
  bash "$LAB/probes/arch_template.sh" "$file" \
    | python3 -c '
import json, sys
path = sys.argv[1]
for raw in sys.stdin:
    row = json.loads(raw)
    row["path"] = path
    print(json.dumps(row, separators=(",", ":")))
' "$file" >> "$WORK/v6-arch.raw"
done

python3 "$LAB/parity_project.py" comment "$WORK/v6-comment.raw" | LC_ALL=C sort -u > "$OUT/v6-comment.tsv"
# the grammar witness, applied exactly as arch-rail.dl6's rule applies it
python3 "$LAB/parity_project.py" arch "$WORK/v6-arch.raw" "$WORK/v6-comment.raw" | LC_ALL=C sort -u > "$OUT/v6-arch.tsv"

# ── the grade ───────────────────────────────────────────────────────────────
grade() {
  local name="$1" v5="$2" v6="$3"
  local n5 n6 only5 only6 both
  n5=$(wc -l < "$v5" | tr -d ' '); n6=$(wc -l < "$v6" | tr -d ' ')
  only5=$(LC_ALL=C comm -23 "$v5" "$v6" | tee "$OUT/$name.only-v5.tsv" | wc -l | tr -d ' ')
  only6=$(LC_ALL=C comm -13 "$v5" "$v6" | tee "$OUT/$name.only-v6.tsv" | wc -l | tr -d ' ')
  both=$(LC_ALL=C comm -12 "$v5" "$v6" | wc -l | tr -d ' ')
  printf '%-12s v5=%-6s v6=%-6s shared=%-6s only-v5=%-5s only-v6=%-5s\n' \
    "$name" "$n5" "$n6" "$both" "$only5" "$only6"
}

echo
printf '%-12s %s\n' artifact 'counts'
grade comment_node "$OUT/v5-comment.tsv" "$OUT/v6-comment.tsv"
grade arch_node    "$OUT/v5-arch.tsv"    "$OUT/v6-arch.tsv"
echo
echo "=== classification ==="
python3 "$LAB/parity_classify.py" "$OUT"
