---
name: project_repo_lift_root
description: v5 lifted to repo root 2026-07-01; v3/v4 + root-era docs archived; ALL v5/ paths are now root-relative
metadata: 
  node_type: memory
  type: project
  originSessionId: 33b32503-a5e3-4068-bf31-2f4638b4145e
---

The sprefa repo root IS the dl crate as of 2026-07-01 (commits 4c9662f/295983e/
74913b9/d6254ba, local until Chris pushes). `v5/` no longer exists: src/, examples/,
tests/, docs/, book/, bench/, anim/, std/ all live at the root. Any older memory or
doc that says `v5/<path>` now means `<path>`.

**Why:** past v4, Chris wants one mainline; v3/v4 working trees + MAIN.md/TASKS.md/
human-goals.md/llm-notes.md/arch SVGs went to `~/projects/sprefa-archive-20260701`
(8.9MB, targets stripped) alongside the older `sprefa-archive-20260428`. The dead
18GB v5cozokuzu experiment was deleted (source archived).

**How to apply:** root Cargo.toml = the sprefa-dl package (workspace members =
["tree-sitter-dl"], exclude .claude). The agg gate test roots at
CARGO_MANIFEST_DIR itself. gen-doc-index.dl scans a brace glob
`{*.md,docs/**,book/**,plans/**,research/**}` to avoid swallowing chat_log/.
chat_log/ + plans/ were deliberately NOT path-rewritten (historical record).
Worktree branches cut pre-lift (feat/dl-mcp-lattice, feat/type-ir-value-space,
feat/lsp-diags-to-claude-code) need a rebase across the rename before merging.
install.sh is binary-only by default now; `dl setup` / `--setup` is explicit
opt-in (never mutate agent config implicitly — Chris's rule, [[feedback_ask_before_mutations]]).
