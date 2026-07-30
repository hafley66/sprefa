#!/usr/bin/env bash
# 1_corpus.sh -- build THE PINNED MULTI-REPO CORPUS into $1.
#
# Four tiny git repositories, each one commit, each with a root go.mod. The
# content is written literally here rather than copied out of the tree, so
# "the pinned corpus" is a fact in this file and not a function of what the
# machine happens to contain. This is the flagship rig's scratch-repo approach
# (flagship-callgraph.sh builds a one-commit repo and makes it the shared cwd)
# widened to several roots, which is what a multi-repo crawl needs.
#
# THE SKEW IS DESIGNED, so the grade has something to be right about:
#   example.com/shared     v1.2.0 in alpha and gamma, v1.3.0 in beta   SKEWED
#   github.com/pkg/errors  v0.9.1 in alpha, beta, shared; v0.8.0 gamma SKEWED
#   golang.org/x/sync      v0.7.0 in gamma only                        NOT skewed
# The unskewed module is not decoration -- it is the negative control. A rig
# that reports every module as skewed passes a corpus with no counter-example.
#
# SEPARATORS ARE SPACES, ON PURPOSE. go.mod in the wild indents requires with a
# TAB. Here the module and its version are separated by a single SPACE because
# the v6 leg's regex cannot contain a backslash (a backslash in a .dl6 string
# constant is eaten twice on the way to a host -- fixtures/v5-git-diags.dl6
# documents the measurement), so it spells the separator `[ ]+` where v5 spells
# it `\s+`. Pinning the separator keeps the two dialects selecting the same
# rows, and 2_gate.sh ASSERTS that rather than trusting it.
set -euo pipefail

root="${1:?usage: 1_corpus.sh <destination-directory>}"
mkdir -p "$root"

write_repo() {
  local slug="$1" gomod="$2"
  mkdir -p "$root/$slug"
  printf '%s' "$gomod" > "$root/$slug/go.mod"
  printf 'package main\n\nfunc main() {}\n' > "$root/$slug/main.go"
  (
    cd "$root/$slug"
    git init -q .
    git add -A
    git -c user.email=rig@sprefa -c user.name=multirepo-rig commit -q -m "pinned corpus: $slug"
  )
}

write_repo alpha 'module example.com/alpha

go 1.21

require (
example.com/shared v1.2.0
github.com/pkg/errors v0.9.1
)
'

write_repo beta 'module example.com/beta

go 1.21

require (
example.com/shared v1.3.0
github.com/pkg/errors v0.9.1
)
'

write_repo gamma 'module example.com/gamma

go 1.21

require (
example.com/shared v1.2.0
github.com/pkg/errors v0.8.0
golang.org/x/sync v0.7.0
)
'

write_repo shared 'module example.com/shared

go 1.21

require (
github.com/pkg/errors v0.9.1
)
'

# The v5 multi-repo config. `repo(slug, root, url)` rows come from here and
# nowhere else; this is what makes version-skew.dl a CROSS-REPO program rather
# than a single-tree one.
{
  for slug in alpha beta gamma shared; do
    printf '[[repos]]\nslug = "%s"\nroot = "%s/%s"\nurl = "https://example.invalid/%s"\n\n' \
      "$slug" "$root" "$slug" "$slug"
  done
} > "$root/all.config.toml"

printf 'corpus: 4 repos at %s\n' "$root"
