#!/usr/bin/env bash
# Reclaim disk from the fleet: remove clean worktrees already merged into
# origin/main, then delete cargo target dirs inside every remaining worktree.
# Dry run by default; --apply does it.
set -euo pipefail
repo="$(git rev-parse --show-toplevel)"
apply=0; [ "${1:-}" = "--apply" ] && apply=1
cd "$repo"
git fetch -q origin

removed=0
while read -r path; do
  [ "$path" = "$repo" ] && continue
  branch="$(git -C "$path" rev-parse --abbrev-ref HEAD 2>/dev/null || true)"
  [ -z "$branch" ] || [ "$branch" = "HEAD" ] && continue
  git merge-base --is-ancestor "$branch" origin/main 2>/dev/null || continue
  [ -z "$(git -C "$path" status --short 2>/dev/null)" ] || continue
  if [ $apply = 1 ]; then git worktree remove --force "$path" && removed=$((removed+1))
  else echo "would remove $path [$branch]"; fi
done < <(git worktree list --porcelain | awk '/^worktree /{print $2}')
git worktree prune

targets=0
while read -r dir; do
  if [ $apply = 1 ]; then rm -rf "$dir" && targets=$((targets+1))
  else echo "would delete $(du -sh "$dir" | cut -f1) $dir"; fi
done < <(find "$repo/.boop-worktrees" "$HOME/projects/sprefa-wt" -maxdepth 4 -type d -name target 2>/dev/null)

[ $apply = 1 ] && echo "removed worktrees: $removed, deleted target dirs: $targets"
df -h / | tail -1
