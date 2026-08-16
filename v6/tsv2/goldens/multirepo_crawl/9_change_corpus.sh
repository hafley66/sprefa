#!/usr/bin/env bash
# 9_change_corpus.sh -- DEEPEN the pinned corpus in $1 with a CHANGE PAIR.
#
# 1_corpus.sh builds one commit per repository and 6_history_corpus.sh adds a
# fork, two tag kinds and a known ahead/behind count. Neither produces a
# deletion, a modification, or a rename, so `git diff` between any two of their
# revisions is creations only. This script adds two commits on the trunk whose
# diff carries ALL FOUR change kinds at once, on top of both, so 11_change_gate
# .sh inherits 1_corpus.sh's corpus assertion and every gate reads the same four
# repositories.
#
# THE SHAPE, per repository, where C is 6_history_corpus.sh's trunk tip:
#
#   C ── E ── F   <default branch>
#        ^ refs/tags/change_base       ^ refs/tags/change_head
#
# WHAT THE E..F PAIR MAKES GRADABLE, per repository:
#   created    arrived.txt                          one row
#   deleted    doomed.txt                           one row
#   modified   lines.txt, blob.bin                  two rows, one of them binary
#   renamed    moves/origin.txt -> moves/dest.txt   one row, identical content
#   unchanged  every 1_corpus.sh file               ZERO rows, the control
#
# THE CONTROL IS THE POINT. A rail that fired on every tracked path would pass
# every "the row is present" check and only the untouched files catch it, which
# is why the gate compares SORTED ROW SETS rather than counts.
#
# THE BINARY FILE IS ON PURPOSE. `blob.bin` carries a NUL in its first bytes, so
# Git prints a header and NO hunk for it. It must produce a `modified` row and
# ZERO `changed_line` rows; a rig with only text files would pass a line
# projection that invented lines for binary content.
#
# THE RENAME CARRIES IDENTICAL CONTENT. `changed` is exact-content renames only,
# the `-M100%` spelling, so a rename contributes no changed line either.
#
# TAG NAMES AVOID `head`. macOS filesystems are case-insensitive, so a tag
# spelled `head` is ambiguous with `HEAD` and every lookup silently resolves to
# the checkout tip (docs/failure-modes.md, fixture-tag-name-head-collision).
#
# THE DATES ARE PINNED, as in 6_history_corpus.sh, so nothing here reads the
# wall clock.
set -euo pipefail

root="${1:?usage: 9_change_corpus.sh <corpus-directory built by 1_corpus.sh>}"
[ -d "$root" ] || { printf 'no corpus at %s; run 1_corpus.sh first\n' "$root" >&2; exit 1; }

WHEN='1700000000 +0000'

deepen() {
  local repo="$root/$1"
  [ -d "$repo/.git" ] || { printf 'no repository at %s\n' "$repo" >&2; exit 1; }
  (
    cd "$repo"
    export GIT_AUTHOR_NAME=multirepo-rig GIT_AUTHOR_EMAIL=rig@sprefa
    export GIT_COMMITTER_NAME=multirepo-rig GIT_COMMITTER_EMAIL=rig@sprefa
    export GIT_AUTHOR_DATE="$WHEN" GIT_COMMITTER_DATE="$WHEN"

    mkdir -p moves
    printf 'alpha\nbeta\ngamma\ndelta\nepsilon\n' > lines.txt
    printf 'this content never changes, only its path does\n' > moves/origin.txt
    printf 'this file is about to be removed\n' > doomed.txt
    printf '\000\001binary header\nbody\n' > blob.bin
    git add -A && git commit -q -m "change base"
    git tag change_base

    # line 2 rewritten and line 6 appended: two head-side lines, and the four
    # untouched lines are the per-line control.
    printf 'alpha\nBETA\ngamma\ndelta\nepsilon\nzeta\n' > lines.txt
    git mv moves/origin.txt moves/destination.txt
    git rm -q doomed.txt
    printf 'this file did not exist at the base\n' > arrived.txt
    printf '\000\001binary header\nbody changed\n' > blob.bin
    git add -A && git commit -q -m "change head"
    git tag change_head
  )
}

for slug in alpha beta gamma shared; do
  deepen "$slug"
done

printf 'change: 4 repos deepened at %s (change_base..change_head carries all four kinds)\n' "$root"
