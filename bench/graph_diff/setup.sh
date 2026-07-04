#!/bin/bash
# D0 setup: idempotent base-worktree creation for the PR graph-diff bench.
# See plans/2026-07-03-pr-diff-graph.md (D0 SPECIFICS).
set -euo pipefail

MAIN_REPO="$HOME/projects/sprefa"
BASE_ROOT="$HOME/projects/sprefa-base"
HEAD_ROOT="/Users/chrishafley/projects/sprefa/.claude/worktrees/vscode-flow-panel"

if [ -d "$BASE_ROOT" ]; then
  echo "base worktree already exists, skipping worktree add: $BASE_ROOT"
else
  git -C "$MAIN_REPO" worktree add --detach "$BASE_ROOT" main
fi

echo "base root: $BASE_ROOT"
echo "head root: $HEAD_ROOT"
