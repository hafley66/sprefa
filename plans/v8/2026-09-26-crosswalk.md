# v5 and v6 to v8 ability crosswalk

## Context

This inventory reads all 227 files in `v5/tests/it` and the named v6 readiness, architecture, language and clock sources against the present v8 `src/` and `tests/`. Status describes the named ability end to end. `have` means a v8 implementation and test exercise that ability. `partial` means the cited v8 mechanism exists but a v5 or v6 surface or contract is absent. `missing` means no v8 implementation was found for the named ability. A shared primitive does not turn a missing application into `have`.

The ladder column names the first relevant rung. `join 1:1/1:n/n:n` distinguishes the shapes within the join rung. The v6 `LANG.md` is explicitly an unbanked candidate. Its `pre(atom)` and the `latest()` retention decision are recorded as source behavior to assess and are not presumed v8 requirements.

## Crosswalk

| ability | v5/v6 source | v8 status | v8 file:line | ladder rung |
|---|---|---|---|---|
| R01 Ground facts and fixed seed rows | `v5/tests/it/facts.rs`; `v6/prolog/LANG.md:45` | have | `src/_6_eval/_1_program.rs:66`; `tests/_0_eval_oracle.rs:20` | seeds |
| R02 File and repo/revision/byte identity | `v5/tests/it/spine_meta.rs`; `v5/tests/it/builtin_file_rel.rs`; `v6/DECISIONS.md:115` | partial: soopy owns the model and `fs.at` exposes a slice | `src/_9_runtime/_3_executors/fs_at.rs:25`; `tests/_20_hosts.rs:304` | seeds |
| R03 Scalar filters and source predicates | `v5/tests/it/regex_glob_filter.rs`; `v5/tests/it/arith.rs` | partial: comparisons and integer add exist | `src/_6_eval/_4_kernel.rs:52`; `tests/_12_term_lt.rs:102` | filter |
| R04 Positive derived rules and 1:1 joins | `v5/tests/it/body_binds.rs`; `v5/tests/it/mixed_source_derived.rs` | have | `src/_6_eval/_9_dd_eval.rs:682`; `tests/_0_eval_oracle.rs:20` | join 1:1 |
| R05 Fan-out joins and chained derivation | `v5/tests/it/agg.rs`; `v5/tests/it/seminaive.rs` | have | `src/_6_eval/_9_dd_eval.rs:883`; `tests/_28_eval_dd.rs:1` | join 1:n |
| R06 Many-to-many cross-repo joins | `v5/tests/it/xrepo_cycle.rs`; `v6/READINESS.md:43` | partial: relational joins exist but v5 repo-scoped call resolution does not | `src/_6_eval/_9_dd_eval.rs:883`; `tests/_20_hosts.rs:647` | join n:n |
| R07 Negated goals and stratification | `v5/tests/it/true_singleton.rs`; `v6/prolog/3_clock_check.pl:358` | have | `src/_6_eval/_2_stratify.rs:61`; `src/_6_eval/_9_dd_eval.rs:824` | negation |
| R08 Count and sum groups | `v5/tests/it/agg.rs`; `v5/tests/it/query_agg.rs` | have | `src/_6_eval/_5_evaluate.rs:797`; `tests/_11_aggregates.rs:102` | count |
| R09 Min/max and declared folds | `v5/tests/it/lattice.rs`; `v6/READINESS.md:47` | partial: min/max and folds exist; v5 choice and merge qualifiers do not | `src/_6_eval/_5_evaluate.rs:631`; `tests/_15_fold.rs:114` | min/max |
| R10 Recursive chains and reachability | `v5/tests/it/seminaive.rs`; `v5/tests/it/halt_bfs.rs` | have | `src/_6_eval/_9_dd_eval.rs:91`; `tests/_28_eval_dd.rs:1` | recursion chain |
| R11 Cycles and withdrawal of derived rows | `v5/tests/it/retraction_scripts.rs`; `v6/DECISIONS.md:34` | partial: v8 has DD and SQLite evaluation; v5 edit-script and v6 two-plane parity are not established | `src/_6_eval/_7_sqlite_eval.rs:48`; `tests/_29_eval_random.rs:1` | recursion cycle |
| R12 Graph algorithms over call/type edges | `v5/tests/it/scc.rs`; `v5/tests/it/node2vec.rs`; `v6/ARCHITECTURE.md:16` | partial: recursive graph rules exist; v5 SCC and node2vec operators do not | `src/_6_eval/_9_dd_eval.rs:682`; `tests/_28_eval_dd.rs:1` | recursion cycle |
| R13 Scalar and term kernels | `v5/tests/it/string_fns.rs`; `v5/tests/it/arith_in_body.rs` | partial: core list/string/int/ordering kernels exist; v5 scalar function set does not | `src/_6_eval/_4_kernel.rs:10`; `tests/_12_term_lt.rs:102` | kernels |
| R14 Demand-limited evaluation | `v5/tests/it/closure_query_guard.rs`; `v6/prolog/LANG.md:30` | partial: demand rules and a demand-count gate exist; v5 query cap and subscription lifecycle do not | `src/_6_eval/_7_sqlite_eval.rs:945`; `tests/_24_demand_count.rs:32` | demand |
| R15 Host request and answer effects | `v5/tests/it/temporal_async.rs`; `v6/prolog/LANG.md:30` | partial: host answer loop and named executors exist; v5 shell template modes do not | `src/_9_runtime/_2_reconcile.rs:42`; `tests/_17_reconcile.rs:211` | paging/effects |
| R16 Shell commands and template registry including `sh`/`sh!`/`sh*` modes and per-row cwd | `v5/src/effect.rs:186`; `v5/src/effect.rs:514`; `v5/tests/it/cmd_op.rs` | missing | — | paging/effects |
| R17 Query transport and paging | `v5/tests/it/lsp_query_paging.rs`; `v5/tests/it/query_json.rs` | partial: CLI eval and JSON results exist; v5 LSP cursor paging does not | `src/bin/dl8.rs:270`; `tests/_0_eval_oracle.rs:20` | paging/effects |
| R18 Comptime fixpoint and effect answers | `v6/ARCHITECTURE.md:29`; `v5/tests/it/settle.rs` | have | `src/_4_comptime/_2_rounds.rs:193`; `tests/_23_comptime_effect.rs:1` | comptime rounds |
| R19 Clock and retention operators | `v6/prolog/LANG.md:44`; `v6/DECISIONS.md:97`; `v6/prolog/3_clock_check.pl:320` | partial: timer exists; `pre(atom)` and `latest()` with v6 retention and clock checks do not | `src/_9_runtime/_3_executors/timer.rs:14`; `tests/_17_reconcile.rs:169` | paging/effects |
| R20 Module import and program assembly | `v5/tests/it/module_use.rs`; `v5/tests/it/module_def.rs` | partial: v8 module imports compile; v5 `use` and template `def` contracts differ | `src/_4_comptime/_0_load/_1_paths.rs:1`; `tests/_5_load_oracle.rs:1` | comptime rounds |
| R21 File enumeration and Git history/ref hosts | `v5/tests/it/git_ref.rs`; `v5/tests/it/changed.rs` | partial: soopy backs `fs.at`, `git.history` and `git.refs`; v5 `changed` relations are absent | `src/_9_runtime/_3_executors/git_history.rs:26`; `tests/_20_hosts.rs:228` | paging/effects |
| R22 Structured JSON/YAML/TOML input | `v5/tests/it/data_ops.rs`; `v5/tests/it/jsonl.rs` | partial: `fs.json` and OpenAPI JSON loading exist; v5 multi-format query op does not | `src/_9_runtime/_3_executors/fs_json.rs:39`; `tests/_20_hosts.rs:394` | seeds |
| R23 Source extraction and TSI type graph | `v5/tests/it/type_graph.rs`; `v5/tests/it/python.rs` | partial: external extraction stream and TSI import exist; v5 built-in family parity is unproved | `src/_9_runtime/_3_executors/extract.rs:113`; `tests/_16_extract_tsi.rs:228` | join 1:n |
| R24 AST/CST matching and rewrite | `v5/tests/it/ast_yaml.rs`; `v5/tests/it/cst_node_rel.rs` | missing | — | filter |
| R25 Grammar-backed comments and Markdown snippets including fenced-sample exclusion and generated-index drift | `v5/tests/it/comment_node.rs`; `v5/tests/it/plans_index.rs`; `v5/tests/it/readme_gen.rs` | missing | — | seeds |
| R26 Call graph extraction and resolution | `v5/tests/it/call_rels.rs`; `v5/tests/it/oracle_rust.rs` | partial: TSI stream has semantic edges; v5 call-family and oracle parity are unproved | `tests/_16_extract_tsi.rs:228`; `src/_9_runtime/_3_executors/extract.rs:113` | join n:n |
| R27 Dataflow and cross-language flow | `v5/tests/it/dataflow.rs`; `v5/tests/it/flow_xlang.rs` | missing | — | recursion chain |
| R28 SCIP import and demand | `v5/tests/it/scip_import.rs`; `v5/tests/it/scip_want.rs` | missing | — | demand |
| R29 Code generation and verified writes | `v5/tests/it/gen_op.rs`; `v5/tests/it/verify_rollback.rs` | partial: SQLite emitter exists; v5 file template sink and transactional verify do not | `src/_5_reify/_7_sqlite.rs:77`; `tests/_18_sqlite_emit.rs:200` | paging/effects |
| R30 Repository registry and multi-repo refresh | `v5/tests/it/config_repos.rs`; `v5/tests/it/repo_sink.rs`; `v6/READINESS.md:43` | partial: soopy owns multi-repo refresh and v8 can read repo heads; v5 config/sink surface is absent | `src/_9_runtime/_3_executors/git_refs.rs:116`; `tests/_20_hosts.rs:647` | demand |
| R31 Watch and daemon lifecycle | `v5/tests/it/daemon.rs`; `v6/READINESS.md:54` | partial: runtime tick and Git ref polling exist; v5 singleton daemon/API and file-watch scaling gate do not | `src/_9_runtime/_2_reconcile.rs:132`; `tests/_17_reconcile.rs:169` | paging/effects |
| R32 LSP/MCP and editor surfaces | `v5/tests/it/lsp_protocol.rs`; `v5/tests/it/mcp.rs`; `v6/READINESS.md:31` | missing | — | paging/effects |
| R33 Diagnostics and check rails | `v5/tests/it/ban_gate.rs`; `v5/tests/it/diag_mute.rs` | partial: compiler diagnostics exist; v5 check daemon and user-authored rails do not | `src/_3_check/_0_api.rs:1`; `tests/_4_check_oracle.rs:1` | filter |
| R34 Persistent storage and incremental cost gates | `v5/tests/it/storage_diet_norm.rs`; `v6/ARCHITECTURE.md:119` | partial: SQLite row store and IVM contract exist; v5 telemetry and cost gates differ | `src/_9_runtime/_1_sqlite.rs:1`; `tests/_25_sqlite_contract.rs:1` | recursion cycle |
| R35 Query verbs and catalogs | `v5/tests/it/q_verb.rs`; `v5/tests/it/rel_catalog.rs` | missing | — | demand |
| R36 Embeddings and similarity | `v5/tests/it/embed_similar.rs`; `v5/tests/it/node2vec.rs` | missing | — | kernels |
| R37 CLI setup and process safeguards | `v5/tests/it/setup_manifest.rs`; `v5/tests/it/process_budget.rs` | missing | — | comptime rounds |
| R38 Execution logs and observability | `v5/tests/it/invlog.rs`; `v5/tests/it/write_ledger.rs` | partial: tracing and store receipts exist; v5 log formats and write ledger do not | `src/bin/dl8.rs:136`; `tests/_9_tracing.rs:1` | comptime rounds |
| R39 Repository-specific rails and documentation automation | `v5/tests/it/arch.rs`; `v5/tests/it/plans_index.rs` | missing | — | join 1:n |

## v6 semantic boundaries

| ability | v5/v6 source | v8 status | v8 file:line | ladder rung |
|---|---|---|---|---|
| V01 Control reconciliation before fact deltas | `v6/ARCHITECTURE.md:29`; `v6/DECISIONS.md:13` | partial: v8 has an effect answer/evaluate tick and an IVM evaluator; no v6 digest-pruned relation dependency plane is shown | `src/_9_runtime/_2_reconcile.rs:207`; `src/_6_eval/_7_sqlite_eval.rs:503` | comptime rounds |
| V02 Previous tick and latest retained row | `v6/prolog/LANG.md:44`; `v6/DECISIONS.md:110` | missing | — | paging/effects |
| V03 Clock path checks and batch invariance boundary (the v6 path walk is pinned off by default) | `v6/prolog/3_clock_check.pl:301`; `v6/prolog/3_clock_check.pl:365` | missing | — | comptime rounds |
| V04 DRed cyclic retraction and integer support counts | `v6/DECISIONS.md:34`; `v6/ARCHITECTURE.md:119` | partial: v8 uses SQLite IVM and DD evaluators; this exact v6 contract is not established | `src/_6_eval/_7_sqlite_eval.rs:48`; `tests/_27_eval_sqlite.rs:1` | recursion cycle |

## Decisions

This is a source and test crosswalk. It does not authorize a compiler kernel change. The v5 where/bytes/file/git-repo/rev model maps to soopy's `hafley-rs/crates/soopy/src/_0_types.rs`, `_3_revision.rs`, `_3a_files.rs`, `_5_git_tree.rs`, `_6_git_batch.rs` and `_7_source_tree.rs` and then to the v8 `fs.at`, `git.history` and `git.refs` adapters. No replacement model is proposed. The v6 candidate syntax is retained as a comparison only.

## Verification

Validate that every `v5/tests/it/*.rs` path has a coverage entry. Check each nonempty v8 `file:line` against the worktree. No build or test is required for this documentation-only change. This work adds no CI coverage.

## Staffing

One agent in this worktree. Base SHA is recorded by the commit parent. Build budget is zero because no executable source changes.

## v5 test-file coverage

The entries below map each integration-test file to at least one crosswalk row. A file can exercise more than the named row; the row records its primary ability here.

- `v5/tests/it/access_log.rs` -> R31
- `v5/tests/it/agent_rels.rs` -> R39
- `v5/tests/it/agg.rs` -> R08
- `v5/tests/it/analysis_bundle_ab.rs` -> R34
- `v5/tests/it/arch.rs` -> R39
- `v5/tests/it/arith.rs` -> R03
- `v5/tests/it/arith_in_body.rs` -> R03
- `v5/tests/it/ast_grammars.rs` -> R24
- `v5/tests/it/ast_whole_match_id.rs` -> R24
- `v5/tests/it/ast_yaml.rs` -> R24
- `v5/tests/it/authoring_edges.rs` -> R33
- `v5/tests/it/ban_gate.rs` -> R33
- `v5/tests/it/body_binds.rs` -> R04, R07
- `v5/tests/it/budget_cpu.rs` -> R34
- `v5/tests/it/builtin_file_rel.rs` -> R02
- `v5/tests/it/call_golden.rs` -> R26
- `v5/tests/it/call_rels.rs` -> R26
- `v5/tests/it/callable_defs.rs` -> R26
- `v5/tests/it/changed.rs` -> R21
- `v5/tests/it/changed_line.rs` -> R21
- `v5/tests/it/chat_marks.rs` -> R33
- `v5/tests/it/check_daemon.rs` -> R33
- `v5/tests/it/check_deadline.rs` -> R33
- `v5/tests/it/checkout_sweep.rs` -> R30
- `v5/tests/it/clock_lock.rs` -> R19
- `v5/tests/it/closure_incremental_bench.rs` -> R12
- `v5/tests/it/closure_query_guard.rs` -> R14
- `v5/tests/it/cmd_op.rs` -> R16
- `v5/tests/it/cold_stage.rs` -> R34
- `v5/tests/it/comment_node.rs` -> R25
- `v5/tests/it/comment_op.rs` -> R25
- `v5/tests/it/composite_key_precision.rs` -> R33
- `v5/tests/it/config_repos.rs` -> R30
- `v5/tests/it/const_value.rs` -> R13
- `v5/tests/it/created_perf.rs` -> R34
- `v5/tests/it/cst_node_perf.rs` -> R24
- `v5/tests/it/cst_node_rel.rs` -> R24
- `v5/tests/it/daemon.rs` -> R31
- `v5/tests/it/daemon_http.rs` -> R31
- `v5/tests/it/daemon_stateful_revs.rs` -> R31
- `v5/tests/it/data_driven_scan.rs` -> R30
- `v5/tests/it/data_ops.rs` -> R22
- `v5/tests/it/dataflow.rs` -> R27
- `v5/tests/it/db_ratio.rs` -> R34
- `v5/tests/it/deprecated_op_names.rs` -> R24
- `v5/tests/it/derived_intern_n1.rs` -> R34
- `v5/tests/it/derived_scope.rs` -> R04
- `v5/tests/it/derived_skip.rs` -> R05
- `v5/tests/it/diag_mute.rs` -> R33
- `v5/tests/it/digest_skip.rs` -> R05
- `v5/tests/it/discover.rs` -> R20
- `v5/tests/it/dl_chain_discovery.rs` -> R20
- `v5/tests/it/dl_diag.rs` -> R33
- `v5/tests/it/doc_comment.rs` -> R25
- `v5/tests/it/doc_node.rs` -> R25
- `v5/tests/it/doc_ref.rs` -> R25
- `v5/tests/it/docs_cmd.rs` -> R35
- `v5/tests/it/embed_similar.rs` -> R36
- `v5/tests/it/empty_scan_scope_guard.rs` -> R34
- `v5/tests/it/ensure_families.rs` -> R23
- `v5/tests/it/entity_parent_xfile.rs` -> R06
- `v5/tests/it/env_rel.rs` -> R39
- `v5/tests/it/eprintln_waiver.rs` -> R33
- `v5/tests/it/error_reporting.rs` -> R33
- `v5/tests/it/extract_cache.rs` -> R23
- `v5/tests/it/extract_digest_namespace.rs` -> R23
- `v5/tests/it/extraction_determinism.rs` -> R23
- `v5/tests/it/facts.rs` -> R01
- `v5/tests/it/family_op_raw_sql_audit.rs` -> R33
- `v5/tests/it/family_scaling_probe.rs` -> R34
- `v5/tests/it/file_lines.rs` -> R02
- `v5/tests/it/flow_breaking.rs` -> R27
- `v5/tests/it/flow_ctor.rs` -> R27
- `v5/tests/it/flow_go_dispatch.rs` -> R27
- `v5/tests/it/flow_interproc.rs` -> R27
- `v5/tests/it/flow_jsx.rs` -> R27
- `v5/tests/it/flow_kotlin_dispatch.rs` -> R27
- `v5/tests/it/flow_py_dispatch.rs` -> R27
- `v5/tests/it/flow_services.rs` -> R27
- `v5/tests/it/flow_std.rs` -> R27
- `v5/tests/it/flow_xlang.rs` -> R27
- `v5/tests/it/flow_xlang_scip.rs` -> R27
- `v5/tests/it/flow_xlang_scip_real.rs` -> R27
- `v5/tests/it/gen_op.rs` -> R29
- `v5/tests/it/gh_cache.rs` -> R15
- `v5/tests/it/ghcacher_parity.rs` -> R15
- `v5/tests/it/git_ref.rs` -> R21
- `v5/tests/it/go.rs` -> R23
- `v5/tests/it/goto_flows.rs` -> R27
- `v5/tests/it/graph_diff_rev.rs` -> R21
- `v5/tests/it/halt_bfs.rs` -> R10
- `v5/tests/it/hermetic_state.rs` -> R31
- `v5/tests/it/hook_deadline.rs` -> R31
- `v5/tests/it/hook_inject.rs` -> R31
- `v5/tests/it/hover_note.rs` -> R32
- `v5/tests/it/http_loopback.rs` -> R31
- `v5/tests/it/invlog.rs` -> R38
- `v5/tests/it/json_agg.rs` -> R08
- `v5/tests/it/jsonl.rs` -> R22
- `v5/tests/it/kotlin.rs` -> R23
- `v5/tests/it/lang_matrix.rs` -> R39
- `v5/tests/it/lang_skill_gen.rs` -> R39
- `v5/tests/it/lattice.rs` -> R09
- `v5/tests/it/lex_utf8.rs` -> R03
- `v5/tests/it/lint_no_touch.rs` -> R33
- `v5/tests/it/log_cap_sweep.rs` -> R34
- `v5/tests/it/lsp_exit.rs` -> R32
- `v5/tests/it/lsp_hierarchy.rs` -> R32
- `v5/tests/it/lsp_hook_event.rs` -> R31
- `v5/tests/it/lsp_protocol.rs` -> R32
- `v5/tests/it/lsp_query_paging.rs` -> R17
- `v5/tests/it/lsp_refs.rs` -> R32
- `v5/tests/it/lsp_symbols.rs` -> R32
- `v5/tests/it/magic_rel_audit.rs` -> R33
- `v5/tests/it/main.rs` -> R31
- `v5/tests/it/match_col_span.rs` -> R03
- `v5/tests/it/mcp.rs` -> R32
- `v5/tests/it/mcp_daemon.rs` -> R31
- `v5/tests/it/mcp_lifecycle.rs` -> R31
- `v5/tests/it/mixed_source_derived.rs` -> R04
- `v5/tests/it/module_binding.rs` -> R20
- `v5/tests/it/module_def.rs` -> R20
- `v5/tests/it/module_graph.rs` -> R20
- `v5/tests/it/module_use.rs` -> R20
- `v5/tests/it/move_refactor.rs` -> R29
- `v5/tests/it/named_args.rs` -> R20
- `v5/tests/it/node2vec.rs` -> R36
- `v5/tests/it/oracle_corpus.rs` -> R26
- `v5/tests/it/oracle_go.rs` -> R26
- `v5/tests/it/oracle_kotlin.rs` -> R26
- `v5/tests/it/oracle_kotlin_parity.rs` -> R26
- `v5/tests/it/oracle_madge.rs` -> R26
- `v5/tests/it/oracle_parity.rs` -> R26
- `v5/tests/it/oracle_python.rs` -> R26
- `v5/tests/it/oracle_rust.rs` -> R26
- `v5/tests/it/oracle_ts.rs` -> R26
- `v5/tests/it/parse_only.rs` -> R33
- `v5/tests/it/path_types.rs` -> R03
- `v5/tests/it/perf_facts.rs` -> R34
- `v5/tests/it/perf_rels.rs` -> R34
- `v5/tests/it/perf_stress.rs` -> R34
- `v5/tests/it/perf_stress_c.rs` -> R34
- `v5/tests/it/perf_woes.rs` -> R33
- `v5/tests/it/pin_skew.rs` -> R30
- `v5/tests/it/plans_index.rs` -> R25, R39
- `v5/tests/it/poll_head_demo.rs` -> R16
- `v5/tests/it/pr_diff.rs` -> R21
- `v5/tests/it/process_budget.rs` -> R37
- `v5/tests/it/propose_extract.rs` -> R24
- `v5/tests/it/python.rs` -> R23
- `v5/tests/it/q_verb.rs` -> R35
- `v5/tests/it/query_agg.rs` -> R08
- `v5/tests/it/query_json.rs` -> R17
- `v5/tests/it/racy_mtime.rs` -> R02
- `v5/tests/it/readme_gen.rs` -> R25, R39
- `v5/tests/it/recompute_guard.rs` -> R33
- `v5/tests/it/recursive_null_pad.rs` -> R10
- `v5/tests/it/regex_glob_filter.rs` -> R03
- `v5/tests/it/regex_sugar.rs` -> R03
- `v5/tests/it/rel_catalog.rs` -> R35
- `v5/tests/it/repo_sink.rs` -> R30
- `v5/tests/it/resolver_import_alias.rs` -> R26
- `v5/tests/it/resolver_import_narrowing.rs` -> R26
- `v5/tests/it/resolver_repo_scope.rs` -> R06
- `v5/tests/it/retraction_props.rs` -> R11
- `v5/tests/it/retraction_scripts.rs` -> R11
- `v5/tests/it/rev_alias_leak.rs` -> R02
- `v5/tests/it/reverse_traversal_plan.rs` -> R11
- `v5/tests/it/rule_edit.rs` -> R33
- `v5/tests/it/scan_kwargs.rs` -> R24
- `v5/tests/it/scan_narrowing.rs` -> R24
- `v5/tests/it/scc.rs` -> R12
- `v5/tests/it/scip_gate.rs` -> R28
- `v5/tests/it/scip_import.rs` -> R28
- `v5/tests/it/scip_name.rs` -> R28
- `v5/tests/it/scip_want.rs` -> R28
- `v5/tests/it/scoped_tick.rs` -> R05
- `v5/tests/it/seam_bench.rs` -> R34
- `v5/tests/it/seminaive.rs` -> R10
- `v5/tests/it/settle.rs` -> R18
- `v5/tests/it/setup_manifest.rs` -> R37
- `v5/tests/it/sg_json_id.rs` -> R24
- `v5/tests/it/sg_kwargs.rs` -> R24
- `v5/tests/it/sg_rewrite.rs` -> R24
- `v5/tests/it/sg_span.rs` -> R24
- `v5/tests/it/sg_term.rs` -> R24
- `v5/tests/it/source_rule_extra_atom.rs` -> R33
- `v5/tests/it/source_stage_volume.rs` -> R34
- `v5/tests/it/spine_meta.rs` -> R02
- `v5/tests/it/stage_routing.rs` -> R33
- `v5/tests/it/stale_binary.rs` -> R37
- `v5/tests/it/storage_diet_index.rs` -> R34
- `v5/tests/it/storage_diet_norm.rs` -> R34
- `v5/tests/it/storage_diet_without_rowid.rs` -> R34
- `v5/tests/it/string_flow.rs` -> R13
- `v5/tests/it/string_fns.rs` -> R13
- `v5/tests/it/strings_gc.rs` -> R34
- `v5/tests/it/strings_view.rs` -> R13
- `v5/tests/it/suppress.rs` -> R25
- `v5/tests/it/sym_dict_bijection.rs` -> R34
- `v5/tests/it/template_parts.rs` -> R13
- `v5/tests/it/temporal_async.rs` -> R15
- `v5/tests/it/temporal_carry.rs` -> R19
- `v5/tests/it/temporal_clock_salt.rs` -> R19
- `v5/tests/it/temporal_every.rs` -> R19
- `v5/tests/it/tick_digest.rs` -> R05
- `v5/tests/it/trace_chrome.rs` -> R38
- `v5/tests/it/true_singleton.rs` -> R01
- `v5/tests/it/type_decl_row.rs` -> R33
- `v5/tests/it/type_decls.rs` -> R33
- `v5/tests/it/type_entity_extractor_gaps.rs` -> R23
- `v5/tests/it/type_entity_xlang.rs` -> R23
- `v5/tests/it/type_graph.rs` -> R23
- `v5/tests/it/type_graph_js.rs` -> R23
- `v5/tests/it/type_graph_ts.rs` -> R23
- `v5/tests/it/unresolved.rs` -> R27
- `v5/tests/it/util.rs` -> R31
- `v5/tests/it/verdict.rs` -> R38
- `v5/tests/it/verify_rollback.rs` -> R29
- `v5/tests/it/view_backed_rel.rs` -> R04
- `v5/tests/it/wall_watchdog.rs` -> R34
- `v5/tests/it/what.rs` -> R35
- `v5/tests/it/where_removed.rs` -> R35
- `v5/tests/it/wildcard_slots.rs` -> R24
- `v5/tests/it/worktree_cold_check.rs` -> R34
- `v5/tests/it/write_ledger.rs` -> R38
- `v5/tests/it/xrepo_cycle.rs` -> R06
