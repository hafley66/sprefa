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
# Usage:  tools/1_madge_oracle.sh <ts-project-root> [extract-binary] [resolver]
#
# resolver = scip (default) grades --scip-deps, the indexer fold.
# resolver = diet          grades --deps, the syntactic resolver in src/deps.rs.
# resolver = both          runs each in turn and prints the two tables.
#
# The corpus must have a tsconfig.json. Both tools are run over the same root,
# and the two graphs are compared as sets of (src, dst) project-relative pairs.
#
# THE DIET LEG IS EXPLICITLY ALLOWED TO LOSE. It has no type checker, so the
# SEMANTIC class below is out of its reach by construction; grading it against
# the same oracle on the same corpus is how that loss is measured instead of
# argued.
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

ROOT="${1:?usage: 1_madge_oracle.sh <ts-project-root> [extract-binary] [resolver]}"
EXTRACT="${2:-target/release/extract}"
RESOLVER="${3:-scip}"
case "$RESOLVER" in
  scip|diet|both) ;;
  *) echo "resolver must be scip, diet or both (got '$RESOLVER')" >&2; exit 1 ;;
esac

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

# Every edge stream is normalized the same way, so the two resolvers are graded
# by identical arithmetic and the tables are comparable line for line.
edges_tsv() {
  node -e '
    const fs = require("fs");
    const rows = fs.readFileSync(process.argv[1], "utf8").trim().split("\n")
      .filter(Boolean)
      .map((line) => JSON.parse(line))
      .map((row) => `${row.src_path}\t${row.dst_path}`);
    process.stdout.write(rows.sort().join("\n") + "\n");
  ' "$1" | LC_ALL=C sort -u
}

count() { wc -l < "$1" | tr -d ' '; }
MADGE=$(count "$WORK/madge.tsv")

grade() {
  local name="$1" jsonl="$2"
  edges_tsv "$jsonl" > "$WORK/$name.tsv"
  comm -12 "$WORK/madge.tsv" "$WORK/$name.tsv" > "$WORK/${name}_agree.tsv"
  comm -23 "$WORK/madge.tsv" "$WORK/$name.tsv" > "$WORK/${name}_madge_only.tsv"
  comm -13 "$WORK/madge.tsv" "$WORK/$name.tsv" > "$WORK/${name}_only.tsv"
  local mine agree
  mine=$(count "$WORK/$name.tsv")
  agree=$(count "$WORK/${name}_agree.tsv")

  echo
  echo "=== resolver: $name ==="
  printf 'madge edges   %s\n' "$MADGE"
  printf '%-13s %s\n' "$name edges" "$mine"
  printf 'agree         %s\n' "$agree"
  printf 'madge only    %s\n' "$(count "$WORK/${name}_madge_only.tsv")"
  printf '%-13s %s\n' "$name only" "$(count "$WORK/${name}_only.tsv")"
  node -e '
    const [agree, madge, mine] = process.argv.slice(1).map(Number);
    const show = (label, value) =>
      console.log(`${label} ${Number.isFinite(value) ? value.toFixed(3) : "n/a"}`);
    show("recall vs madge  ", madge ? agree / madge : NaN);
    show("precision        ", mine ? agree / mine : NaN);
  ' "$agree" "$MADGE" "$mine"

  echo
  echo "-- madge only (expect: corpus the tsconfig excludes) --"
  head -20 "$WORK/${name}_madge_only.tsv" || true
  echo
  echo "-- $name only (expect: what madge's import scan cannot see) --"
  head -20 "$WORK/${name}_only.tsv" || true
}

if [ "$RESOLVER" = scip ] || [ "$RESOLVER" = both ]; then
  # One source path is enough to pick the indexer; --scip-deps covers every
  # document the index holds regardless of which paths are named.
  FIRST_TS="$(find "$ROOT" -name '*.ts' -not -path '*/node_modules/*' | head -1)"
  "$EXTRACT" --scip-deps --project-root "$ROOT" --scip-build "$FIRST_TS" \
    > "$WORK/scip.jsonl"
  grade scip "$WORK/scip.jsonl"
fi

if [ "$RESOLVER" = diet ] || [ "$RESOLVER" = both ]; then
  # The diet resolver's universe IS its argument list, so every corpus file is
  # named. One process over the whole list, never one per file.
  find "$ROOT" -name '*.ts' -not -path '*/node_modules/*' -print0 \
    | xargs -0 "$EXTRACT" --deps --project-root "$ROOT" > "$WORK/diet.jsonl"
  grade diet "$WORK/diet.jsonl"
fi

if [ "$RESOLVER" = both ]; then
  echo
  echo "=== diet vs scip (same corpus, same oracle) ==="
  printf 'both resolvers  %s\n' "$(comm -12 "$WORK/scip.tsv" "$WORK/diet.tsv" | wc -l | tr -d ' ')"
  echo "-- scip only, diet missed (expect: inferred type refs, no import statement) --"
  comm -23 "$WORK/scip.tsv" "$WORK/diet.tsv" | head -20 || true
  echo "-- diet only, scip missed (expect: corpus the tsconfig excludes) --"
  comm -13 "$WORK/scip.tsv" "$WORK/diet.tsv" | head -20 || true
fi
