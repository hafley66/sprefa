#!/usr/bin/env bash
# 0_corpus.sh -- the SHARED pinned corpus, plus the second revision this rig needs.
#
# ../multirepo_crawl/1_corpus.sh is called BYTE-UNMODIFIED and owns commit A: four
# one-commit git repositories, each with a root go.mod, the designed version skew,
# and all.config.toml. Nothing here edits a go.mod, so the dependency crawl reads
# exactly the bytes 5_dep_gate.sh grades and `just multirepo-golden` keeps its own
# corpus digest.
#
# WHAT THIS FILE ADDS, and why each piece exists:
#
#   commit B per repo    combo 1 needs two revisions of one repository to set-
#                        difference. A single-commit corpus can only answer
#                        "added: everything", which no rig can be wrong about.
#   TypeScript sources   the extractor answers defs/refs/calls/df on .ts. The
#                        corpus's own main.go is Go, whose df value nodes are
#                        still start-only (`src/lang/go.rs` df_push `len: 0`), so
#                        a span-containment combo over .go would grade a known
#                        extractor gap instead of the join under test.
#   gamma deletes main.go  the removed-file arm of combo 1. Without a deletion the
#                        set difference is one-sided and `removed_path` is a rel
#                        that never ran.
#   shared gets NO .ts   the zero-extraction arm of combo 4. A repository the
#                        crawl visits and the extractor finds nothing in must be a
#                        NAMED row; a corpus where every repo answers rows cannot
#                        tell a zero row from a missing rule.
#
# THE CROSS-REPO NAME GRAPH (combo 5), designed so the join has something to be
# right and something to be wrong about:
#
#   alpha  defines shared_helper, alpha_only     called by nobody in alpha
#   beta   calls   shared_helper                 def lives in alpha       RESOLVES
#   gamma  calls   alpha_only                    def lives in alpha       RESOLVES
#   gamma  calls   ghost_call                    defined in NO repository UNRESOLVED
#   beta   defines beta_local, calls it          same repo                NEGATIVE CONTROL
#
# ghost_call is the negative control that a name-level chase cannot fake: a rig
# that reports every call site as resolved passes a corpus with no dangling name.
#
# REVS.tsv is the handoff. Every gate leg reads (slug, rev_a, rev_b, root) from it
# rather than re-deriving revisions, so one run's corpus is one set of pins.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
root="${1:?usage: 0_corpus.sh <destination-directory>}"

bash "$HERE/../multirepo_crawl/1_corpus.sh" "$root" >/dev/null

commit_b() {
  local slug="$1"
  (
    cd "$root/$slug"
    git add -A
    git -c user.email=rig@sprefa -c user.name=scip-combo-rig commit -q -m "scip combo corpus: $slug sources"
  )
}

rev_at() { git -C "$root/$1" rev-parse "$2"; }

# Commit A is what 1_corpus.sh left behind, read before anything is written.
declare -a rev_a=()
for slug in alpha beta gamma shared; do
  rev_a+=("$(rev_at "$slug" HEAD)")
done

mkdir -p "$root/alpha/src"
cat >"$root/alpha/src/core.ts" <<'TS'
export function shared_helper(count: number): number {
  const doubled = count * 2;
  return doubled + 1;
}

export function alpha_only(label: string): string {
  const trimmed = label.trim();
  return trimmed;
}
TS

mkdir -p "$root/beta/src"
cat >"$root/beta/src/use.ts" <<'TS'
export function beta_local(seed: number): number {
  return seed + 1;
}

export function beta_entry(seed: number): number {
  const stepped = beta_local(seed);
  return shared_helper(stepped);
}
TS

mkdir -p "$root/gamma/src"
cat >"$root/gamma/src/use.ts" <<'TS'
export function gamma_entry(label: string): string {
  const named = alpha_only(label);
  return ghost_call(named);
}
TS

# The deletion arm. main.go carries no go.mod fact, so the crawl is untouched.
rm "$root/gamma/main.go"

# The REWRITE arm: one path present at both revisions under two blob oids. Without
# it `rewritten_path` is a rel that never arrives, and a rel that never arrives
# cannot be graded either right or wrong.
printf 'package main\n\nfunc main() { println("alpha") }\n' >"$root/alpha/main.go"

# shared moves to commit B without gaining an extractable file: `.md` has no
# entry in sprefa-extract's `sources()` roster, so the extractor answers None on
# it. That is what makes shared the zero-extraction repository combo 4 must name.
printf 'shared is the crawl target every other repo requires.\n' >"$root/shared/NOTES.md"

for slug in alpha beta gamma shared; do
  commit_b "$slug"
done

# THE DIRTY WORKTREE, left uncommitted on purpose. Combo 2 asks whether pinning
# the FILE SET to a revision pins the EXTRACTION, and the answer is only visible
# when the bytes on disk differ from the bytes at HEAD: the extract host reads
# `{repo}/{path}` off the filesystem, so a HEAD-pinned demand still extracts the
# working copy. One uncommitted function is the whole apparatus for that.
cat >>"$root/beta/src/use.ts" <<'TS'

export function beta_uncommitted(seed: number): number {
  return beta_local(seed) * 3;
}
TS

# The dirty worktree's digest is a `git hash-object` oid. SprefaExtractExecutor
# now reads extract bytes by that oid, not the worktree, so the dirty blob must
# exist in the object database before either door runs. The engine's file-feed
# host stays read-only; this rig writes the one dirty blob itself.
git -C "$root/beta" hash-object -w src/use.ts >/dev/null

{
  index=0
  for slug in alpha beta gamma shared; do
    printf '%s\t%s\t%s\t%s\n' "$slug" "${rev_a[$index]}" "$(rev_at "$slug" HEAD)" "$root/$slug"
    index=$((index + 1))
  done
} >"$root/REVS.tsv"

printf 'corpus: 4 repos at %s, two revisions each, pins in %s/REVS.tsv\n' "$root" "$root"
