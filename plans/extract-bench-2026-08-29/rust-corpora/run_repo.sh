#!/usr/bin/env bash
# rust-corpora lane: one repo, all arms, sequential. Usage: run_repo.sh <repo>
set -uo pipefail
REPO="$1"
ROOT="$HOME/corpora/$REPO"
R=/Users/chrishafley/projects/sprefa/.boop-worktrees/bench/extract-rust-corpora/plans/extract-bench-2026-08-29
OUT="$R/rust-corpora"
BIN="$R/../../v6/sprefa-extract/target/release/extract"
PROBE="$R/ra_ide_probe/target/release/ra_ide_probe"
FILES="$OUT/$REPO.files.txt"

mapfile -t PATHS < "$FILES"
ABS=(); for f in "${PATHS[@]}"; do ABS+=("$ROOT/$f"); done

# 1. oracle
mkdir -p "$OUT/$REPO.oracle"
/usr/bin/time -l timeout 900 nice -n 15 "$PROBE" "$ROOT" 900 "$OUT/$REPO.oracle" all \
  > "$OUT/$REPO.oracle.out" 2> "$OUT/$REPO.oracle.time.txt"
echo "oracle rc=$? wall=$(rg -o '[0-9]+\.[0-9]+ real' "$OUT/$REPO.oracle.time.txt" | head -1) rss=$(rg -o '[0-9]+ maximum resident set size' "$OUT/$REPO.oracle.time.txt" | head -1)"
cp "$OUT/$REPO.oracle/rust.oracle.callhier.tsv" "$OUT/$REPO.oracle.call.tsv"

# 2. diet arm
/usr/bin/time -l timeout 900 nice -n 15 "$BIN" --resolve --family call,type "${ABS[@]}" \
  > "$OUT/$REPO.diet.raw.jsonl" 2> "$OUT/$REPO.diet.time.txt"
echo "diet rc=$? wall=$(rg -o '[0-9]+\.[0-9]+ real' "$OUT/$REPO.diet.time.txt" | head -1) rss=$(rg -o '[0-9]+ maximum resident set size' "$OUT/$REPO.diet.time.txt" | head -1)"

# 3. checker arm
/usr/bin/time -l timeout 900 nice -n 15 "$BIN" --resolve --family call,type --rust-checker --project-root "$ROOT" "${ABS[@]}" \
  > "$OUT/$REPO.checker.raw.jsonl" 2> "$OUT/$REPO.checker.time.txt"
echo "checker rc=$? wall=$(rg -o '[0-9]+\.[0-9]+ real' "$OUT/$REPO.checker.time.txt" | head -1) rss=$(rg -o '[0-9]+ maximum resident set size' "$OUT/$REPO.checker.time.txt" | head -1)"

# 4. normalize + census
python3 "$R/normalize.py" resolved "$OUT/$REPO.diet.raw.jsonl" "$ROOT" "$OUT/$REPO.ours.call.tsv" "$OUT/$REPO.ours.type.tsv"
python3 "$R/normalize.py" resolved "$OUT/$REPO.checker.raw.jsonl" "$ROOT" "$OUT/$REPO.ours_check.call.tsv" "$OUT/$REPO.ours_check.type.tsv"
for arm in diet checker; do
  echo "$arm census: $(python3 -c "import json,collections;c=collections.Counter(json.loads(l)['record'] for l in open('$OUT/$REPO.$arm.raw.jsonl'));print(dict(c))")"
done

# 5. score
for arm in diet check; do
  otsv="$OUT/$REPO.oracle.call.tsv"
  o="$OUT/$REPO.ours$([ $arm = check ] && echo _check || echo '').call.tsv"
  echo "== $arm =="
  python3 "$R/fuzzy_bench.py" "$o" "$otsv" --lang rust --oracle-name rust.oracle.call.tsv --files "$FILES" --mode exact
done
