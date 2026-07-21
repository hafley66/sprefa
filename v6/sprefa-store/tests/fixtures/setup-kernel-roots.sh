#!/usr/bin/env bash
# Build the real-repo fixture for the spine acceptance test: THREE sibling
# checkout roots of the Linux kernel at ONE pinned rev, plus a WORK edit in one.
#
# Layout (a folder of 3 sibling checkout folders — none nested in another):
#   .fixtures/kernel-roots/
#     main/         detached checkout at $PINNED  (the primary working copy)
#     worktree/     detached checkout at $PINNED  (a git worktree, sibling)
#     background/   detached checkout at $PINNED  (a daemon-style background copy)
#                   ^ this one gets an uncommitted edit = its WORK state
#
# Each root is a `git worktree` of the base kernel clone, sparse-checked-out to
# `mm/` only (202 files) so 3 roots cost ~600 files on disk, not 3x93,698. All
# three are siblings under kernel-roots/; the worktree is NOT inside main/.
#
# Idempotent: re-running rebuilds cleanly. Gitignored output.
set -euo pipefail

PINNED="27fa82620cbaa89a7fc11ac3057701d598813e87"
SUBDIR="mm"
BASE="${LINUX_REPO:-$HOME/projects/ext/linux}"

here="$(cd "$(dirname "$0")" && pwd)"
crate_root="$(cd "$here/../.." && pwd)"
roots_dir="$crate_root/.fixtures/kernel-roots"

if [ ! -d "$BASE/.git" ]; then
  echo "SKIP: no Linux clone at $BASE (set LINUX_REPO=/path/to/linux)" >&2
  exit 3
fi

# tear down any prior worktrees at these paths
for name in main worktree background; do
  wt="$roots_dir/$name"
  if [ -d "$wt" ]; then
    git -C "$BASE" worktree remove --force "$wt" 2>/dev/null || rm -rf "$wt"
  fi
done
git -C "$BASE" worktree prune 2>/dev/null || true
rm -rf "$roots_dir"
mkdir -p "$roots_dir"

for name in main worktree background; do
  wt="$roots_dir/$name"
  git -C "$BASE" worktree add --no-checkout --detach "$wt" "$PINNED" >/dev/null
  git -C "$wt" sparse-checkout init --cone >/dev/null
  git -C "$wt" sparse-checkout set "$SUBDIR" >/dev/null
  git -C "$wt" checkout >/dev/null 2>&1
done

# The WORK edit: diverge ONE file in background/ from the committed HEAD.
edit_file="$roots_dir/background/$SUBDIR/util.c"
if [ -f "$edit_file" ]; then
  printf '\n/* sprefa v6 WORK-state edit: byte-diverged from HEAD */\n' >> "$edit_file"
fi

echo "$roots_dir"
