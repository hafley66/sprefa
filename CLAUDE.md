# sprefa

Reactive datalog-over-code engine ("dl"), living at the **repo root** (v5 lifted
2026-07-01): SQLite-welded, facts extracted via `scan`+`regex`/`ast`/`sg`/`json`,
recursive rules lower to a SQL fixpoint. Prior iterations: v3/v4 working trees in
`~/projects/sprefa-archive-20260701` (also full git history); the OG coordinate
model (strings/refs/byte-spans) in `~/projects/sprefa-archive-20260428`.

User-facing overview (model, DSL surface, CLI, examples, known gaps): **`README.md`**.

Deep state lives in auto-memory (`project_v5_dl_engine`, etc.) + `chat_log/` session
logs + `plans/`. This file is the standing task ledger only.

**Completed-arc history** (85 landed items, full detail) lives in
`.agent/memories/sprefa-task-ledger.md` — read it on demand, not auto-loaded. This
file keeps only the standing laws + currently-open work.

## v5 Work — Tasks Context

The recurring debt we keep re-hitting has two shapes: **(1) per-row write loops
(N+1)** and **(2) bespoke per-relation refresh functions**. A third,
**(3) string-inline-everywhere**, is the ref-spine debt.

Open items below are one-liners; full history + landed detail in the archive.

### Features / arcs
- [ ] **Auto-refactor**, rides C: `edit(ref_id, new_string_id)` sink, `--fix`/LSP rename. Route A (`--move`) landed; residual = brace-head `use crate::{clk::X, ..}` + physical file move + moved file's own imports. Plan: `plans/2026-05-31-auto-refactor-use-path-rewrite.md`.
- [ ] **vscode Wave 4**: B4 dl/locate follow-user; B5 call/type hierarchy; C3 exploded stratum view; C4 3D iso go/no-go. Plan: `plans/2026-07-10-vscode-ext-review.md`.
- [ ] **LSP thin client over the daemon**: `--lsp` = stdio<->socket adapter (LspPump mirrors mcp::Pump); retires served-copy divergence. Plan: `plans/2026-07-10-lsp-thin-client-daemon.md`.
- [ ] **Turnkey query surface**: `dl q <verb>` runner (param injection + verb_catalog); then blast-radius/dependents verbs via run_reaches_pair + built-in MCP tools dl.what/dl.verb/dl.rows; `dl find` (Tier 3). Plan: `plans/2026-07-10-turnkey-query-surface.md`.
- [ ] Migrate deck graph (`examples/anim-self.dl` + anim AtlasPanel) from name-keyed `type_edge` to sym-keyed `type_link` + `type_entity` (optional; changes node identity).

### Bugs / gaps
- [ ] **SERVED-COPY DIVERGENCE (Chris only)**: running daemon on ~/projects/sprefa runs the old image. Restart: `kill $(head -1 ~/projects/sprefa/.dl/daemon.pid)` then `nohup ~/.cargo/bin/dl --daemon --root ~/projects/sprefa`; then re-copy `.dl/flow-panel.dl` + `dl setup --project`. (LSP thin client retires this class.)
- [ ] **Daemon scip index staleness**: index.scip is gitignored so the watchgate drops its events; `ScipKind::dirty` never fires. Fix: allowlist index.scip through the watchgate.
- [ ] **enumerate_with_hash mtime+size fast path**: equal-length edit in the same fs-timestamp tick reads as unchanged (rapid two-tick same-db test flake risk).
- [ ] Small extractor gaps: Rust trait default methods + Kotlin `object` decls emit no type_entity rows.
- [ ] **S3** body-level bind for pure-fn values (`x = replace(...)` must inline into head). **S4** no string `+`/`concat` (only template interp). **S5** ast-grep patterns exact-shape (metavar-in-JSX `{ element: <$C/> }` matched nothing). **S6** source-extract rule body silently drops an extra joined rel atom (rel-level guard doesn't cover body-level mix).

### Debriefs / friction (backprop candidates)
- [ ] **Change-cost friction inventory** — 12 ranked items, fix shapes + sequencing: `plans/2026-07-10-change-cost-friction-inventory.md`. Top: ambient-config hermeticity, declared cross-family read edges, query --format=json, engine-monolith epic, resolution_source column.
- [ ] Recurring pains across agent debriefs: (a) **ambient config** — every ad-hoc `dl` run ingests `~/.config/sprefa/config.toml` repos; set `SPREFA_CONFIG` for hermetic smoke tests. (b) **rel line bases undocumented** — comment_node 1-based, scip_occurrence 0-based, df 1-based; put base in RelDecl docs. (c) **TS dataflow silently sparse** — class-method bodies emit 0 df_nodes; needs a per-lang df coverage doc. (d) `dl query` indents data rows 2 spaces (mis-keys cell-0 parsers). (e) no public `eng.ensure_families(&[...])`. (f) `crate::daemon` vs `crate::cli::daemon` collision. (g) skill `sprefa-v5-working-conventions` shows a removed `--root` flag. (h) AST_LANG_TABLE buried at ~mod.rs:7674 (engine-monolith placement debt).

### Style notes for this repo
- dl variable names are descriptive, never single-letter: `path`/`line`/`callee_name`, not `p`/`l`/`q`. Applies to every snippet in skills, examples, book, tests, and agent prompts; rename opportunistically when touching old files.
- N+1: never a per-row write. Collect the set, call `Db::insert_rows` once. The tick counter screams if you don't.
- No `provenance`/`substrate`/`load-bearing`/`regime` as prose or identifiers (use source/base/critical/mode).
- Sync tick engine: plural-API + collect-then-flush, NOT async DataLoader (the redux-out-of-hand trap).
- One rel = one rule kind: never head a rel with both a source rule (scan/match/ast/sg/json/cmd/comment) and a derived rule. `rebuild_derived` does a full `DELETE FROM rel` that would wipe the reconciled source rows. The engine now bails; split into two rels and union in a third derived rule. SAME hazard, separately guarded, for a **term-extract** rule (a `json`/`jsonp` body predicate over a bound string) headed together with a derived rule: `eval_extract_rules` fills the extract rows, then `rebuild_derived` (which runs after it so derived rules can read the extract output) drops them. Notably a term-extract rule cannot feed a `@next` carry directly for this reason — route it through its own rel first (the `pr_number -> change_log` split in gh-cache.dl). Engine bails as of the ghcacher-parity arc.
- Recompute guard: a fn that re-derives a relation/embedding FROM SCRATCH (a global op like `embed_graph`, run on a reactive rule) must early-out when its input is unchanged — a `load_rel_digest` digest skip (see `eval_node2vec_rule`, the scc/closure `ConditionCache.digest`) — or carry a `// @recompute unguarded: <reason>` waiver in its body. `examples/recompute-guard.dl --check` (exit 2) is the rail that enforces it; an unguarded recompute re-runs on every git-checkout re-tick under the daemon lock.
