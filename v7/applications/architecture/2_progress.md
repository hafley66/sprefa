# DL7 shared architecture progress

Generated from [`0_system.dl7`](0_system.dl7) by [`1_generate.pl`](1_generate.pl). Do not edit by hand.

## Source

`v7/receipts/16_dl6_dl7_architecture_inventory.md` is the only authored input. One DL7 relational kernel is applied at macrotime, comptime, and runtime; DL6 is a DL7 userland application, not a second core.

## Counts

| metric | value |
| --- | --- |
| components | 72 |
| typed attachment edges | 75 |
| milestones | 72 |
| acceptance item rows | 7 |
| evidence paths | 179 |
| concrete flow steps | 19 |
| state implemented | 59 |
| state partial | 7 |
| state planned | 3 |
| state absent | 2 |
| state reference | 1 |

## Per-component milestone

Fraction is completed acceptance items over total declared acceptance items. `unknown` means no acceptance list is declared for that component; completion is never inferred from a file existing.

| component | label | plane | binding | state | completed/total | percent |
| --- | --- | --- | --- | --- | --- | --- |
| dd.backend | differential-dataflow backend | host-runtime | runtime | planned | unknown | unknown |
| dl6.bind | bind operator and interval/watch bind runners | userland | comptime | implemented | unknown | unknown |
| dl6.clock | clock dependency and tick-ring grading | userland | comptime | implemented | unknown | unknown |
| dl6.conformance | conformance corpus and battery of record | userland | runtime | reference | 163/163 | 100% |
| dl6.declarations | declaration sugar and expansion phases | userland | macrotime | implemented | unknown | unknown |
| dl6.delta | delta rows and boundary diffs | userland | comptime | implemented | unknown | unknown |
| dl6.diag | diagnostics to LSP records | userland | comptime | implemented | unknown | unknown |
| dl6.effects | effect port rows and executor modules | userland | comptime | implemented | unknown | unknown |
| dl6.emit_dd | isolated DD plan JSON emitter | userland | comptime | implemented | unknown | unknown |
| dl6.emit_rust | Rust target emitter | userland | comptime | implemented | unknown | unknown |
| dl6.emit_ts | TypeScript target emitter | userland | comptime | implemented | unknown | unknown |
| dl6.frontier | frontier and support-count write strategies | userland | comptime | implemented | unknown | unknown |
| dl6.host | host declaration expansion and external relations | userland | comptime | implemented | unknown | unknown |
| dl6.interner | program text intern plan and runtime intern planes | userland | comptime | implemented | unknown | unknown |
| dl6.ivm | incremental view maintenance lowering and runtime | userland | comptime | implemented | unknown | unknown |
| dl6.keys | declared relation keys and key checks | userland | comptime | implemented | unknown | unknown |
| dl6.lower | rule to target-neutral lowered plan | userland | comptime | implemented | unknown | unknown |
| dl6.queries | query cone subscription and pruning | userland | comptime | implemented | unknown | unknown |
| dl6.reload_catalog | program reload planner and runtime catalog | userland | comptime | implemented | unknown | unknown |
| dl6.retention | retention bounds and history pruning | userland | comptime | implemented | unknown | unknown |
| dl6.rules | analysis, body walk, stratification, program checks | userland | comptime | implemented | unknown | unknown |
| dl6.storage | storage projection and struct type plans | userland | comptime | implemented | unknown | unknown |
| dl6.surface | DL6 surface grammar, DCG parser, printer, registry | userland | macrotime | implemented | unknown | unknown |
| dl6.types | type plane, semantic type ids, topological order | userland | comptime | implemented | unknown | unknown |
| dl6.watches | filesystem/revision watch sources | userland | comptime | implemented | unknown | unknown |
| dl7.bench | compiler performance budget gate | core | comptime | implemented | 17/17 | 100% |
| dl7.checker | checked Datalog resolution, safety, strata | core | comptime | implemented | unknown | unknown |
| dl7.compiler_cacher | content-addressed compile/prelude cache | core | comptime | implemented | unknown | unknown |
| dl7.compiler_tracer | phase/step compile trace ledger | core | comptime | implemented | unknown | unknown |
| dl7.emit.dbsp | DBSP JSON and Rust plan emitters | core | comptime | partial | 4/4 | 100% |
| dl7.emit.rust | Rust type and runtime region emitters | core | comptime | implemented | unknown | unknown |
| dl7.emit.sqlite | SQLite query and IVM install emitter | core | comptime | partial | 5/5 | 100% |
| dl7.emitter_protocol | target-neutral reification and artifact emission | core | comptime | implemented | unknown | unknown |
| dl7.emitters | userland policy emitter programs | userland | comptime | implemented | unknown | unknown |
| dl7.evaluator | shared stratified Datalog evaluator | core | shared | implemented | unknown | unknown |
| dl7.extract_loader | TSI stream loader and registry validation | core | comptime | implemented | unknown | unknown |
| dl7.host_planner | hosted external operator shape validation | core | comptime | partial | unknown | unknown |
| dl7.kernel | DL7 compiler kernel, lowering, driver | core | comptime | implemented | 10/10 | 100% |
| dl7.macrotime | macrotime library and syntax rewrite | core | macrotime | implemented | unknown | unknown |
| dl7.module_graph | multi-unit module graph and environments | core | comptime | implemented | unknown | unknown |
| dl7.prelude | DL7 prelude declarations and type algebra | core | shared | implemented | unknown | unknown |
| dl7.reader | DL7 reader and CST | core | macrotime | implemented | unknown | unknown |
| dl7.schema | DL7 runtime/wire type schema | core | shared | implemented | unknown | unknown |
| dl7.source_fact_loader | source-fact envelope to span/occurrence graph | core | comptime | implemented | unknown | unknown |
| dl7.tool_cli | CLI mainers over the compiler | core | comptime | implemented | unknown | unknown |
| dl7.userland | userland type algebra and DL6 catalog app | userland | comptime | implemented | 22/22 | 100% |
| ext.core | sprefa-extract crate core and dispatch | host-runtime | runtime | implemented | unknown | unknown |
| ext.cpg | Joern CPG decode | host-runtime | runtime | partial | unknown | unknown |
| ext.langs | per-language extraction front ends | host-runtime | runtime | implemented | unknown | unknown |
| ext.move_rename | move and rename rewriters | host-runtime | runtime | implemented | unknown | unknown |
| ext.scip | SCIP indexer produce/load | host-runtime | runtime | partial | unknown | unknown |
| ext.sqlite_export | extract SQLite snapshot exporter | host-runtime | runtime | implemented | unknown | unknown |
| ext.tsi | typed symbol index protocol and registry | host-runtime | runtime | implemented | unknown | unknown |
| ext.watch | named-ref watch protocol and generations | host-runtime | runtime | implemented | unknown | unknown |
| hmr | program hot reload and generation-boundary swap | host-runtime | runtime | partial | unknown | unknown |
| host.clocks | clock tick host | host-runtime | runtime | implemented | unknown | unknown |
| host.env | environment variable host | host-runtime | runtime | implemented | unknown | unknown |
| host.filesystem | filesystem host executor | host-runtime | runtime | implemented | unknown | unknown |
| host.git | Git refs, history, repo-at hosts | host-runtime | runtime | implemented | unknown | unknown |
| host.http | HTTP get/post hosts | host-runtime | runtime | implemented | unknown | unknown |
| host.shell | shell/process host executor | host-runtime | runtime | implemented | unknown | unknown |
| host.soopy | Soopy stage/commit/checkout hosts | host-runtime | runtime | implemented | unknown | unknown |
| ivm.algebra | shared generic IVM operator algebra | host-runtime | runtime | absent | unknown | unknown |
| ivm.sqlite | SQLite transactional IVM extension | host-runtime | runtime | implemented | 64/64 | 100% |
| lsp.diagnostics | legacy v5 DL LSP diagnostics (donor; not V7) | host-runtime | runtime | implemented | unknown | unknown |
| lsp.edits | legacy v5 LSP rename/codeAction/workspace edits (absent; not V7) | host-runtime | runtime | absent | unknown | unknown |
| rt.dd_ram | dd-runner RAM kernel | host-runtime | runtime | partial | unknown | unknown |
| rt.executor | generic whole-language runtime executor | host-runtime | runtime | planned | unknown | unknown |
| rt.rust | sprefa-engine-rs generated-program runtime | host-runtime | runtime | implemented | unknown | unknown |
| rt.store | sprefa-store cascade engine with dd/salsa oracles | host-runtime | runtime | implemented | unknown | unknown |
| rt.tsv2 | TypeScript tsv2 incremental runtime | host-runtime | runtime | implemented | unknown | unknown |
| rt.v7 | DL7 successor whole-language runtime | host-runtime | runtime | planned | unknown | unknown |

## Evidence paths

| component | path |
| --- | --- |
| dd.backend | sqlite_ivm/bench/shared/34_circuit_dd.rs |
| dd.backend | sqlite_ivm/bench/shared/22_crossover_dd.rs |
| dl6.bind | v6/prolog/1_host_expand.pl:491 |
| dl6.bind | v6/tsv2/serve/2_binds.ts:139,373 |
| dl6.bind | v6/sprefa-engine-rs/src/source_bind/mod.rs |
| dl6.clock | v6/prolog/3_clock_check.pl:36,174,587 |
| dl6.clock | v7/emitters/1_clock.dl7:7 |
| dl6.conformance | v6/prolog/ARCH.pl:658-665 |
| dl6.conformance | v6/prolog/conformance |
| dl6.conformance | v6/prolog/compile/SCOREBOARD.md:15-50 |
| dl6.declarations | v6/prolog/1_expansion.pl:35,73 |
| dl6.declarations | v6/prolog/0_enum_expand.pl:44,61 |
| dl6.declarations | v6/prolog/0_generic_expand.pl |
| dl6.declarations | v6/prolog/0_option_expand.pl:17 |
| dl6.delta | v6/prolog/lower.pl:6987 |
| dl6.delta | v6/tsv2/runtime/diff.ts:23 |
| dl6.diag | v6/prolog/diag.pl:116,159,188 |
| dl6.diag | v6/dl/fixtures/diag-rail.dl6:1 |
| dl6.effects | v6/prolog/lower.pl:1799 |
| dl6.effects | v6/prolog/executor_modules.pl |
| dl6.effects | v6/sprefa-engine-rs/src/hosts.rs:38,99 |
| dl6.emit_dd | v6/prolog/compile/6_isolated_compiler_dd.pl:56,66 |
| dl6.emit_rust | v6/prolog/emit_rust.pl:598 |
| dl6.emit_ts | v6/prolog/emit_ts.pl:2644 |
| dl6.frontier | v6/tsv2/runtime/writeVerbs.ts:29,267 |
| dl6.frontier | v6/sprefa-engine-rs/src/write_verbs.rs:45 |
| dl6.host | v6/prolog/1_host_expand.pl:45,209,239,414 |
| dl6.interner | v6/prolog/lower.pl:3128 |
| dl6.interner | v6/tsv2/runtime/structPlane.ts:166 |
| dl6.interner | v6/tsv2/runtime/textPlane.ts:46 |
| dl6.interner | v6/tsv2/runtime/enumPlane.ts:31 |
| dl6.ivm | v6/prolog/lower.pl:5110-5627,5296 |
| dl6.ivm | v6/tsv2/runtime/1_incremental.ts:990 |
| dl6.ivm | v6/sprefa-engine-rs/src/incremental.rs:1048 |
| dl6.keys | v6/prolog/0_program_check.pl:72 |
| dl6.keys | v6/prolog/0_type_plane.pl:209 |
| dl6.lower | v6/prolog/lower.pl:7575,3151,4019,4517,6987,7859 |
| dl6.queries | v6/prolog/2_subscribe.pl:31,78 |
| dl6.queries | v6/tsv2/runtime/3_subscribe.ts:46 |
| dl6.reload_catalog | v6/tsv2/serve/reloadPlan.ts:39 |
| dl6.reload_catalog | v6/tsv2/serve/3_engine.ts:245 |
| dl6.reload_catalog | v6/dd-runner/src/main.rs:454 |
| dl6.retention | v6/prolog/analyze.pl:1305 |
| dl6.retention | v6/prolog/emit_ts.pl:1193-1203 |
| dl6.rules | v6/prolog/analyze.pl:53,82,113,1246 |
| dl6.rules | v6/prolog/0_body_walk.pl:59 |
| dl6.rules | v6/prolog/strat.pl:19 |
| dl6.rules | v6/prolog/0_program_check.pl:39 |
| dl6.storage | v6/prolog/compile/0_storage_projection.pl:24 |
| dl6.storage | v6/prolog/lower.pl:3449,3151 |
| dl6.surface | v6/prolog/compile/parse_dl_dcg.pl |
| dl6.surface | v6/prolog/print_dl.pl |
| dl6.surface | v6/prolog/compile/registry.pl:35 |
| dl6.surface | v6/prolog/compile/registry.pl:35 |
| dl6.types | v6/prolog/0_type_plane.pl:67,87,209,318 |
| dl6.types | v6/prolog/0_type_ids.pl:18,48 |
| dl6.watches | v6/tsv2/serve/2_binds.ts:209,271 |
| dl6.watches | v6/sprefa-extract/src/4_watch.rs:135,186 |
| dl7.bench | v7/bench/0_compiler_performance.pl:17,37 |
| dl7.bench | v7/receipts/11_compiler_performance_gate.md:49-59 |
| dl7.checker | v7/src/2_comptime/1_checker.pl:1,18,45 |
| dl7.checker | v7/src/2_comptime/1d_host_planner.pl:1,12 |
| dl7.compiler_cacher | v7/src/2_comptime/1c_compiler_cacher.pl:3,39 |
| dl7.compiler_tracer | v7/src/2_comptime/1b_compiler_tracer.pl:3,67 |
| dl7.compiler_tracer | v7/receipts/10_dl7_evaluator_trace.md:153-165 |
| dl7.emit.dbsp | v7/src/3_emit/1a_dbsp_plan_emitter.pl:1,114 |
| dl7.emit.dbsp | v7/src/3_emit/1b_dbsp_rust_emitter.pl:1,12 |
| dl7.emit.dbsp | v7/receipts/14_int_lt_kernel.md:119-126 |
| dl7.emit.rust | v7/src/3_emit/2_rust_type_emitter.pl:1,23 |
| dl7.emit.rust | v7/src/3_emit/2a_dl7_rust_emitter.pl:1,9 |
| dl7.emit.rust | v7/src/3_emit/3_rust_type_region_mainer.pl:1,15 |
| dl7.emit.sqlite | v7/src/3_emit/1c_sqlite_query_emitter.pl:1,41 |
| dl7.emit.sqlite | v7/receipts/14_int_lt_kernel.md:114-117 |
| dl7.emitter_protocol | v7/src/3_emit/0_logical_program_reifier.pl:1,15 |
| dl7.emitter_protocol | v7/src/3_emit/0a_logical_program_grapher.pl:1,16 |
| dl7.emitter_protocol | v7/src/3_emit/1_artifact_emitter.pl:1,22 |
| dl7.emitters | v7/emitters/0_dbsp.dl7:4,44 |
| dl7.emitters | v7/emitters/1_clock.dl7:5,25 |
| dl7.emitters | v7/emitters/2_interned_storage.dl7:4,85 |
| dl7.evaluator | v7/src/1_libtime/0_evaluator.pl:1,35 |
| dl7.evaluator | v7/receipts/9_dl7_evaluator_lookup_perf.md:90-131 |
| dl7.evaluator | v7/receipts/10_dl7_evaluator_trace.md:98-123 |
| dl7.extract_loader | v7/src/2_comptime/0c_extract_loader.pl:1,83 |
| dl7.extract_loader | v7/src/2_comptime/0c_extract_loader.pl:1 |
| dl7.host_planner | v7/src/2_comptime/1d_host_planner.pl:1,12,163 |
| dl7.kernel | v7/src/2_comptime/0_lowerer.pl:1,25 |
| dl7.kernel | v7/src/2_comptime/0a_module_lowerer.pl:1,58 |
| dl7.kernel | v7/src/2_comptime/2_compiler.pl:1,66 |
| dl7.kernel | v7/src/2_comptime/1a_generated_program_assembler.pl:1,15 |
| dl7.macrotime | v7/src/1_libtime/0a_syntax_macro_program.pl:37 |
| dl7.macrotime | v7/src/1_libtime/0b_syntax_rewriter.pl:23 |
| dl7.macrotime | v7/src/1_libtime/1_syntax_expander.pl:20 |
| dl7.macrotime | v7/macrotime/0_standard.dl7:19,71 |
| dl7.module_graph | v7/src/0_reader/4_module_loader.pl:1,29 |
| dl7.module_graph | v7/src/2_comptime/0a_module_lowerer.pl:1,58 |
| dl7.module_graph | v7/src/2_comptime/0b_filesystem_grapher.pl:1,17 |
| dl7.prelude | v7/prelude/0_constructors.dl7 |
| dl7.prelude | v7/prelude/1_declarations.dl7 |
| dl7.prelude | v7/prelude/2_constructor_rules.dl7 |
| dl7.prelude | v7/prelude/3_derived_rules.dl7 |
| dl7.prelude | v7/prelude/4_type_algebra.dl7 |
| dl7.prelude | v7/prelude/5_tsi_primitives.dl7 |
| dl7.reader | v7/src/0_reader/0_parser.pl:1,5 |
| dl7.reader | v7/src/0_reader/1_expander.pl:1,20 |
| dl7.reader | v7/src/0_reader/2_embedder.pl:1,23 |
| dl7.reader | v7/src/0_reader/3_file_loader.pl:1,14 |
| dl7.reader | v7/src/0_reader/4_module_loader.pl:1,29 |
| dl7.schema | v7/schema/0_runtime_types.dl7:4,40 |
| dl7.source_fact_loader | v7/src/2_comptime/0d_source_fact_loader.pl:1,285 |
| dl7.tool_cli | v7/src/4_tool/0_source_query_mainer.pl:1,21 |
| dl7.tool_cli | v7/src/4_tool/4_sqlite_query_mainer.pl:1,102 |
| dl7.userland | v7/prelude/0_constructors.dl7 |
| dl7.userland | v7/prelude/1_declarations.dl7 |
| dl7.userland | v7/prelude/3_derived_rules.dl7 |
| dl7.userland | v7/prelude/4_type_algebra.dl7 |
| dl7.userland | v7/applications/dl6/0_catalog.dl7:7,71 |
| dl7.userland | v7/applications/dl6/1_demo.pl:6 |
| ext.core | v6/sprefa-extract/src/lib.rs:19,52 |
| ext.core | v6/sprefa-extract/src/dispatch.rs:48 |
| ext.core | v6/sprefa-extract/src/types.rs:1 |
| ext.core | v6/sprefa-extract/src/bin/extract.rs:483 |
| ext.cpg | v6/sprefa-extract/src/cpg_decode.rs:1 |
| ext.cpg | v6/sprefa-extract/src/cpg_types.rs:1 |
| ext.cpg | v6/sprefa-extract/proto/cpg.proto:1 |
| ext.langs | v6/sprefa-extract/src/lang/mod.rs:110 |
| ext.langs | v6/sprefa-extract/src/lang/rust.rs:3342 |
| ext.langs | v6/sprefa-extract/src/lang/ts.rs:4049 |
| ext.move_rename | v6/sprefa-extract/src/0_move.rs |
| ext.move_rename | v6/sprefa-extract/src/2_move_text.rs |
| ext.move_rename | v6/sprefa-extract/src/0_rename.rs |
| ext.move_rename | v6/sprefa-extract/src/3_region_writer.rs |
| ext.scip | v6/sprefa-extract/src/scip.rs |
| ext.scip | v6/sprefa-extract/src/scip_ensure.rs:62 |
| ext.scip | v6/sprefa-extract/src/scip_v5_rels.rs |
| ext.sqlite_export | v6/sprefa-extract/src/bin/extract/0_sqlite.rs |
| ext.sqlite_export | v7/src/4_tool/5_extract_sqlite_query_mainer.pl:150 |
| ext.tsi | v6/sprefa-extract/src/tsi/registry.rs:51 |
| ext.tsi | v6/sprefa-extract/src/tsi/types.rs:1 |
| ext.tsi | v6/sprefa-extract/src/tsi/ingest.rs:64 |
| ext.watch | v6/sprefa-extract/src/4_watch.rs:135,152,186,268,328 |
| hmr | v6/tsv2/serve/reloadPlan.ts:39 |
| hmr | v6/tsv2/serve/3_engine.ts:245 |
| hmr | v6/tsv2/serve/4_http.ts:180 |
| hmr | v7/labs/19_rust_dynamic_loading/0_RESEARCH.md:450 |
| host.clocks | v6/sprefa-engine-rs/src/executors/clock.rs:30 |
| host.clocks | v6/sprefa-engine-rs/src/source_bind/_1_runtime.rs |
| host.env | v6/sprefa-engine-rs/src/executors/env.rs:14 |
| host.filesystem | v6/sprefa-engine-rs/src/hosts.rs:353 |
| host.git | v6/sprefa-engine-rs/src/executors/git_refs.rs:32 |
| host.git | v6/sprefa-engine-rs/src/executors/git_history.rs:52 |
| host.git | v6/sprefa-engine-rs/src/executors/repo_at.rs:43 |
| host.http | v6/sprefa-engine-rs/src/executors/http.rs:289,302 |
| host.shell | v6/tsv2/serve/1_hosts.ts:501,564 |
| host.soopy | v6/sprefa-engine-rs/src/hosts.rs:470 |
| host.soopy | v6/sprefa-engine-rs/src/executors/checkout.rs:21 |
| host.soopy | v6/sprefa-engine-rs/src/executors/watch.rs:49 |
| ivm.algebra | sqlite_ivm/src/0b_relational.rs:27 |
| ivm.algebra | v6/dd-runner/src/kernel.rs:53 |
| ivm.algebra | v6/sprefa-store/src/engine.rs:1 |
| ivm.sqlite | sqlite_ivm/src/0b_relational.rs:27 |
| ivm.sqlite | sqlite_ivm/src/1a_relational.rs:624 |
| ivm.sqlite | sqlite_ivm/README.md:38-53 |
| lsp.diagnostics | src/lsp.rs:44,102,537,747 |
| lsp.diagnostics | v6/dl/fixtures/diag-rail.dl6:1 |
| lsp.edits | src/lsp.rs (no WorkspaceEdit handler) |
| rt.dd_ram | v6/dd-runner/src/kernel.rs:53,352,424 |
| rt.dd_ram | v6/dd-runner/src/main.rs:66,197,454 |
| rt.executor | v7/labs/19_rust_dynamic_loading/0_RESEARCH.md:450 |
| rt.executor | v7/design/1_MINIMAL_VERTICAL_SLICE.PLAN.md:24-25 |
| rt.rust | v6/sprefa-engine-rs/src/incremental.rs:1048 |
| rt.rust | v6/sprefa-engine-rs/src/program.rs:191 |
| rt.rust | v6/sprefa-engine-rs/src/serve.rs:399 |
| rt.store | v6/sprefa-store/src/engine.rs:1 |
| rt.store | v6/sprefa-store/src/oracle.rs:1 |
| rt.store | v6/sprefa-store/js/src/engine/engine.ts:1 |
| rt.tsv2 | v6/tsv2/runtime/1_incremental.ts:990 |
| rt.tsv2 | v6/tsv2/runtime/3_subscribe.ts:46 |
| rt.tsv2 | v6/tsv2/runtime/2_boot.ts:28 |
| rt.v7 | v7/labs/19_rust_dynamic_loading/0_RESEARCH.md:450 |

## Phase axis

| binding | consumes | produces |
| --- | --- | --- |
| comptime | tsi_type_module_graph | checked_runtime_program |
| macrotime | syntax_graph | expanded_syntax |
| runtime | whole_language_facts_deltas | maintained_query_effect_relations |