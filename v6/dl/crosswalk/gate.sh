#!/usr/bin/env bash
# @comment-ok: the rig's usage contract and its grading rules, one doc site.
# gate.sh -- the four multirepo_crawl golden programs on the RUST door.
#
#   bash v6/dl/crosswalk/gate.sh
#
# The programs are the arrival-form respellings under v6/dl/crosswalk/goldens
# (the paused tsv2 originals keep the dead `sh` spelling and are not compiled). The corpus builders there are corpus builders and not the paused
# runtime; they spell `git init` and `git commit` in shell, which is their
# business, and nothing in the engine does.
#
# NO PYTHON. 3_classify.py buckets differences through a third reading of the
# corpus bytes; here the comparison is `sort` / `comm` / `diff` over TSV, so the
# rig has no interpreter between the harness and the golden.
#
# THE ONE NAMED GAP is `dep_ver`. v5 writes the two witness versions with
# `min`/`max` over a version STRING and v6 stops by name
# (`aggregate_operand_not_number(min, _, text)`), so those two columns are not
# expressible today. The gate prints v5's three rows so what is missing is
# visible rather than absent.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
V6="$(cd "$HERE/../.." && pwd)"
REPO="$(cd "$V6/.." && pwd)"
ENGINE="$V6/sprefa-engine-rs"
GOLDEN="$HERE/goldens"
TSV2_CORPUS="$V6/tsv2/goldens/multirepo_crawl"
HARNESS="${DL_RUST_HARNESS:-$ENGINE/target/release/emit_rust_harness}"
TAB="$(printf '\t')"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/crosswalk-gate.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
say()  { printf '%s\n' "$*"; }
fail() { printf 'FAIL  %s\n' "$*" >&2; exit 1; }

identical=0
differing=0

# ── the corpus, all three deepenings, so one tree answers every leg ─────────
CORPUS="$WORK/corpus"
timeout 120 bash "$TSV2_CORPUS/1_corpus.sh" "$CORPUS" >"$WORK/corpus.log" 2>&1 \
  || fail "1_corpus.sh: $(tail -5 "$WORK/corpus.log")"
timeout 120 bash "$TSV2_CORPUS/6_history_corpus.sh" "$CORPUS" >>"$WORK/corpus.log" 2>&1 \
  || fail "6_history_corpus.sh: $(tail -5 "$WORK/corpus.log")"
timeout 120 bash "$TSV2_CORPUS/9_change_corpus.sh" "$CORPUS" >>"$WORK/corpus.log" 2>&1 \
  || fail "9_change_corpus.sh: $(tail -5 "$WORK/corpus.log")"
say "PASS  corpus: 4 repositories, deepened with a fork, two tag kinds and a change pair"

# ── build once, compile each program once ──────────────────────────────────
if [ ! -x "$HARNESS" ]; then
  timeout 900 cargo build --release --quiet --manifest-path "$ENGINE/Cargo.toml" \
    --bin emit_rust_harness >"$WORK/build.log" 2>&1 \
    || fail "harness build: $(tail -5 "$WORK/build.log")"
fi
[ -x "$HARNESS" ] || fail "no harness at $HARNESS"

# The saved compiler state, not 40 source modules: measured 0.25s end to end.
compile() {
  local program="$1"
  timeout 300 bash "$REPO/v6/prolog/compile/scripts/dl6c.sh" "$GOLDEN/$program.dl6" \
    --target rust --out "$WORK" >"$WORK/$program.compile.log" 2>&1 \
    || fail "$program compile: $(tail -5 "$WORK/$program.compile.log")"
  [ -s "$WORK/$program.rs" ] || fail "$program: the compiler wrote no program"
}

# One TSV line per row, `rel<TAB>col...`, so nothing here parses JSON.
run() {
  local program="$1" rels="$2"; shift 2
  timeout 300 "$HARNESS" "$WORK/$program.rs" \
    "$@" --live-hosts --final-only --final-tsv --final-rels "$rels" \
    >"$WORK/$program.tsv" 2>"$WORK/$program.err" \
    || fail "$program run: $(tail -5 "$WORK/$program.err")"
}

rows_of() { grep "^$1$TAB" "$WORK/$2.tsv" | cut -f2- || true; }
count_of() { rows_of "$1" "$2" | grep -c . || true; }

# The one grading primitive: two sorted TSVs, byte-compared.
grade() {
  local name="$1" want="$2" got="$3"
  LC_ALL=C sort -o "$want" "$want"
  LC_ALL=C sort -o "$got" "$got"
  if cmp -s "$want" "$got"; then
    identical=$((identical + 1))
    say "GRADE $name: BYTE-IDENTICAL ($(grep -c . <"$want" || true) rows)"
  else
    differing=$((differing + 1))
    say "GRADE $name: DIFFERS"
    diff "$want" "$got" | head -20 | sed 's/^/GRADE   /'
  fi
}

# ═══ leg 1: 0_multirepo_crawl, graded against the pinned v5 golden ══════════
compile 0_multirepo_crawl
SEEDS=()
for slug in alpha beta gamma shared; do
  SEEDS+=(--arrive "want_repo=$slug,$CORPUS/$slug,HEAD")
done
started="$(date +%s)"
run 0_multirepo_crawl 'dep_pin,skewed,skew_row,skew_width' "${SEEDS[@]}"
say "PASS  0_multirepo_crawl folded in $(( $(date +%s) - started ))s"

for rel in dep_pin skewed skew_row skew_width; do
  rows_of "$rel" 0_multirepo_crawl >"$WORK/got.$rel.tsv"
  cp "$TSV2_CORPUS/v5_golden/v5.$rel.tsv" "$WORK/want.$rel.tsv"
  grade "$rel" "$WORK/want.$rel.tsv" "$WORK/got.$rel.tsv"
done

say "GAP   dep_ver: v6 stops by name at aggregate_operand_not_number(min, _, text)."
say "GAP     min/max lower to a delta-compare against the stored extremum and the"
say "GAP     emitter carries only the numeric comparison, so a version STRING has"
say "GAP     no lowering. v5's three rows, so the missing thing is visible:"
sed 's/^/GAP     v5 /' "$TSV2_CORPUS/v5_golden/v5.dep_ver.tsv"

# ═══ leg 2: 4_dep_crawl, the frontier closure over the same go.mod bytes ════
compile 4_dep_crawl
CRAWL=()
for module in alpha beta gamma shared; do
  CRAWL+=(--arrive "want_crawl=$CORPUS,example.com/$module,go_mod")
done
run 4_dep_crawl 'crawl_visit,dep_target,corpus_boundary,crawl_reach' "${CRAWL[@]}"

# v5 pins repositories by SLUG and the crawl by MODULE PATH, which is what each
# repository's own go.mod declares, so the slug takes the module prefix back.
awk -F"$TAB" '{ print "example.com/" $1 "\t" $2 }' "$TSV2_CORPUS/v5_golden/v5.dep_pin.tsv" \
  | LC_ALL=C sort -u >"$WORK/want.dep_target.tsv"
rows_of dep_target 4_dep_crawl | LC_ALL=C sort -u >"$WORK/got.dep_target.tsv"
grade dep_target "$WORK/want.dep_target.tsv" "$WORK/got.dep_target.tsv"

# The boundary of the corpus is a relation: the modules v5 pins that no checkout
# under the root answers for.
cut -f2 "$TSV2_CORPUS/v5_golden/v5.dep_pin.tsv" | LC_ALL=C sort -u >"$WORK/v5.modules.tsv"
rows_of crawl_visit 4_dep_crawl | cut -f2 | LC_ALL=C sort -u >"$WORK/v6.reached.tsv"
LC_ALL=C comm -23 "$WORK/v5.modules.tsv" "$WORK/v6.reached.tsv" >"$WORK/want.boundary.tsv"
rows_of corpus_boundary 4_dep_crawl \
  | awk -F"$TAB" '$2 == "no_local_checkout" { print $1 }' \
  | LC_ALL=C sort -u >"$WORK/got.boundary.tsv"
grade corpus_boundary "$WORK/want.boundary.tsv" "$WORK/got.boundary.tsv"

# ═══ leg 3: 7_git_refs, oracle = 6_history_corpus.sh's own header ══════════
# v1.0.0 is annotated and dated, v0.1.0 is lightweight and carries no date.
compile 7_git_refs
REFS=()
for slug in alpha beta gamma shared; do
  REFS+=(--arrive "want_refs=$CORPUS/$slug")
  REFS+=(--arrive "want_pair=$CORPUS/$slug,v0.1.0,HEAD")
  REFS+=(--arrive "want_pair=$CORPUS/$slug,HEAD,feature")
done
run 7_git_refs 'repo_ref,repo_tag,branch_count,released,reachable,divergence' "${REFS[@]}"

for slug in alpha beta gamma shared; do
  printf '%s\tv1.0.0\t%s\n' "$slug" 1700000000
done >"$WORK/want.released.tsv"
rows_of released 7_git_refs \
  | awk -F"$TAB" -v root="$CORPUS/" '{ sub(root, "", $1); print $1 "\t" $2 "\t" $4 }' \
  >"$WORK/got.released.tsv"
grade released "$WORK/want.released.tsv" "$WORK/got.released.tsv"

# ancestor(v0.1.0, HEAD) is one row per repository; ancestor(HEAD, feature) is
# ZERO, the negative control, so four pairs answer four rows and not eight.
reachable_rows="$(count_of reachable 7_git_refs)"
[ "$reachable_rows" = "4" ] \
  || fail "reachable: 4 rows expected (one per repository, diverged tips answer none), got $reachable_rows"
say "GRADE reachable: 4 rows, the diverged pair answers none"
identical=$((identical + 1))

# ═══ leg 4: 10_change_facts, what a rev PAIR did to the tree ═══════════════
compile 10_change_facts
DIFFS=()
for slug in alpha beta gamma shared; do
  DIFFS+=(--arrive "want_diff=$CORPUS/$slug,change_base,change_head")
done
run 10_change_facts 'created,deleted,modified,renamed,changed_line,opaque_change' "${DIFFS[@]}"

{
  for slug in alpha beta gamma shared; do
    printf '%s\tcreated\tarrived.txt\n' "$slug"
    printf '%s\tdeleted\tdoomed.txt\n' "$slug"
    printf '%s\tmodified\tblob.bin\n' "$slug"
    printf '%s\tmodified\tlines.txt\n' "$slug"
    printf '%s\trenamed\tmoves/destination.txt\n' "$slug"
  done
} >"$WORK/want.kinds.tsv"
{
  for kind in created deleted modified; do
    rows_of "$kind" 10_change_facts \
      | awk -F"$TAB" -v root="$CORPUS/" -v k="$kind" '{ sub(root, "", $1); print $1 "\t" k "\t" $4 }'
  done
  rows_of renamed 10_change_facts \
    | awk -F"$TAB" -v root="$CORPUS/" '{ sub(root, "", $1); print $1 "\trenamed\t" $5 }'
} >"$WORK/got.kinds.tsv"
grade change_kinds "$WORK/want.kinds.tsv" "$WORK/got.kinds.tsv"

# A binary file produces its `modified` row and ZERO `changed_line` rows, which
# is what `opaque_change` names instead of leaving as a silent gap.
opaque="$(count_of opaque_change 10_change_facts)"
[ "$opaque" = "4" ] || fail "opaque_change: 4 rows expected (blob.bin per repository), got $opaque"
say "GRADE opaque_change: 4 rows, the binary blob carries no readable line"
identical=$((identical + 1))

[ "$differing" = "0" ] || fail "$differing grades differ"
say "CROSSWALK GATE: $identical/$identical grades identical, 1 named gap (dep_ver), 0 unclassified"
