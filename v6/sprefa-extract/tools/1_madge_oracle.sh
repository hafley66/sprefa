#!/usr/bin/env bash
# THE MADGE ORACLE: v6's first external ground truth for a graph relation.
#
# v5 carried nine differential oracles (tests/it/oracle_*.rs) and ran zero of
# them by default: all fourteen test functions are #[ignore]d. The v6 extractor
# already runs scip-typescript, scip-go and rust-analyzer as ordinary
# non-ignored ratchets, but every one of those grades CALL resolution. Nothing
# on either side graded a MODULE graph on the default path.
#
# This script closes that: it diffs `extract --scip-deps` against madge's own
# dependency graph over a real TypeScript corpus, and prints a classified table.
#
# Usage:  tools/1_madge_oracle.sh <ts-project-root> [extract-binary]
#
# The corpus must have a tsconfig.json. Both tools are run over the same root,
# and the two graphs are compared as sets of (src, dst) project-relative pairs.
#
# TWO DIVERGENCE CLASSES ARE EXPECTED AND ARE NOT ERRORS. Both were measured
# over v6/tsv2 (212 files, madge 752 edges, scip 755, agreement 746):
#
#   CORPUS   madge walks the filesystem; scip-typescript indexes the tsconfig
#            PROGRAM. A directory the tsconfig `include` list omits is in
#            madge's graph and absent from scip's. The tools disagree about
#            what the corpus is, not about the graph.
#
#   SEMANTIC scip resolves through the type system, madge scans import syntax.
#            A file that reaches a declaration through an inferred type, with
#            no import statement naming it, is a scip edge and not a madge
#            edge. scip is right; this is the same shape as the flagship
#            callgraph result, where v6 is a strict superset on monotone
#            relations because it sees more than a syntactic query can.
#
# Exit 0 always: this is a REPORT, not a gate. Turning it into a gate means
# choosing a floor for agreement, which is a decision to make with numbers from
# more than one corpus.

set -euo pipefail

ROOT="${1:?usage: 1_madge_oracle.sh <ts-project-root> [extract-binary]}"
EXTRACT="${2:-target/release/extract}"

command -v madge >/dev/null || {
  echo "madge not on PATH: npm i -g madge, or pass its path via SPREFA_MADGE" >&2
  exit 1
}
[ -x "$EXTRACT" ] || { echo "no extract binary at $EXTRACT" >&2; exit 1; }
[ -f "$ROOT/tsconfig.json" ] || {
  echo "$ROOT has no tsconfig.json; scip-typescript indexes the tsconfig program" >&2
  exit 1
}

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

echo "corpus: $ROOT"

madge --extensions ts --json "$ROOT" > "$WORK/madge.json"
node -e '
  const graph = require(process.argv[1]);
  const rows = [];
  for (const [src, dsts] of Object.entries(graph))
    for (const dst of dsts) rows.push(`${src}\t${dst}`);
  process.stdout.write(rows.sort().join("\n") + "\n");
' "$WORK/madge.json" | LC_ALL=C sort -u > "$WORK/madge.tsv"

# One source path is enough to pick the indexer; --scip-deps covers every
# document the index holds regardless of which paths are named.
FIRST_TS="$(find "$ROOT" -name '*.ts' -not -path '*/node_modules/*' | head -1)"
"$EXTRACT" --scip-deps --project-root "$ROOT" --scip-build "$FIRST_TS" \
  > "$WORK/scip.jsonl"
node -e '
  const fs = require("fs");
  const rows = fs.readFileSync(process.argv[1], "utf8").trim().split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line))
    .map((row) => `${row.src_path}\t${row.dst_path}`);
  process.stdout.write(rows.sort().join("\n") + "\n");
' "$WORK/scip.jsonl" | LC_ALL=C sort -u > "$WORK/scip.tsv"

comm -12 "$WORK/madge.tsv" "$WORK/scip.tsv" > "$WORK/agree.tsv"
comm -23 "$WORK/madge.tsv" "$WORK/scip.tsv" > "$WORK/madge_only.tsv"
comm -13 "$WORK/madge.tsv" "$WORK/scip.tsv" > "$WORK/scip_only.tsv"

count() { wc -l < "$1" | tr -d ' '; }
MADGE=$(count "$WORK/madge.tsv")
SCIP=$(count "$WORK/scip.tsv")
AGREE=$(count "$WORK/agree.tsv")

echo
printf 'madge edges   %s\n' "$MADGE"
printf 'scip edges    %s\n' "$SCIP"
printf 'agree         %s\n' "$AGREE"
printf 'madge only    %s\n' "$(count "$WORK/madge_only.tsv")"
printf 'scip only     %s\n' "$(count "$WORK/scip_only.tsv")"
node -e '
  const [agree, madge, scip] = process.argv.slice(1).map(Number);
  const show = (label, value) =>
    console.log(`${label} ${Number.isFinite(value) ? value.toFixed(3) : "n/a"}`);
  show("recall vs madge  ", madge ? agree / madge : NaN);
  show("precision        ", scip ? agree / scip : NaN);
' "$AGREE" "$MADGE" "$SCIP"

echo
echo "-- madge only (expect: corpus the tsconfig excludes) --"
head -20 "$WORK/madge_only.tsv" || true
echo
echo "-- scip only (expect: inferred type references with no import) --"
head -20 "$WORK/scip_only.tsv" || true
