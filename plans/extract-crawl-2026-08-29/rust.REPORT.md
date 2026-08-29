# sprefa-extract over rust-analyzer: entrypoint crawl

Binary `v6/sprefa-extract/target/release/extract` at `cec3d5c1d`.
Corpus `/Users/chrishafley/projects/rust-analyzer` at `af4111f`, read-only.
Every `extract` call ran under `timeout 10`. Raw tables sit beside this file.

## Contents

1. [What was measured](#1-what-was-measured)
2. [Step 1: per-file battery](#2-step-1-per-file-battery)
3. [Step 2: whole-project resolve](#3-step-2-whole-project-resolve)
4. [Step 2b: why 89,500 sites resolve to nothing](#4-step-2b-why-89500-sites-resolve-to-nothing)
5. [Step 3: entrypoint crawl](#5-step-3-entrypoint-crawl)
6. [Step 4: scip comparison](#6-step-4-scip-comparison)
7. [Kinks](#7-kinks)
8. [Fixtures](#8-fixtures)
9. [What stays untested and why](#9-what-stays-untested-and-why)
10. [Two corrections to the brief](#10-two-corrections-to-the-brief)

## 1. What was measured

| item | value |
|---|---|
| `.rs` files found | 1,481 |
| excluded from the resolve and crawl arms | 540 under `crates/parser/test_data/**` and `crates/syntax/test_data/**` (deliberately malformed parser fixtures; they were still run through step 1 and are reported separately) |
| excluded as vendored or built | none present; the clone has no `target/` and no vendored tree |
| src bucket used everywhere else | 941 files, 17,360,937 B, 582,761 lines |

`crates/proc-macro-srv/proc-macro-test/imp` is a Cargo workspace `exclude` and
holds no `.rs` file in this clone, so nothing was dropped for it.

## 2. Step 1: per-file battery

`timeout 10 extract <file>`, one process per file, 8 workers.
Raw: `rust.runs.tsv` (`path bucket rc ms bytes lines facts size_skip err`).

| result | count |
|---|---|
| files run | 1,481 |
| `rc != 0` | 0 |
| `rc = 124` (timeout) | 0 |
| `size_skip` rows | 0 |
| files emitting zero facts | 0 |
| files writing anything to stderr | 0 |
| facts emitted | 5,595,506 (src 5,556,489, test_data 39,017) |

The 540 deliberately-malformed `test_data` files all exit 0 and all emit facts,
so the error-tolerant parse holds on input written to break a parser.

Wall ms across all 1,481 files: min 10, median 29, p95 78, max 591.

### Slowest 10

| ms | bytes | facts | path |
|---:|---:|---:|---|
| 591 | 352,112 | 203,878 | crates/syntax/src/ast/generated/nodes.rs |
| 329 | 265,175 | 125,807 | crates/hir/src/lib.rs |
| 296 | 738,071 | 85,930 | crates/ide-db/src/generated/lints.rs |
| 233 | 87,642 | 38,772 | crates/hir/src/source_analyzer.rs |
| 218 | 98,075 | 39,859 | crates/hir-ty/src/mir/lower.rs |
| 193 | 144,134 | 59,759 | crates/hir-ty/src/mir/eval.rs |
| 171 | 105,969 | 45,786 | crates/rust-analyzer/src/handlers/request.rs |
| 150 | 74,948 | 31,016 | crates/hir-ty/src/mir/eval/shim.rs |
| 144 | 19 | 13 | crates/parser/test_data/lexer/err/unstarted_raw_string_with_ascii.rs |
| 137 | 20 | 13 | crates/parser/test_data/lexer/err/unstarted_raw_byte_string_with_ascii.rs |

The last two are 19-byte files: that is scheduler noise under 8 workers, not
parse cost. The 5th-percentile bytes/ms cohort is the same artifact (every
member is a file under 2.3 KB finishing in 43 to 47 ms, which is the process
floor), so no file was opened for a slow construct.

### Largest 10

| bytes | ms | facts | path |
|---:|---:|---:|---|
| 738,071 | 296 | 85,930 | crates/ide-db/src/generated/lints.rs |
| 352,112 | 591 | 203,878 | crates/syntax/src/ast/generated/nodes.rs |
| 265,175 | 329 | 125,807 | crates/hir/src/lib.rs |
| 222,892 | 82 | 22,852 | crates/ide/src/hover/tests.rs |
| 181,653 | 136 | 48,391 | crates/rust-analyzer/src/config.rs |
| 156,630 | 101 | 66,659 | crates/hir-def/src/expr_store/lower.rs |
| 150,051 | 128 | 46,890 | crates/ide-assists/src/handlers/extract_function.rs |
| 144,134 | 193 | 59,759 | crates/hir-ty/src/mir/eval.rs |
| 136,488 | 101 | 20,965 | crates/ide-completion/src/render.rs |
| 122,575 | 100 | 39,152 | crates/hir-def/src/nameres/collector.rs |

Max RSS over those 20 largest, `/usr/bin/time -l`: peak 66,355,200 B on
nodes.rs, 37,732,352 B on the 738 KB lints.rs, everything else under 26 MB. No
RSS finding.

## 3. Step 2: whole-project resolve

`extract --resolve --family call,type` over all 941 src files hits `rc=124` and
emits zero bytes, so the run was split per crate as the brief allows.
Raw: `rust.resolve_runs.tsv`.

| run | files | bytes | rc | ms |
|---|---:|---:|---:|---:|
| all src files, one call | 941 | 17,360,937 | 124 | 10,069 |
| 42 crates, one call each | 941 | 17,360,937 | 0 for 41 | see below |

The one crate that did not fit is `crates/syntax`: 34 files, 800,151 B,
`rc=124`. Recursive halving isolated the cause to a single file.

| label | files | bytes | rc | ms | rows |
|---|---:|---:|---:|---:|---:|
| crates/syntax | 34 | 800,151 | 124 | 10,007 | 0 |
| crates/syntax.a | 17 | 639,843 | 124 | 10,012 | 0 |
| crates/syntax.a.a | 8 | 401,514 | 124 | 10,008 | 0 |
| crates/syntax.a.a.b | 4 | 384,505 | 124 | 10,012 | 0 |
| crates/syntax.a.a.b.b | 2 | 353,199 | 124 | 10,009 | 0 |
| crates/syntax.a.a.b.b.b | 1 | 352,112 | 124 | 10,009 | 0 |

That single file is `crates/syntax/src/ast/generated/nodes.rs`. Measured on its
own, one file as the whole universe:

| run | wall s | rows |
|---|---:|---:|
| `extract nodes.rs` (plain parse, all families) | 0.13 | 203,878 |
| `--resolve --family type nodes.rs` | 0.20 | 0 |
| `--resolve --family call nodes.rs` | 12.12 | 1,684 |

The call resolve arm costs 93x the parse on this file. The scaling is in
[kink 1](#7-kinks).

### Counts from the union of the per-crate runs

| relation | count |
|---|---:|
| `resolved_edge` | 48,723 |
| `resolved_type_edge` | 1,490 |
| `resolved_edge` kind `name_resolve` | 48,723 |
| `resolved_edge` kind `scip_override` | 0 (no index was in the loop) |
| distinct callers `(path, name)` | 18,726 |
| distinct callees `(path, name)` | 9,130 |
| intra-file edges | 23,490 |
| cross-file edges | 25,233 |
| call sites in the src bucket | 138,223 |
| sites resolving to an edge | 48,723 (35.2%) |
| sites resolving to nothing | 89,500 |
| `unresolved` records emitted | 0 |

## 4. Step 2b: why 89,500 sites resolve to nothing

`extract` emits no `unresolved` record on the rust arm, so the cause was
reconstructed mechanically: for each site with no edge, count the corpus files
defining the callee name and place them relative to the caller's crate. The
rust call arm's stated discipline is that a name whose corpus defs span more
than one blob yields no row (`src/lang/rust.rs:929-938`).
Raw: `rust.unresolved_class.tsv`.

| class | count | pct | meaning |
|---|---:|---:|---|
| `no_corpus_def` | 33,446 | 37.4% | std, a crates.io dependency, a macro, or a builtin. No corpus def exists, so no edge is the right answer. |
| `ambiguous_cross_crate` | 27,267 | 30.5% | 2+ corpus files define the name, none of them in the caller's crate. The ambiguity rule drops these under any universe. |
| `ambiguous_in_crate` | 15,021 | 16.8% | 2+ defs of the name inside the caller's own crate. Same rule. |
| `single_def_cross_crate` | 9,656 | 10.8% | Exactly one corpus def, in another crate. **A whole-corpus resolve would resolve these; the per-crate split is what lost them.** This is the measured price of splitting. |
| `resolve_timeout_file` | 3,320 | 3.7% | Every site in nodes.rs, lost to kink 1. |
| `single_def_same_crate_MISS` | 396 | 0.4% | Exactly one def, in the caller's own crate, and still no edge. Opened below. |
| `ambiguous_corpus_wide` | 394 | 0.4% | One def in the caller's crate plus defs elsewhere. The same-crate one does not win. |

The 396 same-crate misses split cleanly:

| sub-cause | count | example |
|---|---:|---|
| collateral of the `crates/syntax` split (`constructors.rs` and `mapping.rs` landed in different halves) | 373 | `crates/syntax/src/ast/syntax_factory/constructors.rs:59` calls `map_node`, defined at `crates/syntax/src/syntax_editor/mapping.rs:198` |
| fn body inside `const _: () = { .. }` | 8 | `crates/span/src/hygiene.rs:145` `self.as_salsa_id()`; `as_id` is never minted as a def, so the site has no caller |
| call in a `static` or `const` initializer | 15 | `crates/ide-diagnostics/src/lib.rs:646` `LazyLock::new(\|\| build_lints_map(..))`; `crates/span/src/ast_id.rs:43` inside a `const`; `crates/ide-completion/src/completions/attribute.rs:317` inside a `static` array |

### Top 10 files by unresolved ratio, sites >= 30

| ratio | sites | resolved | path | cause |
|---:|---:|---:|---|---|
| 1.000 | 3,320 | 0 | crates/syntax/src/ast/generated/nodes.rs | kink 1, resolve timeout |
| 1.000 | 1,841 | 0 | crates/ide-db/src/generated/lints.rs | generated const tables; every site is a struct-literal or `&str` constructor with no corpus fn def |
| 1.000 | 77 | 0 | crates/hir-def/src/macro_expansion_tests/mbe.rs | body is `check!` macro invocations, kink 2 |
| 0.983 | 58 | 1 | crates/proc-macro-srv/proc-macro-test/build.rs | build script, calls only `std` and `cargo_metadata` |
| 0.964 | 84 | 3 | crates/rust-analyzer/src/lsp.rs | `lsp_types` external crate |
| 0.958 | 48 | 2 | xtask/src/pgo.rs | external crate, `std::process` |
| 0.954 | 87 | 4 | crates/ide-completion/src/completions/mod_.rs | cross-crate into `hir` and `ide_db` |
| 0.954 | 410 | 19 | crates/test-fixture/src/lib.rs | cross-crate into `base-db`, `span`, `hir-expand` |
| 0.953 | 447 | 21 | crates/ide/src/syntax_highlighting/highlight.rs | cross-crate into `hir` and `syntax` |
| 0.949 | 39 | 2 | crates/hir-def/src/nameres/tests/mod_resolution.rs | `check!` macro bodies, kink 2 |

## 5. Step 3: entrypoint crawl

Script: `rust.crawl.py`. BFS over `resolved_edge` from two disjoint root sets,
node identity `(path, name)`. Raw: `rust.crawl.json`.

| root set | roots | reachable defs | of 19,190 named defs |
|---|---:|---:|---:|
| program (2 `fn main` in `crates/rust-analyzer/src/bin/*.rs` + 73 `pub(crate) fn handle_*`) | 75 | 336 | 1.75% |
| test (`#[test]` fns) | 7,901 | 10,671 | 55.6% |
| union | 7,976 | 10,928 | 56.9% |
| unreachable | | 8,262 | 43.1% |

### Depth histograms

| depth | program | test |
|---:|---:|---:|
| 0 | 75 | 7,901 |
| 1 | 114 | 581 |
| 2 | 68 | 434 |
| 3 | 30 | 344 |
| 4 | 21 | 222 |
| 5 | 10 | 149 |
| 6 | 8 | 125 |
| 7 | 2 | 49 |
| 8 | 7 | 26 |
| 9 | 1 | 22 |
| 10-14 | 0 | 550 |
| 15-19 | 0 | 359 |
| 20-24 | 0 | 19 |

The program crawl dies at depth 9 having touched 261 non-root defs. The break is
one hop from `fn main` and it is reproducible:

```
step 0  main.rs::main                       out = {actual_main, main}
step 1  main.rs::actual_main                out = {setup_logging, wait_for_debugger,
                                                   with_extra_thread, from_env_or_exit, verbosity}
step 2  main.rs::with_extra_thread           out = {}     <- chain ends
        (source main.rs:68 is `move || run_server(None)`)
        the edge that call produced is  closure@2180 -> run_server
        nothing in the corpus names closure@2180 as a callee
step 3  run_server -> run_session            exists, and is unreachable
        main_loop.rs::handle_event           45 out-edges, 1 in-edge, unreachable
```

The whole LSP server spine is present in the relation and unreachable from
`main`. See kinks 3 and 4.

### 20 largest unreachable defs

| span B | in | out | def | why |
|---:|---:|---:|---|---|
| 44,906 | 1 | 37 | crates/hir-ty/src/mir/eval/shim.rs::exec_intrinsic | reached only through a closure caller |
| 37,872 | 1 | 9 | crates/ide-completion/src/context/analysis.rs::classify_name_ref | cross-crate caller, edge dropped by the per-crate split |
| 27,701 | 11 | 43 | crates/hir-ty/src/display.rs::hir_fmt | 11 in-edges, all from closure or unreached callers |
| 25,422 | 1 | 32 | crates/hir-ty/src/mir/eval.rs::eval_rvalue | caller itself unreachable |
| 25,341 | 9 | 40 | crates/hir-def/src/expr_store/lower.rs::maybe_collect_expr | caller itself unreachable |
| 20,634 | 0 | 13 | crates/hir-expand/src/builtin/derive_macro.rs::coerce_pointee_expand | in-degree 0; its only mention is `register_builtin! { .. CoercePointee => coerce_pointee_expand .. }` at :77, a macro body, kink 2 |
| 18,486 | 1 | 54 | crates/hir/src/diagnostics.rs::inference_diagnostic | cross-crate |
| 17,673 | 1 | 14 | crates/rust-analyzer/src/cli/analysis_stats.rs::run_inference | dispatched from a `flags::` enum match, no fn edge |
| 16,838 | 1 | 2 | crates/rust-analyzer/src/config.rs::field_props | called from a macro body, kink 2 |
| 16,782 | 2 | 3 | crates/hir/src/semantics.rs::descend_into_macros_impl | cross-crate |
| 16,592 | 2 | 41 | crates/hir/src/lib.rs::diagnostics | cross-crate |
| 16,402 | 0 | 12 | crates/hir/src/source_analyzer.rs::resolve_path | in-degree 0; every caller names it as `SourceAnalyzer::resolve_path` on a typed receiver |
| 15,984 | 1 | 45 | crates/rust-analyzer/src/main_loop.rs::handle_event | the server spine, orphaned by the closure at main.rs:68 |
| 14,664 | 1 | 8 | crates/ide/src/hover/render.rs::definition | cross-crate |
| 14,116 | 1 | 1 | crates/ide-completion/src/context/analysis.rs::expected_type_and_name | caller unreachable |
| 14,078 | 0 | 12 | crates/rust-analyzer/src/cli/analysis_stats.rs::run | in-degree 0; reached only as `cmd.run(verbosity)` at main.rs:74 on a `flags::RustAnalyzerCmd` receiver, and 12 other `run` defs share the name |
| 13,896 | 1 | 82 | crates/ide-diagnostics/src/lib.rs::semantic_diagnostics | highest out-degree in the corpus and unreachable |
| 13,778 | 1 | 11 | crates/rust-analyzer/src/flycheck.rs::run | in-degree 1 from a closure |
| 13,719 | 1 | 8 | crates/hir-def/src/expr_store/lower/asm.rs::lower_inline_asm | caller unreachable |
| 13,579 | 1 | 11 | crates/hir-def/src/expr_store/lower/format_args.rs::collect_format_args_impl | caller unreachable |

Ten were opened. None is dead code. Three classes account for all of them:
edges lost to the per-crate split (cross-crate callers), edges lost to the
closure caller (kink 3), and defs registered or dispatched through a table or a
trait object rather than a direct call (`coerce_pointee_expand` is an entry in a
`static` builtin-derive table; `analysis_stats.rs::run` is reached as
`cmd.run()` on a `flags::RustAnalyzerCmd` enum).

### Highest out-degree

19 rows, not 20: the crawl's top-20 slice is taken before the filter that drops
a caller with no matching def node, and one entry fell out.

| out | depth | def |
|---:|---:|---|
| 82 | unreached | crates/ide-diagnostics/src/lib.rs::semantic_diagnostics |
| 60 | 11 | crates/hir-ty/src/infer/expr.rs::infer_expr_inner |
| 54 | unreached | crates/hir/src/diagnostics.rs::inference_diagnostic |
| 51 | 15 | crates/hir-ty/src/mir/lower.rs::lower_expr_to_place_without_adjust |
| 45 | unreached | crates/rust-analyzer/src/main_loop.rs::handle_event |
| 43 | unreached | crates/hir-ty/src/display.rs::hir_fmt |
| 41 | unreached | crates/hir/src/lib.rs::diagnostics |
| 40 | unreached | crates/hir-def/src/expr_store/lower.rs::maybe_collect_expr |
| 37 | unreached | crates/hir-ty/src/mir/eval/shim.rs::exec_intrinsic |
| 34 | unreached | crates/parser/src/grammar/expressions/atom.rs::atom_expr |
| 32 | unreached | crates/hir-ty/src/mir/eval.rs::eval_rvalue |
| 30 | 13 | crates/hir-ty/src/infer/closure/analysis.rs::analyze_closure |
| 29 | 12 | crates/hir-ty/src/infer/coerce.rs::coerce |
| 28 | unreached | crates/hir/src/display.rs::hir_fmt |
| 27 | 5 | crates/ide-completion/src/render/literal.rs::render |
| 27 | 6 | crates/mbe/src/expander/matcher.rs::match_loop_inner |
| 26 | 14 | crates/hir-ty/src/infer/pat.rs::infer_pat_inner |
| 26 | 16 | crates/hir-ty/src/mir/lower/pattern_matching.rs::pattern_match_inner |
| 25 | 1 | crates/ide-completion/src/completions.rs::complete_name_ref |

10 of these 19 are unreachable from any root.

## 6. Step 4: scip comparison

`extract --family scip --scip-timeout 1500` over a copy of the corpus root
returned `rc=0` and exactly one row:

```
{"record":"scip_skip","lang":"rust","bin":"rust-analyzer","reason":"failed",
 "detail":"scip indexer failed: note: Some details are omitted, run with `RUST_BACKTRACE=full` for a verbose backtrace."}
```

`extract` behaved as documented: named skip, exit 0, no silent empty stream. The
indexer is what failed. Run by hand, `rust-analyzer 1.97.0-nightly (9eb3be26)`
panics indexing this corpus:

```
thread 'main' panicked at src/tools/rust-analyzer/crates/hir-ty/src/next_solver/generics.rs:39:14:
No generics for EnumVariantId("Enum::X")
  ...
  5: <hir_ty::infer::InferenceContext>::infer_closure
 12: ide::inlay_hints::hints
 16: <ide::static_index::StaticIndex>::compute
 17: <rust_analyzer::cli::flags::Scip>::run
```

A whole-corpus scip comparison is therefore not obtainable with the installed
toolchain. Four workspace-member crates that stand alone were isolated in
scratch and indexed successfully, and the comparison ran on those.
Raw: `rust.scip_compare.tsv`, `rust.scip_samples.tsv`.

| crate | files | scip_fn_edge | scip name pairs | resolved_edge | resolve name pairs | both | scip only | resolve only |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| line-index | 2 | 116 | 109 | 54 | 40 | 31 | 78 | 9 |
| text-size | 10 | 171 | 144 | 78 | 53 | 47 | 97 | 6 |
| smol_str | 6 | 412 | 326 | 311 | 195 | 90 | 236 | 105 |
| la-arena | 2 | 240 | 224 | 125 | 105 | 50 | 174 | 55 |

`scip_fn_edge` is not a call-graph relation. Its callee is a fn symbol (ending
`().`) in a minority of rows:

| crate | callee is a fn | callee is a type or field |
|---|---:|---:|
| line-index | 26 | 90 |
| text-size | 68 | 103 |
| smol_str | 132 | 280 |
| la-arena | 50 | 190 |

### Reachability side by side, fn-callee edges only, `#[test]` roots

| crate | roots | scip nodes | scip reachable | resolve nodes | resolve reachable |
|---|---:|---:|---:|---:|---:|
| line-index | 4 | 18 | 17 | 24 | 22 |
| text-size | 12 | 35 | 28 | 36 | 20 |
| smol_str | 39 | 66 | 34 | 106 | 62 |
| la-arena | 0 | 39 | 0 | 70 | 0 |

`la-arena` has no `#[test]` fn, so both planes report 0 and the row carries no
information about either.

### 30 sampled edges present in one plane and absent in the other

The 62 sampled pairs in `rust.scip_samples.tsv` classify into five causes.

| side | cause | count in sample | examples |
|---|---|---:|---|
| scip only | callee is a type or a field, not a callable | 18 | `analyze_source_file -> WideChar`, `add -> TextRange`, `alloc -> Idx`, `as_str -> Heap`, `and_modify -> Entry` |
| scip only | operator or trait dispatch: `+` lowers to `Add::add` | 5 | `add -> add`, `add_assign -> TextSize`, `at -> TextRange` |
| scip only | genuine call `extract` misses (cfg-gated arch dispatch, `std` method on a typed receiver) | 7 | `analyze_source_file -> analyze_source_file_generic`, `analyze_source_file_generic -> start`, `empty_text -> test` |
| resolve only | caller spelled `closure@<n>`; scip attributes the same call to the enclosing fn, so the pair differs on both sides | 6 | `closure@6854 -> new`, `closure@10258 -> from_raw`, `closure@2505 -> as_mut` |
| resolve only | name-only binding of a common method to a corpus def where scip resolves it to `std` or to a different trait impl, producing a false self-edge | 24 | `deserialize -> deserialize`, `serialize -> serialize`, `try_from -> try_from`, `as_ref -> as_ref`, `clone -> clone`, `clear -> clear`, `text_len -> len` |

The last row is the same defect as kink 4, seen from the other plane.

## 7. Kinks

| # | class | count in corpus | example file:line | owner fn | fixture |
|---|---|---:|---|---|---|
| 1 | perf, cubic in one file's def count | 1 file times out, costing 3,320 sites directly and 373 more as split collateral | crates/syntax/src/ast/generated/nodes.rs (2,508 defs, 3,320 sites, 12.12 s) | `RustSource::call_name_match`, `src/lang/rust.rs:901`; `own_file_blob`, `src/lang/rust.rs:951` | `rust_findings/resolve_scale_200.rs` |
| 2 | missing_fact, calls inside a macro body are invisible | 17,184 macro invocations in the src bucket; every call inside one mints no site | crates/hir-def/src/macro_expansion_tests/mbe.rs (77 sites, 0 resolved) | `project_call`, `src/lang/rust.rs:1215` (no `syn::Macro` arm in the expression walk) | `rust_findings/macro_body_calls.rs` |
| 3 | missing_fact, a closure caller ends every walk | 6,973 of 48,723 edges have a `closure@<n>` caller, 3,837 distinct closures, 934 callees reachable only through one | crates/rust-analyzer/src/bin/main.rs:68 (`move \|\| run_server(None)`) | `caller_name`, `src/project.rs:1012`, reached through `covering_def` at `src/lang/rust.rs:1031` | `rust_findings/closure_caller_chain.rs` |
| 4 | wrong_fact, the site's `callee_path` is ignored | 294 edges point at a file other than the module the call site names; the intra-file form mints an edge per site all naming one def | crates/rust-analyzer/src/bin/main.rs:30 (`rustc_wrapper::main()` becomes `main -> main`) | `RustSource::call_name_match`, `src/lang/rust.rs:913-928` (the same-file leg runs before any path check) | `rust_findings/qualified_path/{main,wrapper}.rs`, `rust_findings/unresolved_reason.rs` |
| 5 | missing_fact, a fn inside `const _: () = { .. }` mints no def | 8 sites in the corpus; the def itself is invisible corpus-wide | crates/span/src/hygiene.rs:145 (7 `as_salsa_id` sites) | `call_defs_in_items`, `src/lang/rust.rs:1082` | `rust_findings/const_block_defs.rs` |
| 6 | missing_fact, a call in a `static` or `const` initializer has no caller | 15 sites | crates/ide-diagnostics/src/lib.rs:646, crates/span/src/ast_id.rs:43, crates/ide-completion/src/completions/attribute.rs:317 | the bail at `src/lang/rust.rs:1031` | `rust_findings/static_init_call.rs` |
| 7 | missing_fact, the rust arm never emits `unresolved` | 0 rows over 138,223 sites, of which 89,500 resolve to nothing | the record is in the contract at `src/schema.rs:45` | only `src/lang/ts.rs:1194` pushes one | `rust_findings/unresolved_reason.rs` |
| 8 | wrong_fact, `scip_skip` `detail` carries the wrong stderr line | 1 row, and it is the only diagnostic the caller gets | the panic message `No generics for EnumVariantId("Enum::X")` was replaced by `note: Some details are omitted, run with RUST_BACKTRACE=full ...` | `stderr_tail`, `src/scip_ensure.rs:623`, takes the last nonempty line. A Rust panic ends with a backtrace, so the last line is never the message. | none; reproducing it needs a panicking indexer |

Kink 1's blast radius is the largest and it is indirect: one file over budget
takes its whole crate's resolve with it, and the recursive halving that recovers
the crate then drops 373 intra-crate edges that were never in question.

## 8. Fixtures

`v6/sprefa-extract/tests/fixtures/rust_findings/`. Each header states the
expected fact, the observed fact at `cec3d5c1d`, the owner fn, and the corpus
site. All seven were run and reproduce.

| fixture | expected | observed |
|---|---|---|
| `resolve_scale_200.rs` | resolve cost grows with the corpus | n^2.9 in one file's def count; 100/200/400/800/1600 defs cost 40/54/162/1184/10488 ms |
| `macro_body_calls.rs` | 4 sites, 4 edges | 1 site, 1 edge |
| `closure_caller_chain.rs` | a walkable path `entry -> worker` | `entry -> spawn` and `closure@1182 -> worker`, nothing joins them |
| `qualified_path/main.rs` + `wrapper.rs` | `main -> wrapper.rs::main` | `main -> main.rs::main` |
| `const_block_defs.rs` | 2 def nodes, 1 edge | 0 def nodes, 0 edges |
| `static_init_call.rs` | 2 edges | 0 edges |
| `unresolved_reason.rs` | `unresolved` rows with a reason slug; 2 ambiguous `compute` sites yield no edge | 0 unresolved rows; both `compute` sites mint an edge naming one def |

## 9. What stays untested and why

| area | why |
|---|---|
| whole-corpus scip against `resolved_edge` | the installed `rust-analyzer 1.97.0-nightly (9eb3be26)` panics indexing this corpus (`hir-ty/src/next_solver/generics.rs:39`). Not an `extract` defect and not fixable from this lane. The four isolated crates that did index are the whole scip sample. |
| a whole-corpus `--resolve` baseline | `rc=124` at 10 s, and the 10-second law forbids waiting it out. Every cross-crate number in this report is therefore a floor: `single_def_cross_crate` (9,656 sites, 10.8%) is what a single-call universe would recover, and the true resolution rate is above 35.2%. |
| whether the 294 path-qualified misroutes are all wrong | the test requires the qualifier to name a real corpus module that defines the callee, which is sound but not complete. `crate::`, `super::` and `self::` qualifiers were excluded rather than resolved, so the true misroute count is higher than 294. |
| `--family df` and `--family cfg` over the corpus | outside this brief. Step 1 ran the default (all families) and found no failure, so nothing points at them. |
| `--family diet_scip` | the brief's steps 2 and 4 name `--resolve` and `--family scip`; `diet_scip` was not run. |
| the 540 `test_data` files past step 1 | deliberately malformed parser fixtures. They pass step 1 clean; resolving them would measure the fixtures, not the extractor. |
| `--scip-build` inside the corpus root | the corpus is read-only by brief. The build ran against an rsync copy in scratch, so nothing here says whether the in-tree `.dl/.state/` cache path behaves. |

## 10. Two corrections to the brief

- The brief names `crates/rust-analyzer/src/main.rs`. That file does not exist
  at `af4111f`. The binary entrypoints are
  `crates/rust-analyzer/src/bin/main.rs` and
  `crates/rust-analyzer/src/bin/rustc_wrapper.rs`; both were used as roots.
- The brief's first action, `cargo build --release --features cli`, fails in
  any boop worktree: `Cargo.toml:135` and `:166` reach `hafley-rs` through
  `../../../hafley-rs/crates/...`, which resolves to
  `.boop-worktrees/crawl/hafley-rs` and does not exist. The binary
  boop-start hydrates at `target/release/extract` was used instead. No
  `src/**` change was made, so nothing needed rebuilding.

## 11. Fixes (lane `fix-extract-rust-crawl`)

Kinks 3, 5 and 6 are landed. Kinks 4 and 7 are NOT: neither is reachable from
`src/lang/rust.rs`, the only source file this lane owns, and the seam facts
that block them are in section 11.3. Kink 2 stays out of scope by brief.

### 11.1 The table

| kink | before | after | test |
|---|---|---|---|
| 3, closure caller ends the crawl | 6,977 closure-caller edges, 0 reachable through one | 6,995 mirror edges, one per closure-caller edge | `52_rust_crawl_kinks.rs::a_closure_caller_edge_mirrors_onto_the_enclosing_fn`, `::one_mirror_edge_per_closure_caller_edge` |
| 5, fn inside `const _: () = { .. }` | 0 defs from const/static bodies | 125 `const_init` defs, 20 methods, 7 free fns | `::const_block_fns_mint_call_defs`, `::const_block_call_resolves_to_its_sibling` |
| 6, call in a `static`/`const` initializer | 0 edges | 2,006 edges whose caller is the const/static item | `::initializer_calls_carry_the_const_or_static_item_as_caller`, `::a_const_with_no_call_in_its_initializer_mints_no_call_def` |
| 4, `callee_path` ignored | 294 wrong edges | unchanged, see 11.3 | none |
| 7, no `unresolved` row | 0 rows | unchanged, see 11.3 | none |

### 11.2 The corpus receipt

`plans/extract-crawl-2026-08-29/rust.crawl.py` over
`~/projects/rust-analyzer` at `af4111f`, 941 src files, one `--resolve
--family call,type` call per crate under `timeout 10`. Both columns were
measured with THIS method and THIS lane's two release binaries, so the before
column is not section 5's 10,928: the kink-1 hoist (PR #540) landed between,
so `crates/syntax` no longer times out and no crate needed halving. 0 timeouts
in either run; slowest crate 254 ms.

| | before (`c60e5c4cc`) | after (`2e2200e83`) |
|---|---:|---:|
| named defs | 19,190 | 19,339 |
| `resolved_edge` | 50,490 | 59,506 |
| `resolved_type_edge` | 1,720 | 1,720 |
| reachable, union of program + test roots | 10,951 (57.1%) | 12,221 (63.2%) |
| reachable from the 75 program roots | 336 | 477 |
| reachable from the two `fn main` alone | 66 | 269 |
| unreachable | 8,239 | 7,118 |
| program crawl max depth | 9 | 10 |

The 9,038 new edge rows: 6,995 mirrors (kink 3), 2,006 with a const/static
item as caller (kink 6), 37 reaching a def that only exists now (kink 5).

Section 5's headline break is closed. `crates/rust-analyzer/src/bin/main.rs:68`
is `move || run_server(None)`, and the whole LSP server spine now walks from
`fn main`:

| def | before | after |
|---|---|---|
| `bin/main.rs::run_server` | unreachable | reachable |
| `session.rs::run_session` | unreachable | reachable |
| `main_loop.rs::main_loop` | unreachable | reachable |
| `main_loop.rs::handle_event` | unreachable | reachable |

### 11.3 Why kinks 4 and 7 did not land

| kink | what the brief asked | the seam fact that blocks it |
|---|---|---|
| 4 | restrict candidates to defs whose module path (file path segments + `mod` scope owner) ends with `callee_path` | `Resolve::resolve(&self, output, cx)` never receives a path: `resolve_call_edges` holds one and drops it (`src/project.rs:817-832`). `ProjectCx.files` is the unit struct `FileSet` (`src/types.rs:1430`). `DefSite` is `{blob, span, family}` (`src/types.rs:1467`). No blob-to-path map exists anywhere in the resolve seam, so "file path segments" is not computable from `src/lang/rust.rs`. |
| 7 | emit one `unresolved` row per site that resolves to nothing, reason `no_corpus_def` / `ambiguous` | `unresolved` is a PHASE-1 aux record (`CallFAux.unresolved`, serialized at `src/wire.rs:311`) and `UnresolvedReason` is a closed 3-value enum whose header states a fourth reason needs its own issue (`src/types.rs:567-574`). Both requested reasons are corpus-wide facts a per-file phase-1 walk cannot know, and `Resolve<CallF>` returns `Vec<ProjectEdge<CallF>>` with no channel for a non-edge. |

Both are one grant away: kink 4 needs a path on `DefSite` or `ProjectCx`
(`src/types.rs` + `src/project.rs`); kink 7 needs two enum variants
(`src/types.rs`), a resolve-phase channel (`src/project.rs`) and the vocabulary
line at `src/schema.rs:162`. All four files are outside this lane's ownership.

### 11.4 What the fixes changed in the shape

- A new rust ext `CallKind`, tag `const_init`, for a `const`/`static` item that
  owns calls in its initializer. It is a caller and never a callee, and no other
  language has the shape, so it goes through the `Ext(LangKind)` door rather
  than the core enum (`tests/6_kind_vocab.rs`). Its `EXT_KINDS` row belongs in
  that test's list; that file is outside this lane's ownership.
- The item def is minted ONLY when the initializer holds a call no inner def
  covers, so `const GREETING: &str = "hello"` stays out of the corpus name index
  and no rust source fixture moved the wire golden.
- `tests/fixtures/resolve/9_closure_resolved_edges.jsonl` gained ONE appended
  row (`run -> helper`, the mirror of the `closure@63 -> helper` row it already
  pinned), granted by the coordinator. Its test now pins 2 rows, not 1.
- That `.jsonl` is ITSELF a data fixture in `kind_vocab/corpus.txt`, so the
  appended row cascades into the wire golden. `wire_golden.jsonl` was
  regenerated by the procedure `tests/6_kind_vocab.rs` documents (`extract
  <path>` over `corpus.txt`, concatenated): ONE hunk, 10 lines added, 0
  removed, all of them the `data_doc` + 9 `data_value` rows of the appended
  edge. Debug and release binaries produce byte-identical output.
- Gate, `cargo test --features cli --no-fail-fast`, SUM over 82 binaries:
  404 passed, 0 failed, 2 ignored (baseline before this lane: 399/0/2).
