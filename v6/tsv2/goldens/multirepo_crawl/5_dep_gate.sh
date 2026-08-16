#!/usr/bin/env bash
# 5_dep_gate.sh -- THE DEPENDENCY-CRAWL GATE, graded against the SAME pinned v5
# golden 2_gate.sh uses, over the SAME pinned corpus.
#
# 2_gate.sh asks each repository for its pins with a line regex, one posted
# `want_repo` row per repository. This gate asks a different question of the
# same bytes: starting from ONE seed coordinate, which repositories does the
# corpus REACH. The repo set is the ANSWER here, not the input.
#
# ─── WHY THE TWO ANSWERS ARE COMPARABLE, WHICH IS THE WHOLE GRADE ───────────
# Both engines are reading the same go.mod require lines. v5 projects them as
# `dep_pin(repo, module, ver)`; the crawl projects them as `dep_target(from_repo,
# target)`, the union of the edges it resolved to a checkout and the targets it
# could not. Drop v5's version column and map its slug to the module path its
# own go.mod declares, and the two sets must be EQUAL, byte for byte. Nothing
# new is pinned: the v5 side is v5_golden/v5.dep_pin.tsv, captured for 2_gate.sh.
#
# ─── THE LEG THIS GATE RUNS ─────────────────────────────────────────────────
# The Rust runtime, not the served TS engine. 4_dep_crawl.dl6 compiles through
# emit_rust.pl and folds under `emit_rust_harness --live-hosts`, so the tick
# log below is the sprefa-engine-rs host path executing authored dl6.
#
# ─── THE LINKED-ARM RECEIPT ─────────────────────────────────────────────────
# All four `dep_crawl_*` templates end in `exit 3`. If the host seam ever fell
# through to a shell the run would produce ZERO rows and this gate would go red
# on every grade at once, so "the four ruled names execute in-process" is a
# fact this rig proves rather than a claim it repeats. Assertion 0 pins the
# templates so that receipt cannot rot.
#
# ─── SABOTAGE RECEIPTS (run 2026-08-16, both reverted) ──────────────────────
# 1  ARM REMOVED. Deleted the DEP_CRAWL_HOSTS branch in hosts.rs:executor_for_plan,
#    so the four names fall to ShellExecutor and the templates actually run:
#      FAIL  the harness stopped: ... panicked ...
#      sh host 'dep_crawl_edge': exited exit status: 3: dep_crawl_edge <corpus>
#      example.com/alpha go_mod is linked in-process
#    That is what `exit 3` buys: the fall-through is loud, never zero rows.
# 2  FRONTIER PATTERN. Put GoModFrontier's `^[ \t]*` back to the `^\s+` it
#    carried before this corpus (require lines at column 0) was fed to it. Every
#    crawl then closes at its own seed with no targets at all:
#      FAIL  the tick log carries no rows for crawl_edge
#    The empty-rel check fires before the grades because a rel that never
#    arrives is a different failure from a rel that arrives wrong.
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
TSV2="$(cd "$HERE/../.." && pwd)"
REPO="$(cd "$TSV2/../.." && pwd)"
ENGINE="$REPO/v6/sprefa-engine-rs"
HARNESS="${DL_RUST_HARNESS:-$ENGINE/target/debug/emit_rust_harness}"
PROGRAM="$HERE/4_dep_crawl.dl6"
GOLDEN="$HERE/v5_golden"
MANIFEST="$GOLDEN/MANIFEST.tsv"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/tsv2-dep-crawl.XXXXXX")"
CORPUS="$WORK/corpus"
SEEDS="example.com/alpha example.com/beta example.com/gamma example.com/shared"

fail() { printf 'FAIL  %s\n' "$*"; exit 1; }
say() { printf '%s\n' "$*"; }

manifest_field() { awk -F'\t' -v key="$1" '$1 == key { print $2 }' "$MANIFEST"; }

corpus_digest() {
  (
    cd "$CORPUS" || exit 1
    find . -type f -not -path '*/.git/*' -not -name all.config.toml \
      | LC_ALL=C sort \
      | while IFS= read -r file; do printf '%s\n' "$file"; cat "$file"; done
  ) | shasum -a 256 | cut -d' ' -f1
}

bash "$HERE/1_corpus.sh" "$CORPUS" >"$WORK/corpus.log" 2>&1 \
  || fail "corpus build failed: $(cat "$WORK/corpus.log")"

# The v5 side is 2_gate.sh's pinned capture, so this gate inherits its corpus
# assertion verbatim: a golden diffed against a moved corpus is a false green.
[ -f "$MANIFEST" ] || fail "no pinned v5 golden at $MANIFEST; regenerate with $(manifest_field regenerate)"
want="$(manifest_field corpus_sha256)"
got="$(corpus_digest)"
[ "$want" = "$got" ] \
  || fail "1_corpus.sh's corpus MOVED since the v5 golden was captured (golden $want, now $got)"
say "PASS  pinned corpus: 4 repos at $CORPUS, digest holds against $MANIFEST"

# ── assertion 0: the templates that make the linked-arm claim falsifiable ────
template_exits="$(grep -c "is linked in-process' >&2; exit 3" "$PROGRAM")"
[ "$template_exits" = "4" ] \
  || fail "4_dep_crawl.dl6 has $template_exits of 4 templates ending in exit 3; the linked-arm receipt is gone"
say "PASS  all 4 dep_crawl_* templates exit 3, so a shell fall-through cannot answer rows"

# ── compile the authored dl6 through the Rust emitter ───────────────────────
GENERATED="$WORK/dep_crawl.program.rs"
swipl -q -l "$REPO/v6/prolog/compile.pl" -l "$REPO/v6/prolog/emit_rust.pl" \
  -g "compile_dl6('$PROGRAM','$GENERATED',[emitter(emit_rust:emit_program)])" -g halt \
  >"$WORK/compile.log" 2>&1 || fail "emit_rust compile failed: $(tail -5 "$WORK/compile.log")"
[ -s "$GENERATED" ] || fail "emit_rust wrote no program"
say "PASS  4_dep_crawl.dl6 compiled through emit_rust.pl ($(wc -c <"$GENERATED" | tr -d ' ') bytes)"

if [ ! -x "$HARNESS" ]; then
  cargo build --quiet --manifest-path "$ENGINE/Cargo.toml" --bin emit_rust_harness \
    >"$WORK/build.log" 2>&1 || fail "harness build failed: $(tail -5 "$WORK/build.log")"
fi
[ -x "$HARNESS" ] || fail "no harness at $HARNESS"

# ── the run: one tick of want_crawl rows, then the host response tick ───────
python3 - "$CORPUS" $SEEDS >"$WORK/schedule.json" <<'PY'
import json, sys
root, seeds = sys.argv[1], sys.argv[2:]
json.dump([[{"rel": "want_crawl", "sign": "add", "row": [root, seed, "go_mod"]}
            for seed in seeds]], sys.stdout)
PY

started="$(date +%s.%N)"
"$HARNESS" "$GENERATED" "$WORK/schedule.json" --live-hosts \
  >"$WORK/ticks.jsonl" 2>"$WORK/harness.err" \
  || fail "the harness stopped: $(tail -3 "$WORK/harness.err")"
ended="$(date +%s.%N)"
[ -s "$WORK/ticks.jsonl" ] || fail "the harness printed no tick log"

# Every rel's settled add-set, one TSV per rel. No rel in this program deletes,
# so the union of the adds IS the final table.
python3 - "$WORK" "$WORK/ticks.jsonl" <<'PY'
import json, pathlib, sys
work = pathlib.Path(sys.argv[1])
rows = {}
for line in open(sys.argv[2]):
    line = line.strip()
    if not line.startswith("{"):
        continue
    for rel, delta in json.loads(line)["deltas"].items():
        if rel.startswith("__"):
            continue
        rows.setdefault(rel, set()).update(tuple(str(cell) for cell in row) for row in delta["add"])
for rel, settled in rows.items():
    (work / f"crawl.{rel}.tsv").write_text(
        "".join("\t".join(row) + "\n" for row in sorted(settled)))
PY

rel_rows() { [ -f "$WORK/crawl.$1.tsv" ] && wc -l <"$WORK/crawl.$1.tsv" | tr -d ' ' || printf '0\n'; }
for rel in crawl_repo crawl_visit crawl_edge crawl_gap dep_target corpus_boundary crawl_reach; do
  [ -f "$WORK/crawl.$rel.tsv" ] || fail "the tick log carries no rows for $rel"
done
say "PASS  crawl settled in $(awk -v s="$started" -v e="$ended" 'BEGIN { printf "%.2fs", e - s }'), \
repo:visit:edge:gap = $(rel_rows crawl_repo):$(rel_rows crawl_visit):$(rel_rows crawl_edge):$(rel_rows crawl_gap)"

# ── GRADE 1: the crawl's targets against v5's pins over the same bytes ──────
awk -F'\t' '{ print "example.com/" $1 "\t" $2 }' "$GOLDEN/v5.dep_pin.tsv" \
  | LC_ALL=C sort -u >"$WORK/v5.dep_target.tsv"
LC_ALL=C sort -u "$WORK/crawl.dep_target.tsv" >"$WORK/v6.dep_target.tsv"
identical=0; differing=0
if cmp -s "$WORK/v5.dep_target.tsv" "$WORK/v6.dep_target.tsv"; then
  say "GRADE dep_target: BYTE-IDENTICAL to v5 dep_pin ($(wc -l <"$WORK/v5.dep_target.tsv" | tr -d ' ') rows)"
  identical=$((identical + 1))
else
  differing=$((differing + 1))
  say "GRADE dep_target: differs (v5 $(wc -l <"$WORK/v5.dep_target.tsv" | tr -d ' ') rows, crawl $(wc -l <"$WORK/v6.dep_target.tsv" | tr -d ' ') rows)"
  diff "$WORK/v5.dep_target.tsv" "$WORK/v6.dep_target.tsv" | sed 's/^/GRADE   /'
fi

# ── GRADE 2: the boundary of the corpus is a relation, and it is the modules
# v5 pins that no checkout under the root answers for ────────────────────────
cut -f2 "$GOLDEN/v5.dep_pin.tsv" | LC_ALL=C sort -u >"$WORK/v5.modules.tsv"
awk -F'\t' '$2 == "no_local_checkout" { print $1 }' "$WORK/crawl.corpus_boundary.tsv" \
  | LC_ALL=C sort -u >"$WORK/v6.boundary.tsv"
awk -F'\t' '{ print $4 }' "$WORK/crawl.crawl_edge.tsv" | LC_ALL=C sort -u >"$WORK/v6.resolved.tsv"
LC_ALL=C comm -23 "$WORK/v5.modules.tsv" "$WORK/v6.resolved.tsv" >"$WORK/want.boundary.tsv"
if cmp -s "$WORK/want.boundary.tsv" "$WORK/v6.boundary.tsv"; then
  say "GRADE corpus_boundary: BYTE-IDENTICAL to v5's unreachable modules ($(wc -l <"$WORK/v6.boundary.tsv" | tr -d ' ') rows)"
  sed 's/^/GRADE   outside the checkout root: /' "$WORK/v6.boundary.tsv"
  identical=$((identical + 1))
else
  differing=$((differing + 1))
  say "GRADE corpus_boundary: differs (want $(wc -l <"$WORK/want.boundary.tsv" | tr -d ' ') rows, got $(wc -l <"$WORK/v6.boundary.tsv" | tr -d ' ') rows)"
  diff "$WORK/want.boundary.tsv" "$WORK/v6.boundary.tsv" | sed 's/^/GRADE   /'
fi

# ── GRADE 3: the frontier CLOSED. Every edge's `to_repo` is a repository the
# same seed's crawl visited, and every reach count is that seed's visited set.
python3 - "$WORK" <<'PY' || exit 1
import pathlib, sys
work = pathlib.Path(sys.argv[1])
def rows(rel):
    text = (work / f"crawl.{rel}.tsv").read_text()
    return [line.split("\t") for line in text.splitlines()]
visited = {}
for seed, repo, _rev, hop in rows("crawl_visit"):
    visited.setdefault(seed, {})[repo] = int(hop)
dangling = [(seed, to_repo) for seed, _from, _rev, _target, to_repo in rows("crawl_edge")
            if to_repo not in visited.get(seed, {})]
for seed, to_repo in dangling:
    print(f"GRADE   dangling edge: seed {seed} names {to_repo}, which it never visited")
seeded = [seed for seed, at in visited.items() if at.get(seed) != 0]
for seed in seeded:
    print(f"GRADE   seed {seed} is not at hop 0 in its own visited set")
mismatched = [(seed, int(count), len(visited.get(seed, {})))
              for seed, count in rows("crawl_reach") if len(visited.get(seed, {})) != int(count)]
for seed, count, size in mismatched:
    print(f"GRADE   {seed} reached {count} repos, its visited set has {size}")
broken = len(dangling) + len(seeded) + len(mismatched)
print(f"GRADE crawl closure: {'CLOSED' if broken == 0 else 'BROKEN'} "
      f"({len(visited)} seeds, {sum(len(at) for at in visited.values())} visits, "
      f"{len(rows('crawl_edge'))} edges)")
sys.exit(1 if broken else 0)
PY
closure=$?
[ "$closure" = "0" ] || differing=$((differing + 1))

[ "$differing" = "0" ] || fail "$differing dependency-crawl grades differ"
say "artifacts: $WORK"
say "DEPENDENCY CRAWL GRADED: $identical/2 rels byte-identical against the pinned v5 golden, frontier closed, 0 unclassified"
