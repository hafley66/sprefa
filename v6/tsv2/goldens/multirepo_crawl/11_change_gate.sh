#!/usr/bin/env bash
# 11_change_gate.sh -- THE CHANGE-FACT GATE, over the SAME pinned corpus
# 2_gate.sh, 5_dep_gate.sh and 8_git_gate.sh read, deepened by
# 6_history_corpus.sh and then by 9_change_corpus.sh.
#
# ─── WHAT THIS GATE GRADES AGAINST ──────────────────────────────────────────
# 2_gate.sh grades v6 against a pinned v5 golden because both engines read the
# same tracked file bytes. There is no v5 golden here: V5's `changed` and
# `changed_line` read the WORKTREE against HEAD (`src/rels/git.rs:16-20` and
# `:82-86`), and this family answers a REV PAIR, so a pinned v5 capture would be
# a different question rather than the same one. The oracle is instead the Git
# porcelain a person reaches for: `git diff --name-status -M100%` for the four
# kinds and `git diff -U0 -M100%` for the head-side lines.
#
# That is a real grade and not a tautology. The arm does NOT run these command
# lines. It calls `soopy::SourceTree::git_files` twice for the two listings,
# diffs the two path/OID maps itself, pairs exact-content renames itself, and
# runs `imara-diff` over blobs read through ONE `cat-file --batch`. The dumps
# below are written independently, in the spelling a person would reach for, so
# the arm and the oracle are equal only if the arm reads what it claims to.
#
# ─── THE LEG THIS GATE RUNS ─────────────────────────────────────────────────
# The Rust runtime, not the served TS engine. 10_change_facts.dl6 compiles
# through emit_rust.pl and folds under `emit_rust_harness --live-hosts`, so the
# tick log below is the sprefa-engine-rs host path executing authored dl6.
#
# ─── THE LINKED-ARM RECEIPT ─────────────────────────────────────────────────
# All three `git_*` templates end in `exit 3`. If the host seam ever fell
# through to a shell the run would stop loudly rather than answer zero rows.
# Assertion 0 pins the templates so that receipt cannot rot.
#
# ─── SABOTAGE RECEIPTS (run 2026-08-16, all reverted) ───────────────────────
# 1  ARM REMOVED. Deleted the GIT_CHANGE_HOSTS branch in
#    hosts.rs:executor_for_plan, so the three names fall to ShellExecutor and
#    the templates actually run:
#      FAIL  the harness stopped: thread 'main' panicked at
#      src/bin/emit_rust_harness.rs:106:39: sh host 'git_change': exited exit
#      status: 3: git_change <corpus>/alpha change_base change_head is linked
#      in-process
#    That is what `exit 3` buys: the fall-through is loud, never zero rows.
# 2  RENAME PASS DROPPED. Returned an empty Vec from `take_renames`:
#      FAIL  the tick log carries no rows for renamed
#    The empty-rel check fires before the grades, because a rel that never
#    arrives is a different failure from a rel that arrives wrong.
# 3  MEMO KEY. Keyed the diff memo on `repo` alone (hosts.rs, the `diff` key),
#    so every repository's second pair reads its first pair's answer:
#      GRADE created: differs (want 28 rows, got 8 rows)
#      GRADE   < .../alpha  v0.1.0  change_base  lines.txt        (and 5 more)
#      GRADE   > .../alpha  v0.1.0  change_base  arrived.txt
#    The v0.1.0 pair is served the change_base pair's answer. That is why the
#    schedule posts TWO pairs per repository with DIFFERENT answers: one pair
#    each could not tell a collapsed key from a correct one.
# 4  BINARY GUARD DROPPED. Removed the `is_binary` early-out in
#    `changed_lines_of`, so the NUL-bearing blob is diffed as text and stops
#    being the only modified path with no line:
#      FAIL  the tick log carries no rows for opaque_change
#
# THE HARNESS IS REBUILT EVERY RUN, and receipts 1 through 4 are why. The first
# attempt at receipt 1 came back GREEN: the gate skipped `cargo build` whenever
# a binary already sat in target/, so the edited arm was never compiled and the
# run graded a stale executable (docs/failure-modes.md, gate-graded-stale-arm).
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
TSV2="$(cd "$HERE/../.." && pwd)"
REPO="$(cd "$TSV2/../.." && pwd)"
ENGINE="$REPO/v6/sprefa-engine-rs"
HARNESS="${DL_RUST_HARNESS:-$ENGINE/target/debug/emit_rust_harness}"
PROGRAM="$HERE/10_change_facts.dl6"
GOLDEN="$HERE/v5_golden"
MANIFEST="$GOLDEN/MANIFEST.tsv"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/tsv2-change-facts.XXXXXX")"
CORPUS="$WORK/corpus"
SLUGS="alpha beta gamma shared"

# The two pairs, and they must have DIFFERENT answers. `change_base..change_head`
# carries all four kinds; `v0.1.0..change_base` carries creations only.
PAIRS="change_base:change_head v0.1.0:change_base"

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

# The digest is asserted BEFORE either deepener writes a second commit, because
# the pinned digest is of 1_corpus.sh's own tree.
[ -f "$MANIFEST" ] || fail "no pinned v5 golden at $MANIFEST; regenerate with $(manifest_field regenerate)"
want="$(manifest_field corpus_sha256)"
got="$(corpus_digest)"
[ "$want" = "$got" ] \
  || fail "1_corpus.sh's corpus MOVED since the v5 golden was captured (golden $want, now $got)"
say "PASS  pinned corpus: 4 repos at $CORPUS, digest holds against $MANIFEST"

bash "$HERE/6_history_corpus.sh" "$CORPUS" >"$WORK/history.log" 2>&1 \
  || fail "history deepening failed: $(cat "$WORK/history.log")"
say "PASS  $(cat "$WORK/history.log")"

bash "$HERE/9_change_corpus.sh" "$CORPUS" >"$WORK/change.log" 2>&1 \
  || fail "change deepening failed: $(cat "$WORK/change.log")"
say "PASS  $(cat "$WORK/change.log")"

# ── assertion 0: the templates that make the linked-arm claim falsifiable ────
template_exits="$(grep -c "is linked in-process' >&2; exit 3" "$PROGRAM")"
[ "$template_exits" = "3" ] \
  || fail "10_change_facts.dl6 has $template_exits of 3 templates ending in exit 3; the linked-arm receipt is gone"
say "PASS  all 3 git_* templates exit 3, so a shell fall-through cannot answer rows"

# ── compile the authored dl6 through the Rust emitter ───────────────────────
GENERATED="$WORK/change_facts.program.rs"
swipl -q -l "$REPO/v6/prolog/compile.pl" -l "$REPO/v6/prolog/emit_rust.pl" \
  -g "compile_dl6('$PROGRAM','$GENERATED',[emitter(emit_rust:emit_program)])" -g halt \
  >"$WORK/compile.log" 2>&1 || fail "emit_rust compile failed: $(tail -5 "$WORK/compile.log")"
[ -s "$GENERATED" ] || fail "emit_rust wrote no program"
say "PASS  10_change_facts.dl6 compiled through emit_rust.pl ($(wc -c <"$GENERATED" | tr -d ' ') bytes)"

# The harness is REBUILT every run, never reused when it merely exists. A gate
# that skips the build grades whatever binary was last left in target/, so an
# edited arm is invisible to it and every sabotage receipt below would be a lie.
if [ -z "${DL_RUST_HARNESS:-}" ]; then
  cargo build --quiet --manifest-path "$ENGINE/Cargo.toml" --bin emit_rust_harness \
    >"$WORK/build.log" 2>&1 || fail "harness build failed: $(tail -5 "$WORK/build.log")"
fi
[ -x "$HARNESS" ] || fail "no harness at $HARNESS"

# ── the run: one tick of want_diff rows ─────────────────────────────────────
python3 - "$CORPUS" "$PAIRS" $SLUGS >"$WORK/schedule.json" <<'PY'
import json, os, sys
root, pairs, slugs = sys.argv[1], sys.argv[2].split(), sys.argv[3:]
tick = []
for slug in slugs:
    repo = os.path.join(root, slug)
    for pair in pairs:
        base, head = pair.split(":")
        tick.append({"rel": "want_diff", "sign": "add", "row": [repo, base, head]})
json.dump([tick], sys.stdout)
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
    (work / f"change.{rel}.tsv").write_text(
        "".join("\t".join(row) + "\n" for row in sorted(settled)))
PY

RELS="created deleted modified renamed changed changed_line opaque_change new_line changed_line_count"
for rel in $RELS; do
  [ -f "$WORK/change.$rel.tsv" ] || fail "the tick log carries no rows for $rel"
done
say "PASS  change facts settled in $(awk -v s="$started" -v e="$ended" 'BEGIN { printf "%.2fs", e - s }')"
for rel in $RELS; do
  printf 'ROWS  %-20s %s\n' "$rel" "$(wc -l <"$WORK/change.$rel.tsv" | tr -d ' ')"
done

# ── the Git-porcelain dumps, written independently ──────────────────────────
# `-M100%` is the exact-content rename spelling the arm implements: a file that
# moved AND changed stays a deletion plus a creation on both sides.
for slug in $SLUGS; do
  repo="$CORPUS/$slug"
  for pair in $PAIRS; do
    base="${pair%%:*}"; head="${pair##*:}"
    git -C "$repo" diff --name-status -M100% "$base" "$head" \
      | while IFS=$'\t' read -r status left right; do
          case "$status" in
            A)    printf '%s\t%s\t%s\t%s\n' "$repo" "$base" "$head" "$left" >>"$WORK/want.created.tsv" ;;
            D)    printf '%s\t%s\t%s\t%s\n' "$repo" "$base" "$head" "$left" >>"$WORK/want.deleted.tsv" ;;
            M)    printf '%s\t%s\t%s\t%s\n' "$repo" "$base" "$head" "$left" >>"$WORK/want.modified.tsv" ;;
            R100) printf '%s\t%s\t%s\t%s\t%s\n' "$repo" "$base" "$head" "$left" "$right" >>"$WORK/want.renamed.tsv" ;;
            *)    printf 'unclassified %s status %s for %s\n' "$repo" "$status" "$left" >>"$WORK/want.unclassified" ;;
          esac
        done

    # The head-side line numbers, straight off the unified diff. A `+++
    # /dev/null` names a deletion and has no head side; a binary change prints
    # no `@@` at all, which is why blob.bin contributes nothing here.
    git -C "$repo" diff -U0 -M100% "$base" "$head" \
      | awk -v repo="$repo" -v base="$base" -v head="$head" '
          /^\+\+\+ /  { path = substr($0, 5); sub(/^b\//, "", path);
                        if (path == "/dev/null") path = ""; next }
          /^@@ /      { if (path == "") next
                        field = $3; sub(/^\+/, "", field)
                        split(field, parts, ",")
                        start = parts[1] + 0
                        count = (parts[2] == "") ? 1 : parts[2] + 0
                        for (line = start; line < start + count; line++)
                          printf "%s\t%s\t%s\t%s\t%d\n", repo, base, head, path, line }
        ' >>"$WORK/want.changed_line.tsv"
  done
done
[ -f "$WORK/want.unclassified" ] \
  && fail "the porcelain printed a status this gate does not classify: $(cat "$WORK/want.unclassified")"

# ── the grades: five rels, byte-diffed against the porcelain dump ───────────
identical=0; differing=0
grade() {
  local rel="$1"
  local want="$WORK/want.$rel.tsv" got="$WORK/change.$rel.tsv"
  [ -f "$want" ] || : >"$want"
  LC_ALL=C sort -u "$want" -o "$want"
  LC_ALL=C sort -u "$got" -o "$got"
  if cmp -s "$want" "$got"; then
    say "GRADE $rel: BYTE-IDENTICAL to the Git porcelain dump ($(wc -l <"$got" | tr -d ' ') rows)"
    identical=$((identical + 1))
  else
    differing=$((differing + 1))
    say "GRADE $rel: differs (want $(wc -l <"$want" | tr -d ' ') rows, got $(wc -l <"$got" | tr -d ' ') rows)"
    diff "$want" "$got" | sed 's/^/GRADE   /'
  fi
}

for rel in created deleted modified renamed changed_line; do
  grade "$rel"
done

# ── the closure check: the derived rels against the five graded ones ────────
python3 - "$WORK" <<'PY' || exit 1
import pathlib, sys
work = pathlib.Path(sys.argv[1])
def rows(rel):
    return [line.split("\t") for line in (work / f"change.{rel}.tsv").read_text().splitlines()]

broken = []
key = lambda row: tuple(row[:3])

want_changed = (
    {tuple(row) for row in rows("created")}
    | {tuple(row) for row in rows("deleted")}
    | {tuple(row) for row in rows("modified")}
    | {(*key(row), row[4]) for row in rows("renamed")})
got_changed = {tuple(row) for row in rows("changed")}
if want_changed != got_changed:
    broken.append(f"changed is not the four-kind union ({want_changed ^ got_changed})")

lined = {(*key(row), row[3]) for row in rows("changed_line")}
want_opaque = {tuple(row) for row in rows("modified")} - lined
got_opaque = {tuple(row) for row in rows("opaque_change")}
if want_opaque != got_opaque:
    broken.append(
        f"opaque_change is not modified minus the line-carrying paths ({want_opaque ^ got_opaque})")
# A binary change is the only way to be modified and carry no line, so an empty
# opaque_change would mean the rig lost its binary file.
if not got_opaque:
    broken.append("opaque_change is empty; the corpus lost its binary change")

created_paths = {(*key(row), row[3]) for row in rows("created")}
want_new_line = {tuple(row) for row in rows("changed_line") if (*key(row), row[3]) in created_paths}
got_new_line = {tuple(row) for row in rows("new_line")}
if want_new_line != got_new_line:
    broken.append(f"new_line is not created JOIN changed_line ({want_new_line ^ got_new_line})")

want_counts = {}
for row in rows("changed_line"):
    want_counts[(*key(row), row[3])] = want_counts.get((*key(row), row[3]), 0) + 1
got_counts = {(*key(row), row[3]): int(row[4]) for row in rows("changed_line_count")}
if want_counts != got_counts:
    broken.append(f"changed_line_count disagrees with changed_line ({want_counts} vs {got_counts})")

# The control: 1_corpus.sh's own files are tracked at both revisions of the
# first pair and untouched by 9_change_corpus.sh, so a rail firing on every
# tracked path fails right here.
touched = {row[3] for row in rows("changed")} | {row[4] for row in rows("renamed")}
for path in ("go.mod", "main.go"):
    if path in touched:
        broken.append(f"the untouched control file {path} appears in changed")

for line in broken:
    print(f"GRADE   {line}")
print(f"GRADE derived closure: {'CLOSED' if not broken else 'BROKEN'} "
      f"({len(rows('changed'))} changed, {len(rows('changed_line'))} lines, "
      f"{len(got_opaque)} opaque)")
sys.exit(1 if broken else 0)
PY
closure=$?
[ "$closure" = "0" ] || differing=$((differing + 1))

[ "$differing" = "0" ] || fail "$differing change-fact grades differ"
say "artifacts: $WORK"
say "CHANGE FACTS GRADED: $identical/5 rels byte-identical against the Git porcelain dump, derived closure closed, 0 unclassified"
