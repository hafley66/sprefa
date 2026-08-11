# Worktree audit, 2026-08-11

`git worktree list` reports 55 entries (the brief noted 53; two more are
registered as of this run). Data captured against `origin/main`
=`a85a39c8`. Nothing was deleted; removal is the coordinator's call.

## Definitions used

| verdict | rule |
|---|---|
| MERGED | 0 commits ahead of main, empty `git status --short` |
| UNMERGED WORK | 1+ commit ahead OR 1+ dirty file |
| EMPTY | registered in `git worktree list` but the directory is gone |

LIVE marks a path under `.claude/worktrees/` whose branch starts
`worktree-agent-` (5 present). Reported, never touched.

## Table

| path | branch | ahead of main | dirty | last commit date | verdict |
|---|---|---|---|---|---|
| /Users/chrishafley/projects/sprefa | main | 0 | 13 | 2026-08-11 | UNMERGED WORK |
| /private/tmp/main-green | detached | - | - | - | EMPTY |
| /Users/chrishafley/.claude/jobs/eae95965/tmp/ci-drop-v5 | chore/ci-drop-v5-tests | 0 | 0 | 2026-08-11 | MERGED |
| /Users/chrishafley/projects/sprefa-dynamic-loading | lane/catalog-snake-case | 0 | 1 | 2026-08-07 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa-lanes/boop | lane/boop | 0 | 0 | 2026-08-09 | MERGED |
| /Users/chrishafley/projects/sprefa-lanes/catalogdecls | lane/catalogdecls | 0 | 3 | 2026-08-08 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa-lanes/catalogtype | lane/catalogtype | 0 | 5 | 2026-08-08 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa-lanes/doors | lane/enum-fix | 0 | 2 | 2026-08-08 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa-lanes/enumtype | lane/session-log-0808 | 0 | 0 | 2026-08-08 | MERGED |
| /Users/chrishafley/projects/sprefa-lanes/extract-prolog-refs | lane/extract-prolog-refs | 0 | 0 | 2026-08-09 | MERGED |
| /Users/chrishafley/projects/sprefa-lanes/kwargs-recon | lane/kwargs-recon | 1 | 0 | 2026-08-09 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa-lanes/listkind | lane/list-element-widening | 0 | 2 | 2026-08-08 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa-lanes/modscope-arc2 | lane/mount-arc | 0 | 0 | 2026-08-09 | MERGED |
| /Users/chrishafley/projects/sprefa-lanes/modscope-recon | lane/modscope-recon | 2 | 0 | 2026-08-09 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa-lanes/opt3vl-bench | lane/opt3vl-bench | 1 | 1 | 2026-08-09 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa-lanes/optindex | lane/opt-some-id-index | 0 | 0 | 2026-08-09 | MERGED |
| /Users/chrishafley/projects/sprefa-lanes/pack1 | lane/one-file-install | 0 | 0 | 2026-08-07 | MERGED |
| /Users/chrishafley/projects/sprefa-lanes/relplan | lane/b2-reload-planner | 0 | 4 | 2026-08-07 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa-lanes/smell-f1 | lane/smell-f1 | 0 | 3 | 2026-08-08 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa-lanes/smell-f2 | lane/smell-f2 | 0 | 3 | 2026-08-08 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa-lanes/smell-f3 | lane/smell-f3 | 0 | 3 | 2026-08-08 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa-lanes/smell-s1 | lane/smell-s1 | 0 | 3 | 2026-08-08 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa-lanes/smell-s2 | lane/smell-s2 | 0 | 3 | 2026-08-08 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa-lanes/smell-s3 | lane/smell-s3 | 0 | 3 | 2026-08-08 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa-lanes/snakecase | lane/snake-case-contract | 0 | 0 | 2026-08-07 | MERGED |
| /Users/chrishafley/projects/sprefa-lanes/tmuxvis | lane/tmuxvis | 1 | 1 | 2026-08-09 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa-lanes/typeplanedupe | lane/typeplanedupe | 0 | 1 | 2026-08-08 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa-lanes/usedoor | lane/a1a3-use-door | 0 | 5 | 2026-08-07 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa-lanes/variantfield | lane/variant-field-storage-type | 0 | 0 | 2026-08-08 | MERGED |
| /Users/chrishafley/projects/sprefa/.boop-worktrees/chore/ci-archmap-worktrees | chore/ci-archmap-worktrees | 0 | 1 | 2026-08-11 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa/.boop-worktrees/chore/json-list-term-rename | chore/json-list-term-rename | 0 | 1 | 2026-08-10 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa/.boop-worktrees/chore/mind-state-log | chore/mind-state-log | 1 | 0 | 2026-08-11 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa/.boop-worktrees/feature/generic-text-door | feature/generic-text-door | 0 | 3 | 2026-08-10 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa/.boop-worktrees/feature/pub-use | feature/pub-use | 0 | 2 | 2026-08-10 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa/.boop-worktrees/lab/generics-primitive | lab/generics-primitive | 0 | 8 | 2026-08-10 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa/.boop-worktrees/lab/list-flavors | lab/list-flavors | 1 | 4 | 2026-08-10 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa/.boop-worktrees/lane/boop-codex-subagent | lane/boop-codex-subagent | 0 | 1 | 2026-08-10 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa/.boop-worktrees/lane/boop-pstree | lane/boop-pstree | 0 | 1 | 2026-08-10 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa/.boop-worktrees/lane/boop-rows | lane/boop-rows | 0 | 1 | 2026-08-09 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa/.boop-worktrees/lane/schema-emit | lane/schema-emit | 0 | 1 | 2026-08-10 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa/.boop-worktrees/refactor/boop-mux-trait-features | refactor/boop-mux-trait-features | 0 | 1 | 2026-08-11 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa/.boop-worktrees/refactor/descriptor-families | refactor/descriptor-families | 2 | 1 | 2026-08-10 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa/.boop-worktrees/refactor/emit-serializers | refactor/emit-serializers | 0 | 1 | 2026-08-10 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa/.boop-worktrees/review/list-flavors | review/list-flavors | 1 | 1 | 2026-08-10 | UNMERGED WORK |
| /Users/chrishafley/projects/sprefa/.claude/worktrees/agent-a1237a7d94bba8cfe | worktree-agent-a1237a7d94bba8cfe | 0 | 5 | 2026-08-11 | LIVE · UNMERGED WORK |
| /Users/chrishafley/projects/sprefa/.claude/worktrees/agent-a352d47f8d9168b9e | worktree-agent-a352d47f8d9168b9e | 0 | 2 | 2026-08-11 | LIVE · UNMERGED WORK |
| /Users/chrishafley/projects/sprefa/.claude/worktrees/agent-a3adba9dd8d803707 | lane/modscope-arc1-dots | 0 | 0 | 2026-08-09 | MERGED |
| /Users/chrishafley/projects/sprefa/.claude/worktrees/agent-a6818f553bba0658a | lane/serve-reload-wire | 0 | 0 | 2026-08-09 | MERGED |
| /Users/chrishafley/projects/sprefa/.claude/worktrees/agent-a7dd44ce237005fef | lane/boop-harness-adapters | 0 | 0 | 2026-08-09 | MERGED |
| /Users/chrishafley/projects/sprefa/.claude/worktrees/agent-a99df2d5b7a3c222c | lane/type-ir-record | 0 | 0 | 2026-08-09 | MERGED |
| /Users/chrishafley/projects/sprefa/.claude/worktrees/agent-a9e3ac682390f78c5 | worktree-agent-a9e3ac682390f78c5 | 0 | 0 | 2026-08-10 | LIVE · MERGED |
| /Users/chrishafley/projects/sprefa/.claude/worktrees/agent-ab5f1a12702cb6e9a | worktree-agent-ab5f1a12702cb6e9a | 0 | 1 | 2026-08-10 | LIVE · UNMERGED WORK |
| /Users/chrishafley/projects/sprefa/.claude/worktrees/agent-adb68f4a70b768cfe | lane/boop-perf-grid | 0 | 0 | 2026-08-09 | MERGED |
| /Users/chrishafley/projects/sprefa/.claude/worktrees/agent-af25436b65fe54177 | worktree-agent-af25436b65fe54177 | 0 | 0 | 2026-08-09 | LIVE · MERGED |
| /Users/chrishafley/projects/sprefa/.claude/worktrees/agent-afaa394a80af55890 | lane/prolog-catalog-descriptors | 0 | 1 | 2026-08-10 | UNMERGED WORK |

## Summary

55 worktrees: 16 MERGED, 38 UNMERGED WORK, 1 EMPTY. Live agent worktrees not
touched: 5 (agent-a1237a7d94bba8cfe, agent-a352d47f8d9168b9e,
agent-a9e3ac682390f78c5, agent-ab5f1a12702cb6e9a, agent-af25436b65fe54177).

Disk, from `du -sh`:

| path | bytes |
|---|---|
| /Users/chrishafley/projects/sprefa/.boop-worktrees | 9.0G |
| /Users/chrishafley/projects/sprefa/.claude/worktrees | 12G |

Note: `.boop-worktrees/chore/ci-archmap-worktrees` (this worktree) reports 1
dirty because `sweep.sh` generated an untracked
`v6/prolog/compile/dl_view/option_list_column_roundtrips_null_and_present.dl6`.
