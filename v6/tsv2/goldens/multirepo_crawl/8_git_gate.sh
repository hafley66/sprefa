#!/usr/bin/env bash
# 8_git_gate.sh -- THE REF/TAG/ANCESTRY GATE, over the SAME pinned corpus
# 2_gate.sh and 5_dep_gate.sh read, deepened by 6_history_corpus.sh.
#
# ─── WHAT THIS GATE GRADES AGAINST, WHICH IS THE WHOLE POINT ────────────────
# 2_gate.sh grades v6 against a pinned v5 golden, because both engines read the
# same tracked file bytes. There is no v5 golden here: v5 never projected a ref
# store or a commit DAG at all, so a pinned v5 capture would be an empty file.
# The oracle is instead the Git PLUMBING that Soopy itself shells to --
# `for-each-ref`, `merge-base --all`, `merge-base --is-ancestor`, and
# `rev-list --left-right --count`. Grading the authored dl6 against those four
# commands is what makes a wrong projection in `hosts.rs` visible: the arm and
# the oracle are only equal if the arm reads the fields it claims to read.
#
# That is a real grade and not a tautology. The arm does NOT run these command
# lines. It calls `soopy::Refs::snapshot` and `soopy::RevisionGraph::query`,
# which use a NUL-separated `for-each-ref` format, a `cat-file --batch` pass for
# tag messages, and peeling through `%(*objectname)`. The dumps below are
# written independently, in the spelling a person would reach for.
#
# ─── THE LEG THIS GATE RUNS ─────────────────────────────────────────────────
# The Rust runtime, not the served TS engine. 7_git_refs.dl6 compiles through
# emit_rust.pl and folds under `emit_rust_harness --live-hosts`, so the tick log
# below is the sprefa-engine-rs host path executing authored dl6.
#
# ─── THE LINKED-ARM RECEIPT ─────────────────────────────────────────────────
# All five `git_*` templates end in `exit 3`. If the host seam ever fell through
# to a shell the run would stop loudly rather than answer zero rows, so "the
# five ruled names execute in-process" is a fact this rig proves rather than a
# claim it repeats. Assertion 0 pins the templates so that receipt cannot rot.
#
# ─── SABOTAGE RECEIPTS (run 2026-08-16, all reverted) ───────────────────────
# 1  ARM REMOVED. Deleted the GIT_REF_HOSTS and GIT_REVISION_HOSTS branches in
#    hosts.rs:executor_for_plan, so the five names fall to ShellExecutor and the
#    templates actually run:
#      FAIL  the harness stopped: thread 'main' panicked at
#      src/bin/emit_rust_harness.rs:106:39: sh host 'git_ahead_behind': exited
#      exit status: 3: git_ahead_behind <corpus>/alpha HEAD feature is linked
#      in-process
#    That is what `exit 3` buys: the fall-through is loud, never zero rows.
# 2  MEMO KEY. Keyed the revision memo on `repo` alone (hosts.rs, the `pair`
#    key), so every repository's second pair reads its first pair's answer:
#      FAIL  the tick log carries no rows for reachable
#    The diverged pair answers zero ancestry rows, so the collapsed memo hands
#    those zero rows to the linear pair too and `reachable` empties out. The
#    empty-rel check fires before the grades because a rel that never arrives is
#    a different failure from a rel that arrives wrong. That is why the schedule
#    posts TWO pairs per repository with DIFFERENT answers: one pair each could
#    not tell a collapsed key from a correct one.
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
TSV2="$(cd "$HERE/../.." && pwd)"
REPO="$(cd "$TSV2/../.." && pwd)"
ENGINE="$REPO/v6/sprefa-engine-rs"
HARNESS="${DL_RUST_HARNESS:-$ENGINE/target/debug/emit_rust_harness}"
PROGRAM="$HERE/7_git_refs.dl6"
GOLDEN="$HERE/v5_golden"
MANIFEST="$GOLDEN/MANIFEST.tsv"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/tsv2-git-refs.XXXXXX")"
CORPUS="$WORK/corpus"
SLUGS="alpha beta gamma shared"

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

# The digest is asserted BEFORE 6_history_corpus.sh writes a second commit,
# because the pinned digest is of 1_corpus.sh's own tree.
[ -f "$MANIFEST" ] || fail "no pinned v5 golden at $MANIFEST; regenerate with $(manifest_field regenerate)"
want="$(manifest_field corpus_sha256)"
got="$(corpus_digest)"
[ "$want" = "$got" ] \
  || fail "1_corpus.sh's corpus MOVED since the v5 golden was captured (golden $want, now $got)"
say "PASS  pinned corpus: 4 repos at $CORPUS, digest holds against $MANIFEST"

bash "$HERE/6_history_corpus.sh" "$CORPUS" >"$WORK/history.log" 2>&1 \
  || fail "history deepening failed: $(cat "$WORK/history.log")"
say "PASS  $(cat "$WORK/history.log")"

# ── assertion 0: the templates that make the linked-arm claim falsifiable ────
template_exits="$(grep -c "is linked in-process' >&2; exit 3" "$PROGRAM")"
[ "$template_exits" = "5" ] \
  || fail "7_git_refs.dl6 has $template_exits of 5 templates ending in exit 3; the linked-arm receipt is gone"
say "PASS  all 5 git_* templates exit 3, so a shell fall-through cannot answer rows"

# ── compile the authored dl6 through the Rust emitter ───────────────────────
GENERATED="$WORK/git_refs.program.rs"
swipl -q -l "$REPO/v6/prolog/compile.pl" -l "$REPO/v6/prolog/emit_rust.pl" \
  -g "compile_dl6('$PROGRAM','$GENERATED',[emitter(emit_rust:emit_program)])" -g halt \
  >"$WORK/compile.log" 2>&1 || fail "emit_rust compile failed: $(tail -5 "$WORK/compile.log")"
[ -s "$GENERATED" ] || fail "emit_rust wrote no program"
say "PASS  7_git_refs.dl6 compiled through emit_rust.pl ($(wc -c <"$GENERATED" | tr -d ' ') bytes)"

if [ ! -x "$HARNESS" ]; then
  cargo build --quiet --manifest-path "$ENGINE/Cargo.toml" --bin emit_rust_harness \
    >"$WORK/build.log" 2>&1 || fail "harness build failed: $(tail -5 "$WORK/build.log")"
fi
[ -x "$HARNESS" ] || fail "no harness at $HARNESS"

# ── the run: one tick of want_refs and want_pair rows ───────────────────────
# TWO pairs per repository, with DIFFERENT answers: a diverged pair (2 ahead,
# 1 behind, no ancestry) and a linear one (0 ahead, 2 behind, one ancestry
# row). One pair per repository would not discriminate a collapsed memo.
python3 - "$CORPUS" $SLUGS >"$WORK/schedule.json" <<'PY'
import json, os, sys
root, slugs = sys.argv[1], sys.argv[2:]
tick = []
for slug in slugs:
    repo = os.path.join(root, slug)
    tick.append({"rel": "want_refs", "sign": "add", "row": [repo]})
    tick.append({"rel": "want_pair", "sign": "add", "row": [repo, "HEAD", "feature"]})
    tick.append({"rel": "want_pair", "sign": "add", "row": [repo, "v0.1.0", "HEAD"]})
json.dump([tick], sys.stdout)
PY

started="$(date +%s.%N)"
"$HARNESS" "$GENERATED" "$WORK/schedule.json" --live-hosts \
  >"$WORK/ticks.jsonl" 2>"$WORK/harness.err" \
  || fail "the harness stopped: $(tail -3 "$WORK/harness.err")"
ended="$(date +%s.%N)"
[ -s "$WORK/ticks.jsonl" ] || fail "the harness printed no tick log"

# Every rel's settled add-set, one TSV per rel. No rel in this program deletes,
# so the union of the adds IS the final table. Booleans render lowercase, which
# is the spelling `for-each-ref` comparisons below produce.
python3 - "$WORK" "$WORK/ticks.jsonl" <<'PY'
import json, pathlib, sys
work = pathlib.Path(sys.argv[1])
def cell(value):
    if isinstance(value, bool):
        return "true" if value else "false"
    return str(value)
rows = {}
for line in open(sys.argv[2]):
    line = line.strip()
    if not line.startswith("{"):
        continue
    for rel, delta in json.loads(line)["deltas"].items():
        if rel.startswith("__"):
            continue
        rows.setdefault(rel, set()).update(tuple(cell(c) for c in row) for row in delta["add"])
for rel, settled in rows.items():
    (work / f"git.{rel}.tsv").write_text(
        "".join("\t".join(row) + "\n" for row in sorted(settled)))
PY

RELS="repo_ref repo_tag fork_point divergence reachable head_commit branch_count released tag_in_history"
for rel in $RELS; do
  [ -f "$WORK/git.$rel.tsv" ] || fail "the tick log carries no rows for $rel"
done
rel_rows() { wc -l <"$WORK/git.$1.tsv" | tr -d ' '; }
say "PASS  refs settled in $(awk -v s="$started" -v e="$ended" 'BEGIN { printf "%.2fs", e - s }')"
for rel in $RELS; do
  printf 'ROWS  %-16s %s\n' "$rel" "$(rel_rows "$rel")"
done

# ── the Soopy-direct dumps, written from Git plumbing ───────────────────────
# `%(*objectname)` is the peeled target and is empty for anything but an
# annotated tag, so this is where the `direct`-versus-`peeled` choice shows.
for slug in $SLUGS; do
  repo="$CORPUS/$slug"
  git -C "$repo" for-each-ref \
    --format='%(refname)%09%(objecttype)%09%(objectname)%09%(*objectname)%09%(taggerdate:unix)' \
  | while IFS=$'\t' read -r name kind direct peeled when; do
      target="${peeled:-$direct}"
      case "$name" in
        refs/heads/*)   printf '%s\t%s\tbranch\t%s\n' "$repo" "$name" "$target" >>"$WORK/want.repo_ref.tsv" ;;
        refs/tags/*)    printf '%s\t%s\ttag\t%s\n' "$repo" "$name" "$target" >>"$WORK/want.repo_ref.tsv"
                        if [ "$kind" = "tag" ]; then
                          printf '%s\t%s\t%s\t%s\ttrue\n' "$repo" "${name#refs/tags/}" "$target" "$when" >>"$WORK/want.repo_tag.tsv"
                        else
                          printf '%s\t%s\t%s\t0\tfalse\n' "$repo" "${name#refs/tags/}" "$target" >>"$WORK/want.repo_tag.tsv"
                        fi ;;
        refs/remotes/*) printf '%s\t%s\tremote\t%s\n' "$repo" "$name" "$target" >>"$WORK/want.repo_ref.tsv" ;;
        *)              printf '%s\t%s\tother\t%s\n' "$repo" "$name" "$target" >>"$WORK/want.repo_ref.tsv" ;;
      esac
    done
  printf '%s\tHEAD\thead\t%s\n' "$repo" "$(git -C "$repo" rev-parse HEAD)" >>"$WORK/want.repo_ref.tsv"

  for pair in "HEAD feature" "v0.1.0 HEAD"; do
    set -- $pair
    left="$1"; right="$2"
    git -C "$repo" merge-base --all "$left" "$right" \
      | while IFS= read -r base; do
          printf '%s\t%s\t%s\t%s\n' "$repo" "$left" "$right" "$base" >>"$WORK/want.fork_point.tsv"
        done
    counts="$(git -C "$repo" rev-list --left-right --count "$left...$right")"
    printf '%s\t%s\t%s\t%s\t%s\n' "$repo" "$left" "$right" \
      "$(printf '%s' "$counts" | cut -f1)" "$(printf '%s' "$counts" | cut -f2)" \
      >>"$WORK/want.divergence.tsv"
    left_sha="$(git -C "$repo" rev-parse "$left^{commit}")"
    right_sha="$(git -C "$repo" rev-parse "$right^{commit}")"
    if git -C "$repo" merge-base --is-ancestor "$left_sha" "$right_sha"; then
      printf '%s\t%s\t%s\n' "$repo" "$left_sha" "$right_sha" >>"$WORK/want.reachable.tsv"
    fi
    if [ "$left_sha" != "$right_sha" ] \
       && git -C "$repo" merge-base --is-ancestor "$right_sha" "$left_sha"; then
      printf '%s\t%s\t%s\n' "$repo" "$right_sha" "$left_sha" >>"$WORK/want.reachable.tsv"
    fi
  done
done

# ── the grades: four rels, byte-diffed against the plumbing dump ────────────
identical=0; differing=0
grade() {
  local rel="$1"
  local want="$WORK/want.$rel.tsv" got="$WORK/git.$rel.tsv"
  [ -f "$want" ] || : >"$want"
  LC_ALL=C sort -u "$want" -o "$want"
  LC_ALL=C sort -u "$got" -o "$got"
  if cmp -s "$want" "$got"; then
    say "GRADE $rel: BYTE-IDENTICAL to the Git plumbing dump ($(wc -l <"$got" | tr -d ' ') rows)"
    identical=$((identical + 1))
  else
    differing=$((differing + 1))
    say "GRADE $rel: differs (want $(wc -l <"$want" | tr -d ' ') rows, got $(wc -l <"$got" | tr -d ' ') rows)"
    diff "$want" "$got" | sed 's/^/GRADE   /'
  fi
}

for rel in repo_ref repo_tag fork_point divergence reachable; do
  grade "$rel"
done

# ── the closure check: the derived rels against the two projected families ──
# repo_ref and repo_tag are graded above, so a derived rel that disagrees with
# them is a rule defect rather than a host defect.
python3 - "$WORK" <<'PY' || exit 1
import pathlib, sys
work = pathlib.Path(sys.argv[1])
def rows(rel):
    return [line.split("\t") for line in (work / f"git.{rel}.tsv").read_text().splitlines()]

broken = []
refs = rows("repo_ref")
want_heads = {(repo, name, sha) for repo, name, kind, sha in refs if kind in ("branch", "head")}
got_heads = {tuple(row) for row in rows("head_commit")}
if want_heads != got_heads:
    broken.append(f"head_commit is not the branch+head union ({want_heads ^ got_heads})")

want_branches = {}
for repo, _name, kind, _sha in refs:
    if kind == "branch":
        want_branches[repo] = want_branches.get(repo, 0) + 1
got_branches = {repo: int(count) for repo, count in rows("branch_count")}
if want_branches != got_branches:
    broken.append(f"branch_count disagrees with repo_ref ({want_branches} vs {got_branches})")

want_released = {(repo, tag, sha, when)
                 for repo, tag, sha, when, annotated in rows("repo_tag") if annotated == "true"}
got_released = {tuple(row) for row in rows("released")}
if want_released != got_released:
    broken.append(f"released is not the annotated-tag filter ({want_released ^ got_released})")

# A lightweight tag carries no tag object, so a tagged_at other than 0 on one
# would mean the arm invented a date.
dated_lightweight = [row for row in rows("repo_tag") if row[4] == "false" and row[3] != "0"]
if dated_lightweight:
    broken.append(f"a lightweight tag carries a tag date: {dated_lightweight}")

reach = {(repo, ancestor) for repo, ancestor, _descendant in rows("reachable")}
want_history = {(repo, tag, descendant)
                for repo, tag, sha, _when, _annotated in rows("repo_tag")
                for r_repo, r_ancestor, descendant in rows("reachable")
                if (r_repo, r_ancestor) == (repo, sha)}
got_history = {tuple(row) for row in rows("tag_in_history")}
if want_history != got_history:
    broken.append(f"tag_in_history is not repo_tag JOIN reachable ({want_history ^ got_history})")

for line in broken:
    print(f"GRADE   {line}")
print(f"GRADE derived closure: {'CLOSED' if not broken else 'BROKEN'} "
      f"({len(refs)} refs, {len(rows('repo_tag'))} tags, {len(reach)} ancestries)")
sys.exit(1 if broken else 0)
PY
closure=$?
[ "$closure" = "0" ] || differing=$((differing + 1))

[ "$differing" = "0" ] || fail "$differing ref/ancestry grades differ"
say "artifacts: $WORK"
say "REFS AND ANCESTRY GRADED: $identical/5 rels byte-identical against the Git plumbing dump, derived closure closed, 0 unclassified"
