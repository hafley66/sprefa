---
name: parallel-worktree-workflow
description: "How to run parallel sub-agent worktrees in sprefa — stale-base gotcha, verify-before-merge discipline, worktree cleanup"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 9e8c04df-9cf4-4ae6-af6a-42f750539a6b
---

The user drives sprefa architecture work via multiple background sub-agents in isolated git worktrees, then has the main session reconcile + merge each to `main`.

**Stale-base gotcha:** the harness `isolation: "worktree"` Agent spawns get pinned to a stale base commit (observed: `15bb9bf`) regardless of where `main` actually is. An agent built on the wrong base silently regresses prerequisite work.
- **How to apply:** Do NOT rely on `isolation: "worktree"`. Pre-create the worktree yourself on the *current* main: `git worktree add -b <branch> .claude/worktrees/<name> <current-main-sha>`, then spawn a NON-isolated background agent whose prompt pins it to that exact path (`cd` there first) and includes a sanity assert (`git rev-parse --short HEAD` == expected base, else STOP). Always tell agents to self-abort on base mismatch rather than proceed.

**Verify-before-merge (the user's hard discipline):** never merge a sub-agent's branch on its summary alone — the summary states intent, not what landed.
- **How to apply:** before merging, check (1) `git merge-base <branch> main` == current main HEAD exactly, (2) `git show --stat` scope = only the expected files, (3) grep the commit diff for forbidden/regression patterns (e.g. reintroduced `.0.to_string()` / `parse::<u64>().ok()?` at typed boundaries; assertion/count/expected-value edits when the task was a non-test fix). Fast-forward merge only, then independently re-run the FULL test matrix on merged main before declaring done. Pre-existing failures must stay exactly equal, not grow.

**Worktree cleanup:** the user flagged Rust agent worktrees "butt chugging my hardrive." Clean each worktree + its branch (`git worktree remove -f -f` then `git branch -D`, then `git worktree prune`) as soon as its work merges. Preserve genuinely-unmerged branches; never mass-delete unverified worktrees (could lose work). Non-workflow session branches (`codex-*`, `claude/*`) are not yours to clean.

**No green-faking:** when fixing failing tests, only migrate stale syntax / fix real bugs; weakening or deleting assertions to force green is forbidden unless the old assertion is proven wrong. Verify test-fix diffs touch zero assertions/counts.

**Proactively isolate high-blast changes:** the user expects large/risky changes (tree-sitter grammar regen, core enum reshape like `Value`, binding-model flips) to be done in a dedicated worktree by default, WITHOUT being asked. When a plan touches grammar + a pervasive enum + lowering semantics, set up `git worktree add ../<name> -b <branch>` off current main and execute entirely there; only ff-merge back after full `cargo test` green in the worktree. Observed 2026-05-18: user rejected proceeding on `main` with "do all of this in a worktree bc there is huge change this fucks up".

Related: [[sprefa-genericization-initiative]]
