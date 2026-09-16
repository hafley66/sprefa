#!/bin/bash
# Remove drained lane worktrees + branches. Ancestor-gate each tip first.
set -u
REPO=/Users/chrishafley/projects/sprefa
PAIRS="
sprefa-duel-a-flash:lane/duel-a-flash
sprefa-duel-a-kimi:lane/duel-a-kimi
sprefa-duel-b-flash:lane/duel-b-flash
sprefa-duel-b-kimi:lane/duel-b-kimi
sprefa-plan-flash:lane/plan-flash
sprefa-plan-kimi:lane/plan-kimi
sprefa-plan-schemagen:lane/plan-schemagen
sprefa-plan-schemagen-flash:lane/plan-schemagen-flash
sprefa-research-schema:lane/research-schema
sprefa-reconcile-flash:lane/reconcile-flash
sprefa-lane-flash-a:lane/flash-effectcache-dump
sprefa-lane-flash-b:lane/flash-grading-determinism
sprefa-lanes/audit-hs:lane/audit-hs
sprefa-lanes/audit-pl:lane/audit-pl
sprefa-lanes/dl6-diag:lane/dl6-diag
sprefa-lanes/dl6-diag-fix:lane/dl6-diag-fix
sprefa-lanes/dl6-vscode:lane/dl6-vscode
sprefa-lanes/extract-drift:lane/extract-drift
sprefa-lanes/hs/demand:lane/hs-demand
sprefa-lanes/hs/graph:lane/hs-graph
sprefa-lanes/hs/idioms:lane/hs-idioms
sprefa-lanes/swi-scc:lane/swi-scc
"
for pair in $PAIRS; do
  rel=${pair%%:*}; BR=${pair##*:}
  WT=/Users/chrishafley/projects/$rel
  [ -d "$WT" ] || { echo "MISSING $WT"; continue; }
  TIP=$(git -C "$WT" rev-parse HEAD)
  if ! git -C "$REPO" merge-base --is-ancestor "$TIP" HEAD; then
    echo "SKIP $rel: tip $TIP not ancestor of HEAD"; continue
  fi
  if git -C "$REPO" worktree remove --force "$WT"; then
    if git -C "$REPO" branch -D "$BR" >/dev/null; then
      echo "OK   $rel ($TIP) removed, branch $BR deleted"
    else
      echo "WARN $rel removed but branch delete failed: $BR"
    fi
  else
    echo "FAIL worktree remove $rel"
  fi
done
rmdir /Users/chrishafley/projects/sprefa-lanes/hs 2>/dev/null
rmdir /Users/chrishafley/projects/sprefa-lanes 2>/dev/null
echo === remaining worktrees:
git -C "$REPO" worktree list
echo === remaining lane branches:
git -C "$REPO" branch --list 'lane/*'
