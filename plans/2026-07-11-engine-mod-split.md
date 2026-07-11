# Split src/engine/mod.rs (9,005 lines) into engine/ submodules

## Context

`src/engine/mod.rs` is 9,005 lines: one 5,700-line `impl Engine` (lines
2200-7893, ~150 methods), ~900 lines of rel-decl catalogs, `ModuleRows`,
`GitBatch`, free helpers. The repo already proves the pattern: `engine/extract.rs`
and `engine/tick.rs` each hold their own `impl Engine` block in a sibling file.
This is the mechanical layer UNDER the trait-extraction epic
(project_engine_refactor_epic), not a replacement for it. Governed by the
file-size law (scripts/filesize-rail.sh): target 300 lines, hard 500 —
allowlist rows for these new files shrink as the trait epic lands.

## Method clusters (2026-07-11 line survey)

| new file | contents (mod.rs lines today) | ~lines |
| --- | --- | --- |
| engine/decls.rs | all `*_rel_decls()` catalogs, builtin decl/name/brand fns, fn_docs/op_docs, `*_rels_used()` gates, every_intervals/clock_periods, classify_call_kind (483-1390) | 900 |
| engine/repo.rs | resolve_rev..resolve_scan_bindings (2246-2523), run_repo_pulls/checkout family/parse_github_org/observe_ref/files_changed_between (5581-6091), collect_manifests, impl GitBatch, snapshot/save_repos_meta | 1,000 |
| engine/lens.rs | located_spans..textual_lens (2555-3403): definition/hover/refs/locate | 850 |
| engine/symbols.rs | document_highlights/workspace_symbols/document_symbols + call/type hierarchy (3387-3867) | 480 |
| engine/meta.rs | ensure_meta/shape persistence (4016-4413), digest family (4414-4592), file meta + spine inserts (4817-4884, 6736-6807), carry (6277-6354) | 1,100 |
| engine/reconcile.rs | reconcile_sources/retract_path(s) (4593-4816), insert_source_rows (6700-6735), eval_extract_rules (6355-6531) | 600 |
| engine/declare.rs | declare/declare_all/declare_builtins/declare_closure + refresh_builtin_rels/refresh_every/refresh_clock (4885-5343, 6248-6276) | 500 |
| engine/rpc.rs | query_sql/count_rows/diags/extraction drops (3877-4015), log_query/inject_rpc/hook events/diag-mute/drain_rpc/retire_rpc/rel_rows/repo_relation/drain_external_sinks (5344-5580, 5734) | 550 |
| engine/derive.rs | rebuild_derived/save_stmt_ms (6532-6625), closure machinery + run_reaches/scc/node2vec (6092-6223, 6626-6699, 6808-7014), scc/carry tbl helpers | 900 |
| engine/query.rs | run_query/print/query_one_sql/run_queries_capture + verify/journal (7015-7144) | 250 |
| engine/gen.rs | run_gens/run_gen/apply_splices/appends/cursors/zones (7145-7538) | 450 |
| engine/lang_tables.rs | AST_LANG_TABLE region (~7674) — the ledgered placement-debt item | 150 |
| mod.rs keeps | Engine struct + new/run/setters, config knobs, ModuleRows, result structs (DiagRow/QueryResult/RefHit family), small shared helpers | ~800 |

Clusters over 500 lines get their own allowlist row pointing HERE; the trait
epic is the shrink path.

## Rules

1. PURE MOVES: no renames, no signature or logic changes; visibility bumps to
   `pub(crate)` only where a moved item is referenced cross-file.
2. One cluster = one commit, `cargo build` clean each; full `cargo test`
   batched (max 3 runs); verify.sh (incl. filesize + magic-rel rails) at the end.
3. Order: decls → gen → query → symbols → lens → rpc → declare → reconcile →
   repo → meta → derive → lang_tables. derive/meta go LAST and rebase over any
   landed perf branches (semi-naive fixpoint edits rebuild_derived) — never the
   reverse.
4. Free helpers move with their only caller; shared ones stay in mod.rs.
5. Review with `git diff --color-moved=dimmed-zebra` to prove moves are pure.

## Sequencing hazard — RESOLVED (2026-07-11, main 81241d5)

The semi-naive fixpoint branch (a1e7ebc) and the sym interning branch
(81241d5) both LANDED before this split starts, so the old "wait for them"
hazard is gone. What they changed for this plan:
- mod.rs is now ~9,200 lines; the line ranges in the table above are the
  pre-landing survey — RE-FIND each cluster by method name, do not trust the
  numbers.
- The derive cluster gained `rebuild_derived_seminaive` (moves with
  `rebuild_derived`, same file) plus the `fixpoint_full_reruns` /
  `force_naive_fixpoint` Cell fields on Engine (fields stay in mod.rs with
  the struct).
- src/spine.rs gained `Sym`/`SymSink`; src/db.rs gained `flush_syms` —
  untouched by this plan, listed so a mechanical mover doesn't "helpfully"
  relocate them.
- Baseline gate before ANY move: `cargo test --test it` green on the base sha
  (743/0/10 at 81241d5), and re-run per the batching rule below.

## Staffing

CODEX (Chris's call, 2026-07-11): run via `codex exec` in a dedicated git
worktree, one cluster per commit, no subagents, file-size law in force
(each new file over 500 lines gets an allowlist row pointing here).

<!-- todo(triage): SG_LANG_TABLE final home (src/sg.rs vs engine/lang_tables.rs) when the lang_tables cluster moves -->
<!-- todo(feature): trait-extraction epic Phase 1 (RelKind) resumes on top of the split -->
