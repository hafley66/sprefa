# Worktree salvage 2026-08-02

Banked before removing every worktree fully merged into codex/rel-ref-file-span-lab
(721be80a + session-state commits). Apply any patch with `git apply <file>.patch`
from the repo root.

## Flash-vs-opus lanes (all 10 removed; opus branches were merged, flash never committed)
- REPORT-<task>-<model>.md — the 10 lane reports verbatim.
- lane-<task>-flash.patch — each flash lane's full uncommitted work (the unmerged
  alternate implementation; opus set was chosen and merged).

## Older merged-but-dirty trees (leftover docs/briefs/experiment results)
- base-dl-gitignore.patch — sprefa-base detached tree, one .dl/.gitignore mod.
- codex-{dlfix,flowres,m9storage,symfix}.patch — dispatch briefs, flow-parity
  residue findings doc, perf_report example.
- exp-g{2,3,6,8,9,10}*.patch — EXPERIMENT-G* docs/results + local src tweaks from
  the sprefa-store perf-harness era. g10's perf-runs.sqlite and twin-audit's
  .scratch/*.sqlite* binaries were dropped (scratch artifacts, not source).
- agent-{hermetic-state-v11,v11-work,twin-audit,snapshot-identity}.patch —
  .claude agent worktrees (old plans/ docs + engine drafts).
- extract-golden-plan-chatlogs.patch — chat_log files unique to that tree.

Not banked: agent-a4e42b1a71418cb1e's untracked .sandbox/ + sprefa/ (a
materialized copy of main's tree from the git-archive tunneling incident;
content already in git), lab-sqlperf's node_modules.

Kept alive (unmerged work): sprefa-codex-intern, sprefa-codex-qscip,
sprefa-wt-g4-unify, sprefa-lsp-claude-diags, sprefa-types, sprefa-refactor,
.claude/worktrees/{clock-checker-resume,agent-ae284db38a18d76eb,vscode-flow-panel},
and sprefa-flash-prolog (user ruling pending: redo/keep/drop).
