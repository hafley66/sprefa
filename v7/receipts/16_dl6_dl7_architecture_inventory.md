# DL6 to DL7 shared-graph inventory

Date: 2026-09-10
Base: `32d4de862b34f9d756f455fd38eaa6dd10880b97`
Scope: source-backed capability and attachment inventory. Read-only source
investigation; no code, diagram, or kernel change.
Convention: `DL6` names the `.dl6` compiler and runtime under `v6/`; `DL7` names
the `.dl7` language and SWI-Prolog compiler under `v7/`. State is one of
`implemented | partial | planned | absent | reference`. Milestone is
`completed acceptance items / total declared acceptance items`, sourced from the
document named in `milestone_src`; `unknown/unknown` where no acceptance list is
declared. No percentage is stated; no denominator is invented.

## TOC

- [Record format](#record-format)
- [Machine-readable inventory](#machine-readable-inventory)
- [Typed edges](#typed-edges)
- [DL6 to DL7 equivalence](#dl6-to-dl7-equivalence)
- [Citations](#citations)
- [Counted summaries](#counted-summaries)

## Record format

Fields, in order, `|`-separated, one record per line:

```text
component|id|label|state|milestone|milestone_src|dl7_equiv|deps|emits|consumers|evidence|tests
edge|src|relation|dst
```

Lists inside a field use `;`. `dl7_equiv` names the DL7 node and
`full|partial|none`; DL7 nodes carry `self`.

## Machine-readable inventory

```text
component|dl7.reader|DL7 reader and CST|implemented|unknown/unknown|unknown|self|tree-sitter-dl7|reader_node;source rows;dl7_unit|dl7.kernel;dl7.module_graph|v7/src/0_reader/0_parser.pl:1,5;v7/src/0_reader/1_expander.pl:1,20;v7/src/0_reader/2_embedder.pl:1,23;v7/src/0_reader/3_file_loader.pl:1,14;v7/src/0_reader/4_module_loader.pl:1,29|v7/test/0_reader.test.pl;v7/test/1a_syntax_expander.test.pl
component|dl7.macrotime|macrotime library and syntax rewrite|implemented|unknown/unknown|unknown|self|dl7.reader;dl7.evaluator|GeneratedSyntax;syntax_frontier;syntax_source|dl7.kernel|v7/src/1_libtime/0a_syntax_macro_program.pl:37;v7/src/1_libtime/0b_syntax_rewriter.pl:23;v7/src/1_libtime/1_syntax_expander.pl:20;v7/macrotime/0_standard.dl7:19,71|v7/test/1a_syntax_expander.test.pl;v7/test/2_module_system.test.pl
component|dl7.kernel|DL7 compiler kernel, lowering, driver|implemented|10/10|v7/tasks/00_PROGRESS.md:24-34|self|dl7.reader;dl7.macrotime;dl7.checker|checked Datalog IR;compiled_unit|dl7.evaluator;dl7.emitter_protocol|v7/src/2_comptime/0_lowerer.pl:1,25;v7/src/2_comptime/0a_module_lowerer.pl:1,58;v7/src/2_comptime/2_compiler.pl:1,66;v7/src/2_comptime/1a_generated_program_assembler.pl:1,15|v7/test/1_entrypoints.test.pl;v7/test/18_binding_symmetry.test.pl;v7/test/19_lexical_binding.test.pl
component|dl7.evaluator|shared stratified Datalog evaluator|implemented|unknown/unknown|unknown|self|dl7.kernel|sorted closure;diagnostics|dl7.compiler_tracer;dl7.checker|v7/src/1_libtime/0_evaluator.pl:1,35;v7/receipts/9_dl7_evaluator_lookup_perf.md:90-131;v7/receipts/10_dl7_evaluator_trace.md:98-123|v7/test/1_entrypoints.test.pl;v7/test/3_compiler_trace.test.pl
component|dl7.checker|checked Datalog resolution, safety, strata|implemented|unknown/unknown|unknown|self|dl7.kernel|check diagnostics;relation/2 keysets|dl7.kernel;dl7.emitter_protocol|v7/src/2_comptime/1_checker.pl:1,18,45;v7/src/2_comptime/1d_host_planner.pl:1,12|v7/test/1_entrypoints.test.pl
component|dl7.module_graph|multi-unit module graph and environments|implemented|unknown/unknown|unknown|self|dl7.reader;dl7.kernel|module owners;basements;imported env|dl7.kernel|v7/src/0_reader/4_module_loader.pl:1,29;v7/src/2_comptime/0a_module_lowerer.pl:1,58;v7/src/2_comptime/0b_filesystem_grapher.pl:1,17|v7/test/2_module_system.test.pl;v7/test/1_entrypoints.test.pl
component|dl7.userland|userland type algebra and DL6 catalog app|implemented|22/22|v7/receipts/3_dl6_userland_integration.md:71|self|dl7.kernel;dl7.prelude|storage layout rows;wrapper identity|dl7.emitters|v7/prelude/0_constructors.dl7;v7/prelude/1_declarations.dl7;v7/prelude/3_derived_rules.dl7;v7/prelude/4_type_algebra.dl7;v7/applications/dl6/0_catalog.dl7:7,71;v7/applications/dl6/1_demo.pl:6|v7/test/2_module_system.test.pl;v7/test/15_interned_storage.test.pl;v7/test/16_storage_plain_field.test.pl;v7/test/17_dl6_interned.test.pl;v7/test/18_binding_symmetry.test.pl
component|dl7.emitter_protocol|target-neutral reification and artifact emission|implemented|unknown/unknown|unknown|self|dl7.checker|logical_program_rows;compiler_view;artifacts|dl7.emit.dbsp;dl7.emit.sqlite;dl7.emit.rust|v7/src/3_emit/0_logical_program_reifier.pl:1,15;v7/src/3_emit/0a_logical_program_grapher.pl:1,16;v7/src/3_emit/1_artifact_emitter.pl:1,22|v7/test/9_dbsp_plan.test.pl;v7/test/1_entrypoints.test.pl
component|dl7.emit.dbsp|DBSP JSON and Rust plan emitters|partial|4/4|v7/README.md:178-181|self|dl7.emitter_protocol|dd-runner operator JSON;native Rust constructors|rt.rust;rt.dd_ram|v7/src/3_emit/1a_dbsp_plan_emitter.pl:1,114;v7/src/3_emit/1b_dbsp_rust_emitter.pl:1,12;v7/receipts/14_int_lt_kernel.md:119-126|v7/test/9_dbsp_plan.test.pl;v7/test/11_dbsp_rust_emitter.test.pl
component|dl7.emit.sqlite|SQLite query and IVM install emitter|partial|5/5|v7/README.md:131-133|self|dl7.emitter_protocol;ivm.sqlite|SELECT SQL;IVM install SQL|ivm.sqlite|v7/src/3_emit/1c_sqlite_query_emitter.pl:1,41;v7/receipts/14_int_lt_kernel.md:114-117|v7/test/14_sqlite_query_emitter.test.pl
component|dl7.emit.rust|Rust type and runtime region emitters|implemented|unknown/unknown|unknown|self|dl7.emitter_protocol;dl7.schema|Rust types;Soopy-owned region|rt.rust|v7/src/3_emit/2_rust_type_emitter.pl:1,23;v7/src/3_emit/2a_dl7_rust_emitter.pl:1,9;v7/src/3_emit/3_rust_type_region_mainer.pl:1,15|v7/test/6_rust_type_emitter.test.pl;v7/test/10_dl7_rust_emitter.test.pl;v7/test/7_rust_type_region.e2e.pl
component|dl7.prelude|DL7 prelude declarations and type algebra|implemented|unknown/unknown|unknown|self|dl7.kernel|kernel relation declarations;type-algebra rules|dl7.userland;dl7.checker|v7/prelude/0_constructors.dl7;v7/prelude/1_declarations.dl7;v7/prelude/2_constructor_rules.dl7;v7/prelude/3_derived_rules.dl7;v7/prelude/4_type_algebra.dl7;v7/prelude/5_tsi_primitives.dl7|v7/test/1_entrypoints.test.pl;v7/test/15_interned_storage.test.pl
component|dl7.emitters|userland policy emitter programs|implemented|unknown/unknown|unknown|self|dl7.userland|dbsp rows;clock rows;interned storage layout|dl7.emitter_protocol|v7/emitters/0_dbsp.dl7:4,44;v7/emitters/1_clock.dl7:5,25;v7/emitters/2_interned_storage.dl7:4,85|v7/test/15_interned_storage.test.pl;v7/test/9_dbsp_plan.test.pl
component|dl7.schema|DL7 runtime/wire type schema|implemented|unknown/unknown|unknown|self|dl7.kernel|Rust wire vocabulary|dl7.emit.rust;rt.rust|v7/schema/0_runtime_types.dl7:4,40|v7/test/10_dl7_rust_emitter.test.pl
component|dl7.extract_loader|TSI stream loader and registry validation|implemented|unknown/unknown|unknown|self|ext.tsi;dl7.kernel|TSI type graph rows|dl7.checker|v7/src/2_comptime/0c_extract_loader.pl:1,83;v7/src/2_comptime/0c_extract_loader.pl:1|v7/test/4_extract_loader.test.pl;v7/test/8_source_query.test.pl
component|dl7.source_fact_loader|source-fact envelope to span/occurrence graph|implemented|unknown/unknown|unknown|self|ext.core;dl7.kernel|content-span/occurrence identities|dl7.checker|v7/src/2_comptime/0d_source_fact_loader.pl:1,285|v7/test/5_source_fact_loader.test.pl
component|dl7.host_planner|hosted external operator shape validation|partial|unknown/unknown|unknown|self|dl7.checker;host.soopy|host planning rows|dl7.kernel|v7/src/2_comptime/1d_host_planner.pl:1,12,163|v7/test/1_entrypoints.test.pl
component|dl7.compiler_tracer|phase/step compile trace ledger|implemented|unknown/unknown|unknown|self|dl7.evaluator|COMPILE-TRACE;JSONL steps|dl7.bench|v7/src/2_comptime/1b_compiler_tracer.pl:3,67;v7/receipts/10_dl7_evaluator_trace.md:153-165|v7/test/3_compiler_trace.test.pl;v7/test/20_compiler_performance.test.pl
component|dl7.compiler_cacher|content-addressed compile/prelude cache|implemented|unknown/unknown|unknown|self|dl7.kernel|process-local cache|dl7.kernel;dl7.bench|v7/src/2_comptime/1c_compiler_cacher.pl:3,39|v7/test/20_compiler_performance.test.pl
component|dl7.tool_cli|CLI mainers over the compiler|implemented|unknown/unknown|unknown|self|dl7.emit.dbsp;dl7.emit.sqlite;dl7.emit.rust;ext.core|JSON query output;plan JSON;region check/apply|shell|v7/src/4_tool/0_source_query_mainer.pl:1,21;v7/src/4_tool/4_sqlite_query_mainer.pl:1,102|v7/test/8_source_query.test.pl;v7/test/14_sqlite_query_emitter.test.pl
component|dl7.bench|compiler performance budget gate|implemented|17/17|v7/receipts/11_compiler_performance_gate.md:217-237|self|dl7.compiler_tracer;dl7.compiler_cacher|DL7-PERF report;exit code|CI|v7/bench/0_compiler_performance.pl:17,37;v7/receipts/11_compiler_performance_gate.md:49-59|v7/test/20_compiler_performance.test.pl
component|dl6.surface|DL6 surface grammar, DCG parser, printer, registry|implemented|unknown/unknown|unknown|dl7.reader:partial;dl7.checker:partial|none|prog/2;plan/9|dl6.declarations;dl6.lower|v6/prolog/compile/parse_dl_dcg.pl;v6/prolog/print_dl.pl;v6/prolog/compile/registry.pl:35;v6/prolog/compile/registry.pl:35|v6/prolog/compile/test/plunit_tests.pl;v6/prolog/compile/scripts/text_door_receipt.pl
component|dl6.declarations|declaration sugar and expansion phases|implemented|unknown/unknown|unknown|dl7.kernel:partial;dl7.prelude:partial|dl6.surface|expanded prog term|dl6.types;dl6.rules;dl6.lower|v6/prolog/1_expansion.pl:35,73;v6/prolog/0_enum_expand.pl:44,61;v6/prolog/0_generic_expand.pl;v6/prolog/0_option_expand.pl:17|v6/prolog/compile/test/plunit_tests.pl
component|dl6.rules|analysis, body walk, stratification, program checks|implemented|unknown/unknown|unknown|dl7.kernel:partial;dl7.checker:full|dl6.declarations|analysis facts;strata order;violations|dl6.lower|v6/prolog/analyze.pl:53,82,113,1246;v6/prolog/0_body_walk.pl:59;v6/prolog/strat.pl:19;v6/prolog/0_program_check.pl:39|v6/prolog/compile/test/plunit_tests.pl
component|dl6.types|type plane, semantic type ids, topological order|implemented|unknown/unknown|unknown|dl7.prelude:full;dl7.checker:partial|dl6.declarations|type rows;storage mapping|dl6.storage;dl6.lower|v6/prolog/0_type_plane.pl:67,87,209,318;v6/prolog/0_type_ids.pl:18,48|v6/prolog/compile/test/type_relation_ir.test.pl
component|dl6.keys|declared relation keys and key checks|implemented|unknown/unknown|unknown|dl7.checker:partial|dl6.types|key rows;key conflicts|dl6.lower|v6/prolog/0_program_check.pl:72;v6/prolog/0_type_plane.pl:209|v6/prolog/compile/test/plunit_tests.pl
component|dl6.storage|storage projection and struct type plans|implemented|unknown/unknown|unknown|dl7.emitters:partial;dl7.emit.sqlite:partial|dl6.types|storage rows;relplans;DDL/SQL|dl6.lower;dl6.interner|v6/prolog/compile/0_storage_projection.pl:24;v6/prolog/lower.pl:3449,3151|v6/prolog/compile/test/0_storage_projection.test.pl
component|dl6.interner|program text intern plan and runtime intern planes|implemented|unknown/unknown|unknown|dl7.userland:partial|dl6.storage|intern plan;interned ids|dl6.lower;rt.tsv2|v6/prolog/lower.pl:3128;v6/tsv2/runtime/structPlane.ts:166;v6/tsv2/runtime/textPlane.ts:46;v6/tsv2/runtime/enumPlane.ts:31|v6/tsv2/tests
component|dl6.lower|rule to target-neutral lowered plan|implemented|unknown/unknown|unknown|dl7.kernel:partial|dl6.rules;dl6.storage;dl6.types|lowered/8;boot statements|dl6.emit_ts;dl6.emit_rust;dl6.emit_dd|v6/prolog/lower.pl:7575,3151,4019,4517,6987,7859|v6/prolog/compile/test/plunit_tests.pl;v6/prolog/test/run_sql_check.pl
component|dl6.emit_ts|TypeScript target emitter|implemented|unknown/unknown|unknown|dl7.emit.dbsp:none;dl7.emit.rust:none|dl6.lower|emitted .ts module|rt.tsv2|v6/prolog/emit_ts.pl:2644|v6/prolog/compile/test/emit_rust.test.pl;v6/tsv2/tests/tickLoop.test.ts
component|dl6.emit_rust|Rust target emitter|implemented|unknown/unknown|unknown|dl7.emit.rust:partial|dl6.lower|emitted .rs module|rt.rust|v6/prolog/emit_rust.pl:598|v6/prolog/compile/test/emit_rust.test.pl
component|dl6.emit_dd|isolated DD plan JSON emitter|implemented|unknown/unknown|unknown|dl7.emit.dbsp:partial|dl6.lower|dd_plan JSON|rt.dd_ram|v6/prolog/compile/6_isolated_compiler_dd.pl:56,66|v6/prolog/compile/test/6_isolated_compiler_dd.test.pl
component|dl6.ivm|incremental view maintenance lowering and runtime|implemented|unknown/unknown|unknown|ivm.sqlite:full;dl7.kernel:partial|dl6.storage|refcount/DRed SQL;tick deltas|dl6.delta;ivm.sqlite;rt.tsv2|v6/prolog/lower.pl:5110-5627,5296;v6/tsv2/runtime/1_incremental.ts:990;v6/sprefa-engine-rs/src/incremental.rs:1048|v6/prolog/compile/test/plunit_tests.pl;v6/tsv2/tests
component|dl6.delta|delta rows and boundary diffs|implemented|unknown/unknown|unknown|dl7.evaluator:partial|dl6.ivm|delta lists;before/after diff|rt.tsv2;rt.rust|v6/prolog/lower.pl:6987;v6/tsv2/runtime/diff.ts:23|v6/tsv2/tests
component|dl6.frontier|frontier and support-count write strategies|implemented|unknown/unknown|unknown|none|dl6.delta|shared frontier tables|rt.tsv2;rt.rust|v6/tsv2/runtime/writeVerbs.ts:29,267;v6/sprefa-engine-rs/src/write_verbs.rs:45|v6/tsv2/tests/shared_frontier
component|dl6.retention|retention bounds and history pruning|implemented|unknown/unknown|unknown|none|dl6.ivm|retention guards;prune SQL|dl6.lower|v6/prolog/analyze.pl:1305;v6/prolog/emit_ts.pl:1193-1203|v6/prolog/compile/test/plunit_tests.pl
component|dl6.host|host declaration expansion and external relations|implemented|unknown/unknown|unknown|dl7.host_planner:partial|dl6.declarations|demand/response rels;host plans|dl6.bind;dl6.effects;rt.rust|v6/prolog/1_host_expand.pl:45,209,239,414|v6/prolog/compile/test/typed_host_contracts.test.pl
component|dl6.bind|bind operator and interval/watch bind runners|implemented|unknown/unknown|unknown|none|dl6.host|bind plans;arrival rows|rt.tsv2|v6/prolog/1_host_expand.pl:491;v6/tsv2/serve/2_binds.ts:139,373;v6/sprefa-engine-rs/src/source_bind/mod.rs|v6/tsv2/tests
component|dl6.effects|effect port rows and executor modules|implemented|unknown/unknown|unknown|none|dl6.host|port rows;executor binding|rt.rust;rt.tsv2|v6/prolog/lower.pl:1799;v6/prolog/executor_modules.pl;v6/sprefa-engine-rs/src/hosts.rs:38,99|v6/prolog/compile/test/typed_host_contracts.test.pl
component|dl6.watches|filesystem/revision watch sources|implemented|unknown/unknown|unknown|none|dl6.host;dl6.bind|watch batches;generation rows|dl6.queries|v6/tsv2/serve/2_binds.ts:209,271;v6/sprefa-extract/src/4_watch.rs:135,186|v6/sprefa-extract/tests;v7/test/12_watch_pipeline.e2e.pl
component|dl6.queries|query cone subscription and pruning|implemented|unknown/unknown|unknown|none|dl6.rules|subscribed relation set|dl6.lower;rt.tsv2|v6/prolog/2_subscribe.pl:31,78;v6/tsv2/runtime/3_subscribe.ts:46|v6/tsv2/tests/subscribePrune.test.ts
component|dl6.reload_catalog|program reload planner and runtime catalog|implemented|unknown/unknown|unknown|hmr:partial|dl6.storage|create/recreate/refill/keep/drop plan;__dl7_catalog|rt.tsv2;rt.dd_ram|v6/tsv2/serve/reloadPlan.ts:39;v6/tsv2/serve/3_engine.ts:245;v6/dd-runner/src/main.rs:454|v6/tsv2/tests/reloadPlan.test.ts;v6/tsv2/tests/serveReload.test.ts
component|dl6.clock|clock dependency and tick-ring grading|implemented|unknown/unknown|unknown|dl7.emitters:partial|dl6.rules|clock facts;refusals|dl6.lower|v6/prolog/3_clock_check.pl:36,174,587;v7/emitters/1_clock.dl7:7|v6/prolog/compile/test/3_clock_check.test.pl
component|dl6.diag|diagnostics to LSP records|implemented|unknown/unknown|unknown|lsp.diagnostics:partial|dl6.surface|diag records;diag_v5|v5 dl LSP|v6/prolog/diag.pl:116,159,188;v6/dl/fixtures/diag-rail.dl6:1|v6/prolog/compile/test/diag.test.pl
component|dl6.conformance|conformance corpus and battery of record|reference|163/163|v6/prolog/ARCH.pl:658|none|none|conformance PASS;plunit;oracle snapshots|dl7.checker;dl6.rules|v6/prolog/ARCH.pl:658-665;v6/prolog/conformance;v6/prolog/compile/SCOREBOARD.md:15-50|v6/prolog/conformance/go.pl
component|ext.core|sprefa-extract crate core and dispatch|implemented|unknown/unknown|unknown|dl7.source_fact_loader:partial|none|family bundles;signed rows;cache|ext.langs;ext.tsi;ext.watch|v6/sprefa-extract/src/lib.rs:19,52;v6/sprefa-extract/src/dispatch.rs:48;v6/sprefa-extract/src/types.rs:1;v6/sprefa-extract/src/bin/extract.rs:483|v6/sprefa-extract/tests
component|ext.langs|per-language extraction front ends|implemented|unknown/unknown|unknown|none|ext.core|CST/type/call/df families|ext.tsi|v6/sprefa-extract/src/lang/mod.rs:110;v6/sprefa-extract/src/lang/rust.rs:3342;v6/sprefa-extract/src/lang/ts.rs:4049|v6/sprefa-extract/tests/4_capability_parity.rs
component|ext.tsi|typed symbol index protocol and registry|implemented|unknown/unknown|unknown|dl7.extract_loader:full|ext.core|TSI JSONL;witness;coverage|dl7.extract_loader;dl6.watches|v6/sprefa-extract/src/tsi/registry.rs:51;v6/sprefa-extract/src/tsi/types.rs:1;v6/sprefa-extract/src/tsi/ingest.rs:64|v6/sprefa-extract/tests/100_tsi_intersection.rs
component|ext.scip|SCIP indexer produce/load|partial|unknown/unknown|unknown|none|ext.core|scip_* relation rows|dl7.extract_loader|v6/sprefa-extract/src/scip.rs;v6/sprefa-extract/src/scip_ensure.rs:62;v6/sprefa-extract/src/scip_v5_rels.rs|v6/sprefa-extract/tests/8_scip_families_cli.rs;v6/sprefa-extract/tests/scip_freshness.rs
component|ext.cpg|Joern CPG decode|partial|unknown/unknown|unknown|none|ext.core|CpgNode/CpgEdge/CpgProperty|dl7.source_fact_loader|v6/sprefa-extract/src/cpg_decode.rs:1;v6/sprefa-extract/src/cpg_types.rs:1;v6/sprefa-extract/proto/cpg.proto:1|v6/sprefa-extract/tests
component|ext.watch|named-ref watch protocol and generations|implemented|unknown/unknown|unknown|none|ext.core;ext.tsi|snapshot/delta batches;signed rows|dl6.watches;dl7.tool_cli|v6/sprefa-extract/src/4_watch.rs:135,152,186,268,328|v7/test/12_watch_pipeline.e2e.pl
component|ext.move_rename|move and rename rewriters|implemented|unknown/unknown|unknown|none|ext.core|rewritten files;SCIP cross-check|dl7.tool_cli|v6/sprefa-extract/src/0_move.rs;v6/sprefa-extract/src/2_move_text.rs;v6/sprefa-extract/src/0_rename.rs;v6/sprefa-extract/src/3_region_writer.rs|v6/sprefa-extract/tests
component|ext.sqlite_export|extract SQLite snapshot exporter|implemented|unknown/unknown|unknown|ivm.sqlite:partial|ext.tsi|snapshot database|dl7.emit.sqlite|v6/sprefa-extract/src/bin/extract/0_sqlite.rs;v7/src/4_tool/5_extract_sqlite_query_mainer.pl:150|v7/test/14_sqlite_query_emitter.test.pl
component|host.filesystem|filesystem host executor|implemented|unknown/unknown|unknown|none|rt.rust;host.soopy|/soopy/files,/soopy/files_at rows|dl6.host|v6/sprefa-engine-rs/src/hosts.rs:353|v6/sprefa-engine-rs/tests
component|host.git|Git refs, history, repo-at hosts|implemented|unknown/unknown|unknown|none|rt.rust|git_ref,git_tag,change,rename,repo_files_at|dl6.queries|v6/sprefa-engine-rs/src/executors/git_refs.rs:32;v6/sprefa-engine-rs/src/executors/git_history.rs:52;v6/sprefa-engine-rs/src/executors/repo_at.rs:43|v6/sprefa-engine-rs/tests
component|host.shell|shell/process host executor|implemented|unknown/unknown|unknown|none|rt.tsv2|host rows|dl6.effects|v6/tsv2/serve/1_hosts.ts:501,564|v6/tsv2/tests
component|host.http|HTTP get/post hosts|implemented|unknown/unknown|unknown|none|rt.rust|http response rows|dl6.host|v6/sprefa-engine-rs/src/executors/http.rs:289,302|v6/sprefa-engine-rs/tests
component|host.clocks|clock tick host|implemented|unknown/unknown|unknown|none|rt.rust|/clock/tick rows|dl6.clock|v6/sprefa-engine-rs/src/executors/clock.rs:30;v6/sprefa-engine-rs/src/source_bind/_1_runtime.rs|v6/sprefa-engine-rs/tests
component|host.env|environment variable host|implemented|unknown/unknown|unknown|none|rt.rust|/env/var rows|dl6.host|v6/sprefa-engine-rs/src/executors/env.rs:14|v6/sprefa-engine-rs/tests
component|host.soopy|Soopy stage/commit/checkout hosts|implemented|unknown/unknown|unknown|none|rt.rust|stage/commit/checkout rows|dl6.effects;dl7.tool_cli|v6/sprefa-engine-rs/src/hosts.rs:470;v6/sprefa-engine-rs/src/executors/checkout.rs:21;v6/sprefa-engine-rs/src/executors/watch.rs:49|v7/test/7_rust_type_region.e2e.pl
component|lsp.diagnostics|DL LSP diagnostic publishing|implemented|unknown/unknown|unknown|dl6.diag:partial|dl6.diag|textDocument/publishDiagnostics|editor|src/lsp.rs:44,102,537,747;v6/dl/fixtures/diag-rail.dl6:1|v6/tsv2/scripts/lsp-diags.sh
component|lsp.edits|LSP rename/codeAction/workspace edits|absent|unknown/unknown|unknown|none|none|none|none|none|src/lsp.rs (no WorkspaceEdit handler)|none
component|ivm.algebra|shared generic IVM operator algebra|absent|unknown/unknown|unknown|none|none|none|none|sqlite_ivm/src/0b_relational.rs:27;v6/dd-runner/src/kernel.rs:53;v6/sprefa-store/src/engine.rs:1|none
component|ivm.sqlite|SQLite transactional IVM extension|implemented|64/64|sqlite_ivm/README.md:54-58|dl6.ivm:full;dl7.emit.sqlite:full|none|maintained result tables;__ivm_* catalogs|dl7.emit.sqlite;dl6.storage|sqlite_ivm/src/0b_relational.rs:27;sqlite_ivm/src/1a_relational.rs:624;sqlite_ivm/README.md:38-53|sqlite_ivm/scripts/9_verify.sh;sqlite_ivm/bench/46_feature_acceptance.md
component|rt.tsv2|TypeScript tsv2 incremental runtime|implemented|unknown/unknown|unknown|dl6.ivm:full;dl6.reload_catalog:full|dl6.emit_ts|tick deltas;IReloadPlan|dl6.reload_catalog|v6/tsv2/runtime/1_incremental.ts:990;v6/tsv2/runtime/3_subscribe.ts:46;v6/tsv2/runtime/2_boot.ts:28|v6/tsv2/tests
component|rt.rust|sprefa-engine-rs generated-program runtime|implemented|unknown/unknown|unknown|dl6.emit_rust:partial|dl6.emit_rust|tick engine;unix-socket HTTP API|host.filesystem;host.git;host.http|v6/sprefa-engine-rs/src/incremental.rs:1048;v6/sprefa-engine-rs/src/program.rs:191;v6/sprefa-engine-rs/src/serve.rs:399|v6/sprefa-engine-rs/tests
component|rt.dd_ram|dd-runner RAM kernel|partial|unknown/unknown|unknown|dl7.emit.dbsp:partial|dl7.emit.dbsp|closure rows;tick deltas|dl7.emit.dbsp;dl6.reload_catalog|v6/dd-runner/src/kernel.rs:53,352,424;v6/dd-runner/src/main.rs:66,197,454|v6/dd-runner/grade.sh;v7/test/13_sqlite_plan.e2e.pl
component|rt.store|sprefa-store cascade engine with dd/salsa oracles|implemented|unknown/unknown|unknown|none|none|cascade deltas;reconcile;reach|bench;oracle|v6/sprefa-store/src/engine.rs:1;v6/sprefa-store/src/oracle.rs:1;v6/sprefa-store/js/src/engine/engine.ts:1|v6/sprefa-store/tests
component|hmr|program hot reload and generation-boundary swap|partial|unknown/unknown|unknown|dl6.reload_catalog:full|rt.tsv2;dl6.reload_catalog|reload plan;drop/refill/keep|rt.tsv2|v6/tsv2/serve/reloadPlan.ts:39;v6/tsv2/serve/3_engine.ts:245;v6/tsv2/serve/4_http.ts:180;v7/labs/19_rust_dynamic_loading/0_RESEARCH.md:450|v6/tsv2/tests/serveReload.test.ts
component|dd.backend|differential-dataflow backend|planned|unknown/unknown|unknown|none|none|none|none|sqlite_ivm/bench/shared/34_circuit_dd.rs;sqlite_ivm/bench/shared/22_crossover_dd.rs|sqlite_ivm/scripts/16_shootout.sh
edge|dl7.kernel|depends_on|dl7.reader
edge|dl7.kernel|depends_on|dl7.macrotime
edge|dl7.evaluator|depends_on|dl7.kernel
edge|dl7.checker|depends_on|dl7.kernel
edge|dl7.module_graph|depends_on|dl7.reader
edge|dl7.module_graph|depends_on|dl7.kernel
edge|dl7.userland|depends_on|dl7.kernel
edge|dl7.userland|depends_on|dl7.prelude
edge|dl7.emitter_protocol|depends_on|dl7.checker
edge|dl7.emit.dbsp|depends_on|dl7.emitter_protocol
edge|dl7.emit.sqlite|depends_on|dl7.emitter_protocol
edge|dl7.emit.rust|depends_on|dl7.emitter_protocol
edge|dl7.emit.dbsp|lowers_to|rt.dd_ram
edge|dl7.emit.sqlite|lowers_to|ivm.sqlite
edge|dl7.emit.rust|emits|rt.rust
edge|dl7.bench|depends_on|dl7.compiler_cacher
edge|dl7.bench|depends_on|dl7.compiler_tracer
edge|dl7.extract_loader|reads|ext.tsi
edge|dl7.source_fact_loader|reads|ext.core
edge|dl7.host_planner|hosts|host.soopy
edge|dl7.tool_cli|emits|ext.watch
edge|dl6.surface|depends_on|none
edge|dl6.declarations|depends_on|dl6.surface
edge|dl6.rules|depends_on|dl6.declarations
edge|dl6.types|depends_on|dl6.declarations
edge|dl6.keys|depends_on|dl6.types
edge|dl6.storage|depends_on|dl6.types
edge|dl6.interner|depends_on|dl6.storage
edge|dl6.lower|depends_on|dl6.rules
edge|dl6.lower|depends_on|dl6.storage
edge|dl6.emit_ts|depends_on|dl6.lower
edge|dl6.emit_rust|depends_on|dl6.lower
edge|dl6.emit_dd|depends_on|dl6.lower
edge|dl6.ivm|depends_on|dl6.storage
edge|dl6.delta|depends_on|dl6.ivm
edge|dl6.frontier|depends_on|dl6.delta
edge|dl6.retention|depends_on|dl6.ivm
edge|dl6.host|depends_on|dl6.declarations
edge|dl6.bind|depends_on|dl6.host
edge|dl6.effects|depends_on|dl6.host
edge|dl6.watches|effects|dl6.host
edge|dl6.queries|reads|dl6.rules
edge|dl6.reload_catalog|reloads|dl6.storage
edge|dl6.clock|depends_on|dl6.rules
edge|dl6.ivm|emits|ivm.sqlite
edge|dl6.ivm|emits|rt.tsv2
edge|dl6.delta|emits|rt.tsv2
edge|dl6.host|hosts|rt.rust
edge|dl6.effects|hosts|rt.rust
edge|dl6.watches|watches|ext.watch
edge|ext.watch|emits|ext.tsi
edge|ext.core|hosts|ext.langs
edge|ext.core|emits|ext.tsi
edge|ext.scip|emits|ext.tsi
edge|ext.cpg|emits|ext.tsi
edge|ext.move_rename|writes|ext.core
edge|ext.sqlite_export|writes|ivm.sqlite
edge|host.filesystem|depends_on|host.soopy
edge|host.git|depends_on|rt.rust
edge|host.shell|depends_on|rt.tsv2
edge|host.http|depends_on|rt.rust
edge|host.clocks|depends_on|rt.rust
edge|host.env|depends_on|rt.rust
edge|host.soopy|depends_on|rt.rust
edge|lsp.diagnostics|reads|dl6.diag
edge|ivm.sqlite|lowers_to|ivm.algebra
edge|rt.tsv2|reads|dl6.ivm
edge|rt.rust|reads|dl6.emit_rust
edge|rt.dd_ram|reads|dl7.emit.dbsp
edge|rt.store|depends_on|none
edge|hmr|reloads|rt.tsv2
edge|dd.backend|later_target|ivm.algebra
edge|dl6.conformance|watches|dl6.rules
```

## Typed edges

The `edge|src|relation|dst` records above use the task vocabulary:
`depends_on`, `emits`, `lowers_to`, `hosts`, `reads`, `writes`, `watches`,
`reloads`, `effects`, `later_target`. `none` as a `dst` marks a root or an
absent target.

## DL6 to DL7 equivalence

| DL6 facility | DL7 node | Class | Basis |
|---|---|---|---|
| declarations / sugar expanders | `dl7.kernel`, `dl7.prelude` | partial | DL7 has Lisp-shaped prefix forms and prelude rules; DL6 decl spellings are dropped (`v7/audit/results/1_READER.md`, `2_EXPANSION.md`) |
| rules / strata / program checks | `dl7.kernel`, `dl7.checker` | full | stratified evaluator plus checked goal IR (`v7/src/2_comptime/1_checker.pl`, `1_libtime/0_evaluator.pl`) |
| types / semantic ids / topological order | `dl7.prelude`, `dl7.checker` | partial | `named/primitive/application` identities and type algebra in prelude; module identity ruling still open |
| keys / functional-key checks | `dl7.checker` | partial | `validate_functional_rows` in DL7; DL6 key surface dropped |
| storage / intern plans | `dl7.emitters`, `dl7.emit.sqlite` | partial | userland `2_interned_storage.dl7` describes layout; no persistent interning |
| IVM / delta / frontier / retention | `ivm.sqlite`, `dl6.ivm` | full (donor) | DL7 emits to the same SQLite IVM via `1c_sqlite_query_emitter.pl`; no DL7 frontier/retention construct yet |
| host / bind / effects | `dl7.host_planner` | partial | DL7 validates `Hosted/HostPort` shape; executor binding stays in `sprefa-engine-rs` |
| watches / queries / reload catalog | none | none | DL7 has no watcher, subscription cone, or catalog reload; `ext.watch` and `rt.*` remain the runtime |
| clock | `dl7.emitters` | partial | `1_clock.dl7` grades level clock roles zero; DL6 `3_clock_check.pl` is the full checker |
| diagnostics / LSP | `lsp.diagnostics` | partial | LSP remains the v5 `src/lsp.rs` reading `diag_v5`; DL6 `diag.pl` is donor |

## Citations

Absolute paths are relative to the base worktree. Primary sources:

- `v7/README.md:16-266` boundary, artifacts, runtime shootout.
- `v7/AGENTS.md:8-32` performance gates; `v7/tasks/00_PROGRESS.md:1-621` milestone ledger.
- `v7/design/1_MINIMAL_VERTICAL_SLICE.PLAN.md:1-835` kernel contract; `v7/design/4_TYPE_ALGEBRA.md:1-195` type algebra.
- `v7/audit/0_SHARED.md:1-56` audit protocol; `v7/audit/results/0_INDEX.md:1-72` reuse classes.
- `v7/audit/results/{1..12}` per-slice predicate class counts.
- `v7/receipts/3_dl6_userland_integration.md`; `9_dl7_evaluator_lookup_perf.md`; `10_dl7_evaluator_trace.md`; `11_compiler_performance_gate.md`; `12_current_stratum_evaluation.md`; `14_int_lt_kernel.md`.
- `v6/prolog/ARCH.pl:658-677` battery of record; `v6/prolog/compile/registry.pl:35` registry rows; `v6/prolog/compile/SCOREBOARD.md:15-50` sweep totals.
- `v6/README.md:1-110` v6 crate framing; `v6/AGENTS.md:1-60` crate and cascade law.
- `sqlite_ivm/README.md:1-203` supported shapes and acceptance.
- `v6/sprefa-extract/src/tsi/registry.rs:51-207` TSI relation table; `4_watch.rs:135-394` watch protocol.
- `v6/sprefa-engine-rs/src/hosts.rs:38-137` executor roster.
- `v6/tsv2/serve/reloadPlan.ts:39-67`; `3_engine.ts:245-302` reload contract.
- `v6/dd-runner/src/kernel.rs:53-424`; `main.rs:454-485` catalog guard.
- `src/lsp.rs:44-1884` LSP surface.

## Counted summaries

| Metric | Count |
|---|---|
| components (DL7) | 21 |
| components (DL6) | 24 |
| components (extraction/hosts/LSP) | 17 |
| components (backends) | 8 |
| components total | 70 |
| typed edges | 74 |
| evidence path references | 176 |
| milestone with sourced denominator | 7 |
| milestone unknown denominator | 63 |
| state implemented | 59 |
| state partial | 7 |
| state absent | 2 |
| state planned | 1 |
| state reference | 1 |
| DL6 facilities with full DL7 equivalent | 2 (rules/strata/checks, IVM lowering chain) |
| DL6 facilities with partial DL7 equivalent | 7 |
| DL6 facilities with zero DL7 equivalent | 3 (watches/queries/reload catalog, bind runners, frontier write strategies) |
