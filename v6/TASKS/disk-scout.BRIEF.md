# disk-scout

## Goal
Disk is 777G used / 151G free on a 927G volume. Chris expects ~500G free.
Find where ~300G went. READ ONLY: no deletions, no moves.

## Method
`du -xsh` / `du -xd1` (never cross filesystems), sort by size, descend the top
entries until each row is a concrete path with a plain reason. Cover at least:
~/projects (per repo, and per repo its target/, node_modules/, .git/, .boop-worktrees/),
/private/tmp, ~/Library/Caches, ~/Library/Developer (Xcode DerivedData, simulators),
~/Library/Containers, ~/.cargo (registry, git), ~/.rustup, ~/.npm, ~/.pnpm-store,
~/Library/pnpm, ~/.cache, ~/.local, ~/.agent, /opt/homebrew, docker/colima/orbstack
data (`docker system df` if present), Time Machine local snapshots
(`tmutil listlocalsnapshots /`), ~/Downloads, ~/Movies, ~/.Trash, ~/projects/*archive*.
Use `sudo` only if it needs no password; else note "unreadable".
Keep every single du under 60s; if a subtree is slow, sample one level deeper and move on.

## Deliverable
A table: path / size / what it is / class (rebuildable cache | merged-lane worktree
target | archive | user data | unknown). Sorted by size. Then a second table:
the top candidates for reclaiming, with the size each frees and the exact rm or
tool command that would do it. Total the candidates. Report only; Chris deletes.
