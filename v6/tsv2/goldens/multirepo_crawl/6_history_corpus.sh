#!/usr/bin/env bash
# 6_history_corpus.sh -- DEEPEN the pinned corpus in $1 with a ref history.
#
# 1_corpus.sh builds four repositories of ONE commit each, which is everything
# 2_gate.sh and 5_dep_gate.sh need: both read tracked file bytes, and a second
# commit would not change a go.mod line. A ref/tag/ancestry gate needs the
# opposite -- refs that branch, tags of both kinds, and a fork with a known
# ahead/behind count. This script adds that history ON TOP of the pinned
# corpus rather than replacing it, so 8_git_gate.sh inherits 1_corpus.sh's
# corpus assertion and the two gates read the same four repositories.
#
# THE SHAPE, per repository, where A is 1_corpus.sh's own commit:
#
#   A ── B ── C   <default branch>, refs/tags/v1.0.0 (annotated, at C)
#   └─── D        refs/heads/feature
#   ^ refs/tags/v0.1.0 (lightweight, at A)
#
# THE FACTS THAT MAKES GRADABLE, per repository:
#   merge-base(HEAD, feature)     = A          one row, not zero and not two
#   ahead/behind(HEAD, feature)   = 2 / 1      asymmetric, so a swapped
#                                              projection cannot pass
#   ancestor(v0.1.0, HEAD)        = true       one row
#   ancestor(HEAD, feature)       = neither    ZERO rows, the negative control
#   refs                          = 5          2 branches, 2 tags, HEAD
#   tags                          = 2          one annotated, one lightweight
#
# BOTH TAG KINDS ARE ON PURPOSE. A lightweight tag has no tag object and so no
# tagger date. A rig carrying only annotated tags would pass a `tagged_at`
# projection that invented a date for the lightweight case.
#
# THE DEFAULT BRANCH IS NEVER NAMED. `git init` reads `init.defaultBranch` from
# machine configuration, so this script and the gate both spell the trunk
# `HEAD` and read its ref name back with `git symbolic-ref` when they need it.
# A script that hard-codes `main` is green on one machine and red on another.
#
# THE DATES ARE PINNED. Every commit and the annotated tag carry a fixed
# timestamp, so `tagged_at` is a constant the gate can assert rather than a
# reading of the wall clock.
set -euo pipefail

root="${1:?usage: 6_history_corpus.sh <corpus-directory built by 1_corpus.sh>}"
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
    git tag v0.1.0
    git branch feature
    printf 'package main\n\nfunc second() {}\n' > second.go
    git add -A && git commit -q -m "second"
    printf 'package main\n\nfunc third() {}\n' > third.go
    git add -A && git commit -q -m "third"
    git tag -a v1.0.0 -m "the first release"
    git checkout -q feature
    printf 'package main\n\nfunc diverged() {}\n' > diverged.go
    git add -A && git commit -q -m "diverged"
    git checkout -q -
  )
}

for slug in alpha beta gamma shared; do
  deepen "$slug"
done

printf 'history: 4 repos deepened at %s (2 branches, 2 tags, 2-ahead/1-behind fork each)\n' "$root"
