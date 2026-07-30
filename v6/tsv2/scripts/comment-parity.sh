#!/usr/bin/env bash
# Compare the v5 comment_node and arch_node rails with the v6 extraction route
# over the verdict's pinned Rust corpus.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TSV2="$(cd "$SCRIPT_DIR/.." && pwd)"
ROOT="$(cd "$TSV2/../.." && pwd)"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/comment-parity.XXXXXX")"
CORPUS="$WORK/corpus"
DL="${DL_V5_BIN:-$ROOT/target/release/dl}"
EX="${DL_EXTRACT_BIN:-$ROOT/v6/sprefa-extract/target/release/extract}"
CN="${DL_COMMENT_NODE:-$SCRIPT_DIR/comment_node.py}"
ARCH="${DL_ARCH_MARKER:-$SCRIPT_DIR/comment_arch_marker.py}"
FILES="src/main.rs src/lower.rs src/cst.rs src/parse_mod.rs src/strata.rs"

say() { printf '%s\n' "$*"; }
fail() { say "FAIL  $*"; exit 1; }

if [ ! -x "$DL" ]; then
  say "building v5 release engine"
  (cd "$ROOT" && cargo build --release --bin dl) >"$WORK/v5-build.log" 2>&1 \
    || fail "v5 build: $(tail -8 "$WORK/v5-build.log")"
fi
if [ ! -x "$EX" ]; then
  say "building release extractor"
  (cd "$ROOT/v6/sprefa-extract" && cargo build --release --features cli --bin extract) >"$WORK/extract-build.log" 2>&1 \
    || fail "extractor build: $(tail -8 "$WORK/extract-build.log")"
fi
[ -x "$DL" ] || fail "v5 engine missing: $DL"
[ -x "$EX" ] || fail "extractor missing: $EX"

mkdir -p "$CORPUS/src" "$CORPUS/std"
cp "$ROOT/src/main.rs" "$CORPUS/src/main.rs"
cp "$ROOT/src/lower.rs" "$CORPUS/src/lower.rs"
cp "$ROOT/src/cst.rs" "$CORPUS/src/cst.rs"
cp "$ROOT/src/parse/mod.rs" "$CORPUS/src/parse_mod.rs"
cp "$ROOT/src/engine/strata.rs" "$CORPUS/src/strata.rs"
cp "$ROOT/std/arch.dl" "$CORPUS/std/arch.dl"
cp "$ROOT/std/suppress.dl" "$CORPUS/std/suppress.dl"
for rail in arch suppress; do
  cmp -s "$ROOT/std/$rail.dl" "$CORPUS/std/$rail.dl" \
    || fail "std/$rail.dl copy changed"
done
(cd "$CORPUS" && git init -q && git add -A \
  && git -c user.email=comment-rail@local -c user.name=comment-rail commit -qm pin) \
  || fail "could not pin parity corpus"
say "PASS  parity corpus: 5 Rust files; std/arch.dl and std/suppress.dl byte-identical"

cat >"$CORPUS/probe.dl" <<'EOF'
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
  (cd "$CORPUS" && DL_STATE_DIR="$WORK/state" "$DL" "$1" --db "$WORK/$1.sqlite" --no-daemon)
}
strip_v5() {
  grep -v '^?' "$1" | grep -vE '^ *\([0-9]+ rows?\) *$' | grep -v '^ *$'
}
run_v5 probe.dl >"$WORK/v5-comment.raw" 2>"$WORK/v5-comment.err" \
  || fail "v5 comment rail: $(tail -8 "$WORK/v5-comment.err")"
run_v5 probe_arch.dl >"$WORK/v5-arch.raw" 2>"$WORK/v5-arch.err" \
  || fail "v5 ARCH rail: $(tail -8 "$WORK/v5-arch.err")"
strip_v5 "$WORK/v5-comment.raw" | LC_ALL=C sort -u >"$WORK/v5-comment.tsv"
strip_v5 "$WORK/v5-arch.raw" | LC_ALL=C sort -u >"$WORK/v5-arch.tsv"

: >"$WORK/v6-comment.raw"
: >"$WORK/v6-arch.raw"
for file in $FILES; do
  (cd "$CORPUS" && "$EX" --family cst "$file" 2>/dev/null \
    | python3 "$CN" comments "$file") >>"$WORK/v6-comment.raw"
  (cd "$CORPUS" && python3 "$ARCH" "$file") \
    | python3 -c '
import json, sys
path = sys.argv[1]
for raw in sys.stdin:
    row = json.loads(raw)
    row["path"] = path
    print(json.dumps(row, separators=(",", ":")))
' "$file" >>"$WORK/v6-arch.raw"
done

python3 - "$WORK/v6-comment.raw" "$WORK/v6-comment.tsv" <<'PY'
import json
import sys

with open(sys.argv[1]) as source, open(sys.argv[2], "w") as target:
    for raw in source:
        if raw.strip():
            row = json.loads(raw)
            target.write("\t".join(str(row[key]) for key in (
                "path", "line", "col", "end_line", "end_col",
                "comment_text", "kind")) + "\n")
PY
LC_ALL=C sort -u "$WORK/v6-comment.tsv" -o "$WORK/v6-comment.tsv"

python3 - "$WORK/v6-comment.raw" "$WORK/v6-arch.raw" "$WORK/v6-arch.tsv" <<'PY'
import json
import sys

witness = set()
with open(sys.argv[1]) as source:
    for raw in source:
        if raw.strip():
            row = json.loads(raw)
            witness.add((row["path"], row["line"]))

with open(sys.argv[2]) as source, open(sys.argv[3], "w") as target:
    for raw in source:
        if raw.strip():
            row = json.loads(raw)
            if (row["path"], row["line"]) in witness:
                target.write("\t".join(str(row[key]) for key in (
                    "path", "line", "url")) + "\n")
PY
LC_ALL=C sort -u "$WORK/v6-arch.tsv" -o "$WORK/v6-arch.tsv"

grade() {
  local name="$1" v5="$2" v6="$3" n5 n6 shared only5 only6
  n5="$(wc -l <"$v5" | tr -d ' ')"
  n6="$(wc -l <"$v6" | tr -d ' ')"
  shared="$(comm -12 "$v5" "$v6" | wc -l | tr -d ' ')"
  only5="$(comm -23 "$v5" "$v6" | wc -l | tr -d ' ')"
  only6="$(comm -13 "$v5" "$v6" | wc -l | tr -d ' ')"
  say "$name v5=$n5 v6=$n6 shared=$shared only-v5=$only5 only-v6=$only6"
  [ "$only5" = 0 ] && [ "$only6" = 0 ] || return 1
}

grade comment_node "$WORK/v5-comment.tsv" "$WORK/v6-comment.tsv" \
  || fail "comment_node parity differs; work tree: $WORK"
grade arch_node "$WORK/v5-arch.tsv" "$WORK/v6-arch.tsv" \
  || fail "arch_node parity differs; work tree: $WORK"
say "COMMENT NODE PARITY HOLDS"
