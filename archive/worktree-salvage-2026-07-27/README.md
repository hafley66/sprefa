# Worktree salvage + removal, 2026-07-27

Reconcile pass over `git worktree list`: 42 worktrees found, ledger claimed ~1.
Every merged tree's uncommitted work is captured here as a `git apply`-able
patch (created via `add -A` then `diff --cached --binary HEAD`, so untracked
files are included). Patches restore against the tree's HEAD commit, column 3.

## Salvaged (13 patches)

| patch | branch | tree HEAD | uncommitted content |
|---|---|---|---|
| exp-g8-mmap-kv | exp/g8-mmap-kv | 5d3339dd | mmap-kv experiment: 2 result docs, candidates doc, 1_mmap_kv.rs, redb_count.rs |
| exp-g2-dred-speed | exp/g2-dred-speed | 1aa36502 | EXPERIMENT-G2 + result, cascade.rs changes |
| exp-g3-knob-sweep | exp/g3-knob-sweep | c1c40709 | EXPERIMENT-G3-KNOBS, cascade.rs changes |
| exp-g10-golden-data | exp/g10-golden-data | 2310e7bf | G10 golden-data docs, cascade.rs, perf_report.rs, perf-runs.csv. perf-runs.sqlite (337MB raw perf runs) EXCLUDED as regenerable |
| exp-g6-scc-floor | exp/g6-scc-floor | 916e3d5a | perf_report.rs tweak |
| exp-g9-chat-tool | exp/g9-chat-tool | 015c4784 | EXPERIMENT-G9-CHAT-TOOL.md |
| codex-m9-storage-plane | codex/m9-storage-plane | 3047d6cb | perf_report.rs example draft |
| codex-dl-selfdefense | codex/dl-selfdefense | 5df7abf6 | DLFIX_BRIEF.txt |
| codex-symfix | codex/symfix | 05c3242a | SYMFIX_BRIEF.txt, TRIAGE_BRIEF.txt |
| hermetic-state-v11 | hermetic-state-v11 | 88a05ec5 | plans/2026-07-21-hermetic-state-isolation.md |
| v11-work | v11-work | d75e576e | cold_stage/mod/tick engine changes |
| docs-syntax-examples | worktree-agent-ac82718cb39bd3740 | 24782c3d | docs/reference syntax+examples edits (.scratch excluded) |
| snapshot-identity | snapshot-identity | 24782c3d | src/engine/snapshot.rs, db/meta/spine changes, plans/2026-07-20-snapshot-identity.md |

## Skipped salvage (junk-only dirt)

- sprefa-base (detached 2c6711d8): one-line `.dl/.gitignore` modification
- sprefa-lab-sqlperf (lab/sqlite-retract-perf): `node_modules/` only
- agent-a4e42b1a71418cb1e: `.sandbox/` (99MB) + stray nested `sprefa/` checkout

## Removal blocked: run this yourself

`git worktree remove` was denied by the session permission classifier, both
forced and unforced. Every tree below is fully merged into main and its
uncommitted content is in this directory. To finish:

```sh
cd ~/projects/sprefa
# clean trees (no --force needed)
for wt in ~/.claude/jobs/f43b8b80/tmp/head-check \
  ~/projects/sprefa-codex-{split,q1,q2,q3,q3design,q15,q16,q17,sym,typed-template} \
  .claude/worktrees/dl-m9-{before,core} \
  ~/projects/sprefa-wt-{g4-v2,g5-scc,g7-wire,sweep} \
  .claude/worktrees/agent-a1ace28435cc61066; do git worktree remove "$wt"; done
# dirty trees (salvaged above; --force discards the dirt)
for wt in ~/projects/sprefa-wt-{g8-mmap,g2-dred-speed,g3-knob-sweep,g10-golden,g6-floor,g9-chat} \
  ~/projects/sprefa-codex-{m9storage,dlfix,symfix} \
  .claude/worktrees/agent-{acdb456f0d8ffb293,aae639dd9de4689b8,ac82718cb39bd3740,a197d3ef86d92ecca,a4e42b1a71418cb1e} \
  ~/projects/sprefa-base ~/projects/sprefa-lab-sqlperf; do git worktree remove --force "$wt"; done
git worktree prune
# branches whose trees died and whose tips are ancestors of main
git branch -d codex/engine-mod-split codex/feedback-batch1 codex/feedback-batch2 \
  codex/feedback-batch3 codex/feedback-designs codex/final-q15 codex/final-q16 \
  codex/final-q17 codex/text-is-sym codex/typed-template-bootstrap dl/m9-before \
  dl/m9-core exp/g4-unify-v2 exp/g5-scc-counting exp/g7-wire-engines exp/reach-sweep \
  exp/g8-mmap-kv exp/g2-dred-speed exp/g3-knob-sweep exp/g10-golden-data \
  exp/g6-scc-floor exp/g9-chat-tool codex/m9-storage-plane codex/dl-selfdefense \
  codex/symfix hermetic-state-v11 v11-work snapshot-identity lab/sqlite-retract-perf \
  worktree-agent-a1ace28435cc61066 worktree-agent-a4e42b1a71418cb1e \
  worktree-agent-ac82718cb39bd3740
```

## Kept alive (unmerged, not touched)

| tree | branch | ahead of main | note |
|---|---|---|---|
| sprefa-lsp-claude-diags | feat/lsp-diags-to-claude-code | 12 | parked feature |
| sprefa-types | feat/type-ir-value-space | 1 | parked |
| sprefa-codex-intern | codex/df-intern | 2 | parked |
| sprefa-codex-qscip | codex/final-qscip | 3 | parked |
| sprefa-wt-g4-unify | exp/g4-unify-harness | 1 | parked |
| sprefa-refactor | refactor/file-splits | 7 | parked |
| .claude/worktrees/vscode-flow-panel | worktree-vscode-flow-panel | 5 | known parked (ledger) |
| .claude/worktrees/extract-golden-plan | plan/extract-golden-plan | 76 + 4 dirty chat_logs | whole parked arc; decide merge-or-kill separately |
