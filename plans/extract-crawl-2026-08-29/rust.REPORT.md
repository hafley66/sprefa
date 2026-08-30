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
11. [Fixes (lane `fix-extract-rust-crawl`)](#11-fixes-lane-fix-extract-rust-crawl)
12. [Fixes (lane `fix-extract-rust-crawl-2`)](#12-fixes-lane-fix-extract-rust-crawl-2)
13. [Fixes: the module plane (lane `fix-extract-rust-module-plane`)](#13-fixes-the-module-plane-lane-fix-extract-rust-module-plane)
14. [macro_rules expansion, kink 2 closed (lane `feature-extract-rust-mbe`)](#14-macro_rules-expansion-kink-2-closed-lane-feature-extract-rust-mbe)
15. [Scip macro-span call sites](#15-scip-macro-span-call-sites-lane-feature-extract-rust-scip-macros)
16. [The 52,101 `ambiguous` drops classified](#16-the-52101-ambiguous-drops-classified-lane-fix-extract-rust-cross-crate)
17. [The receiver leg](#17-the-receiver-leg-lane-fix-extract-rust-cross-crate)
18. [The receiver leg, pass 2](#18-the-receiver-leg-pass-2-lane-fix-extract-rust-receivers-2)
24. [The checker tier](#24-the-checker-tier-lane-fix-extract-rust-checker)

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

## 12. Fixes (lane `fix-extract-rust-crawl-2`)

Kinks 4 and 7 are landed. Section 11.3's two seam blockers are gone: PR #546
put a blob-to-path index on `IndexBag` (`src/types.rs`, `PathIndex`, set in
`resolve_project`), and this lane added the resolve-phase non-edge channel it
named as missing.

### 12.1 The table

| kink | before | after | test |
|---|---|---|---|
| 4, `callee_path` ignored | 310 of 846 judged edges point at a file other than the module the site names | 0 of 981 | `52_rust_crawl_kinks.rs::a_module_qualified_call_binds_in_the_module_the_path_names`, `::a_type_qualifier_keeps_the_name_leg_and_an_unknown_crate_mints_nothing` |
| 7, no `unresolved` row | 0 rows over 138,223 sites | 86,081 rows, one per dropped site | `::a_dropped_site_mints_an_unresolved_row_naming_why`, `::one_unresolved_row_per_dropped_site` |

### 12.2 The rule kink 4 now applies

A site's `callee_path` is read ONLY when every qualifier segment is module-shaped
(lowercase first letter). `Widget::build` and `Vec::new` are type qualifiers and
keep the bare-name leg, because receiver typing is out of this arm's scope.

| written | resolved against |
|---|---|
| `a::b::f` | any corpus file whose module path ends with `a::b` |
| `crate::b::f` | the caller's crate directory, then the `b` suffix |
| `self::b::f` | the caller's own module path, then `b` |
| `super::f` | the caller's module path with one segment popped |
| `other_crate::f` | nothing: no corpus file spells that module |

A file's module path is its supplied path minus `.rs`, with `src` segments
dropped, `mod.rs`/`lib.rs`/`main.rs` collapsing to their directory, and `-`
read as `_`, so `crates/ide-db/src/famous_defs.rs` spells `ide_db::famous_defs`.
Two candidate blobs after the filter is still an ambiguity this tier drops.

### 12.3 The channel kink 7 uses

`Resolve<CallF>` returns `Vec<ProjectEdge<CallF>>` and has no seat for a
non-edge, so the drop channel is a second fn pointer on the roster row:
`ResolveArm.drops` (`src/project.rs`), fed by `RustSource::call_drops`. Every
other arm sets `drops: None` and its output is byte-identical to before.
`UnresolvedReason` gained `no_corpus_def` and `ambiguous`, both corpus-wide
facts a per-file phase-1 walk cannot decide, carrying the issue row
`issues/extract-unresolved-resolve-phase-reasons`. The `unresolved` record
gained an optional `path`, absent from a per-file run and present under
`--resolve`, because a resolve spans files and a bare span does not say which.

| reason | corpus rows | decided by |
|---|---:|---|
| `no_corpus_def` | 69,192 | no corpus def bears the callee's name |
| `ambiguous` | 16,889 | the corpus defines the name; this tier does not settle which |

### 12.4 The corpus receipt

`plans/extract-crawl-2026-08-29/rust.crawl.py` and `rust.qualified.py` over
`~/projects/rust-analyzer` at `af4111f`, 941 src files, one `--resolve --family
call,type` call per crate under `timeout 10`. BOTH columns were measured with
this lane's script and its two release binaries, so the before column is a
fresh measurement of `b9b98e3af`, not a copy of section 11.2. 0 timeouts in
either run, 46 crates each, slowest crate 254 ms before and 262 ms after.

| | before (`b9b98e3af`) | after |
|---|---:|---:|
| named defs | 19,339 | 19,339 |
| `resolved_edge` | 59,506 | 59,097 |
| judged qualified edges | 846 | 981 |
| edges whose callee file disagrees with the site's `callee_path` | 310 | **0** |
| `unresolved` rows | 0 | 86,081 |
| reachable, union of program + test roots | 12,221 (63.2%) | 12,142 (62.8%) |
| reachable from the 75 program roots | 477 | 471 |
| unreachable | 7,118 | 7,197 |
| program crawl max depth | 10 | 10 |

The COUNT rail holds at corpus scale: 138,223 sites, 52,142 of them bound to at
least one edge, 86,081 `unresolved` rows, and 138,223 - 52,142 = 86,081.

409 net edges left (911 lost, 502 gained). The losses are the price of the rule:
a qualifier naming an inline `mod` no file spells, or a `use` re-export, no
longer falls back to a bare-name guess. The gains are sites that were previously
ambiguous corpus-wide and the path now disambiguates. Reachability paid 79 defs
for 310 corrected edge targets.

Raw: `rust.qualified.before.json`, `rust.qualified.after.json`,
`rust.resolve_runs.after.tsv`.

### 12.5 What the fixes changed in the shape

- `tests/6_kind_vocab.rs` gained the `rust::CONST_INIT` EXT_KINDS row section
  11.4 asked for. The wire golden did not move for it: no `rust_findings`
  fixture is in `kind_vocab/corpus.txt`.
- `tests/fixtures/resolve/9_closure_resolved_edges.jsonl` gained 3 rows, the
  `unresolved` rows for `iter`, `map` and `collect`, which have no corpus def
  in that two-file universe. That `.jsonl` is itself a data fixture in
  `kind_vocab/corpus.txt`, so `wire_golden.jsonl` was regenerated by the
  procedure `tests/6_kind_vocab.rs` documents: TWO hunks, 30 lines added, 0
  removed, all of them the `data_doc` + 9 `data_value` rows of the three
  appended lines.
- `tests/8_scip_families_cli.rs::the_discrimination_holds_through_rust_analyzer_too`
  asserted the heuristic output does not contain the string `helper` at all.
  The drop row now names it in `detail`, so the assertion was narrowed to what
  it means: no edge row binds `helper`, and one row SAYS the name is ambiguous.
- `tests/20_unresolved.rs` pins that a phase-1 row's `path` is absent.
- Gate, `cargo test --features cli --no-fail-fast`, SUM over 83 binaries:
  424 passed, 0 failed, 2 ignored.

## 13. Fixes: the module plane (lane `fix-extract-rust-module-plane`)

Module resolution is the LANGUAGE'S OWN algorithm, run once per file set as
its own plane, and every rust resolve arm binds an imported name through it
before falling to the corpus-wide name-match (user decision 2026-08-29, the
same ruling that landed the ts twin, `fix-extract-ts-module-plane`, PR #549).

### 13.1 What was built

| piece | where |
|---|---|
| `RustModuleFacts`: a dedicated second `syn` parse per file (`use` bindings with qualifier/asked/reexport/glob kept apart, inline `mod x {}` names, `mod x;`/`#[path]` declarations) | `src/lang/rust_modules.rs` |
| `RustModuleIndex`: qualifier -> home file via the kink-4 `module_target`/`module_segments`/`crate_root_of` text math (reused, not duplicated), `export_table` per file (local defs, `pub use` hops, glob merge with ambiguity), `local_scope_table` for a non-reexport glob's own-file scope | same |
| `IndexBag.rust_modules`, built after the def index | `src/types.rs`, `src/project.rs` |
| `Resolve<CallF>` / `Resolve<TypeF>` try the plane after a same-file def, before the corpus-wide ambiguous match | `src/lang/rust.rs` |
| `resolved_import` rows, chained with the ts plane's in one fact loop | `src/project.rs` `import_facts` |

Two bugs the fail-first tests in `tests/57_rust_module_plane.rs` caught before
landing (both fixed, both covered): `home_file` bucketed candidate files by
the raw qualifier text (`super`/`self`/`crate` are never a file's own module
segment), so any `super::`/`self::` use could never resolve; and a per-NAME
walk over a module's star imports reproduced kink 1's shape (a generated
200/400-leaf-plus-barrel corpus measured wall(400)/wall(200) = 2.557x against
the 2.5x budget). Both are the SAME class of defect the ts plane already
named in its own header comment (`ts_resolve.rs`): resolve by TABLE, not by
per-name re-walk.

### 13.2 The receipt

Same corpus, same 941 files (`plans/extract-crawl-2026-08-29/rust.crawl.py`'s
own partition of the 46 crate labels in `rust.resolve_runs.after.tsv`),
same machine. `before` = `da239de8f` (this lane's base sha, kink 4/7 already
landed), `after` = `53189700b`. Kink 1's timeout (the reason sections 3/11/12
split the resolve into 46 per-crate calls) is ALREADY FIXED on both binaries
by a prior PR, so this receipt uses ONE whole-corpus `--resolve --family
call,type` call instead: a cleaner before/after with no per-crate-split
distortion, and a correction to two of the brief's baseline citations (13.3).

| | before (`da239de8f`) | after (`53189700b`) |
|---|---:|---:|
| whole-corpus `--resolve` wall | 2.65s | 4.66s |
| `resolved_edge` | 55,298 | 59,921 |
| `resolved_edge` kind `import_resolve` | 0 | 11,907 |
| `resolved_edge` kind `name_resolve` | 55,298 | 48,014 |
| `resolved_type_edge` | 2,366 | 2,575 |
| `resolved_import` rows | 0 (no record for rust) | 7,870 |
| `unresolved` | 89,827 | 85,435 |
| `unresolved` reason `ambiguous` | 59,465 | 55,073 |
| `unresolved` reason `no_corpus_def` | 30,362 | 30,362 |
| named defs | 19,333 | 19,333 |
| reachable from the 75 program roots | 3,117 | 3,196 |
| reachable, union of program + test roots | 12,827 (66.4%) | 12,924 (66.9%) |
| unreachable | 6,506 | 6,409 |

`resolved_import` by kind: local 5,398, indirect 1,437, namespace 563, star
472. `no_corpus_def` is BYTE IDENTICAL before/after: the plane only removes
AMBIGUITY (2+ corpus defs, now one of them named by a `use`), it can never
manufacture a def std/an external crate never had. Of the 11,907
`import_resolve` edges, 7,284 are call sites that ALREADY resolved via
name-match to the same target (now correctly attributed to the language's own
rule instead of incidental corpus-wide uniqueness) and 4,623 are net-new edges
a dropped or ambiguous site gained; 55,298 + 4,623 = 59,921.

### 13.3 Two corrections to the brief's baseline numbers

- The brief cites reachable-from-program-roots 477 and union 12,221 (63.2%)
  as the number to move. Both are section 11.2's PRE-kink-4/7 figures; section
  12.4 already moved them to 471 and 12,142 (62.8%) on the SAME per-crate-split
  binary this lane started from. Neither is directly comparable to 13.2's
  numbers, which are a whole-corpus run (see above) — a wider universe than
  any per-crate split, so its raw counts sit higher across the board.
- `ambiguous_cross_crate` (27,267) and `single_def_cross_crate` (9,656) are
  section 4's names for shapes measured against a PER-CRATE-SPLIT universe:
  "no candidate in the caller's crate" only means something when the universe
  really is one crate at a time. Re-run at whole-corpus scope with a
  crate-partition classifier (`/tmp/rmp_crawl_scratch/classify.py`, not
  committed — a reconstruction script, not part of the shipped binary):
  `ambiguous_in_crate` 28,473 -> 26,690, `ambiguous_cross_crate` 27,569 ->
  27,465. `single_def_cross_crate` has no whole-corpus analogue: a single
  cross-crate def is not ambiguous once the universe already contains it, so
  it resolves as `name_resolve` on BOTH binaries, uncounted by either.

### 13.4 Cost

Whole-corpus wall grew 2.65s -> 4.66s (+76%), the same shape as the ts
plane's own cost line (2s -> 3.10s, +55%): one extra `syn::parse_file` per
rust input the dedicated second parse costs (phase 1's own parse doesn't
survive past dispatch), plus the `export_table`/`local_scope_table` build.
Under the 10-second law on every crate size measured, including the
whole-corpus call. The COUNT test guarding against a per-name re-walk lives
in `tests/57_rust_module_plane.rs::barrel_resolve_wall_grows_linearly_with_file_count`.

### 13.5 Gate

`cargo test --release --features cli --no-fail-fast`: 460 passed, 0 failed,
2 ignored, over 86 binaries.

### 13.6 What stays untested and why

| gap | why |
|---|---|
| a qualified call THROUGH an imported module alias (`use a::b as ns; ns::f()`) | `Resolve<CallF>`'s qualified branch (kink 4) reads `callee_path` as an absolute/`crate`/`self`/`super` module path; it does not check whether the FIRST segment is itself a `use`-bound alias. Bare-name imports (the brief's stated scope) are covered; this is a real gap, not fixed here. |
| `pub(crate)`/`pub(in path)`/private visibility enforcement | explicitly out of scope per the brief, same as kink 4's ruling |
| a workspace `[patch]`/virtual manifest / `Cargo.toml` dependency graph reader | the plane never reads a manifest; a `use other_crate::f` resolves by suffix match against whatever corpus files are SUPPLIED, same discipline as `crate::`'s `crate_root_of`, so a same-named module in an unrelated crate in the SAME corpus can false-positive (pre-existing kink-4 limitation, not new) |
| macro-generated `use` items | `rust_module_facts`'s parse sees written source only, same boundary as kink 2 |
| incremental re-resolve after an edit | the plane, like `TsModuleIndex`, is built once per run |

## 14. macro_rules expansion, kink 2 closed (lane `feature-extract-rust-mbe`)

`plans/extract-macro-lab-2026-08-29/PLAN.md` Option 1, wired into
`RustSource::extract`'s call arm (`src/lang/rust.rs::splice_macro_expansions`,
`src/lang/rust_mbe.rs`). Kink 2 was: a call written only inside a local
`macro_rules!` invocation has no site, because `syn::visit::visit_file` never
enters a `Macro` expression/item node. The hook splices each invocation's
expansion into the file's own text and re-runs the call walker on it, mapping
gained spans back to the invocation that minted them.

### Corpus cleanup

The `~/projects/rust-analyzer` clone (still commit `af4111f`) carried 943
`*.rs.expanded.rs` files left over by the ORIGINAL lab's corpus battery
(`labs/macro_expand/src/main.rs`'s `corpus` mode writes one such file per
invocation next to its source). That inflated `find crates -name '*.rs' |
grep '/src/'` from 873 to 1,816 files. Removed with `git clean -fdx --
'*.rs.expanded.rs'` (dry-run checked first: exactly 943 matches, nothing
else); corpus back to the PLAN.md bucket, 873 files.

### Bucket note

PLAN.md's own bucket (`crates/*/src/**`, 873 files) is NOT the 941-file
bucket sections 2-5 above used; comparing counts across the two would be the
same bucket-mismatch PLAN.md itself flagged for the 17,184/941 figures. Every
number below re-measures BOTH sides (with the mbe hook, without it) on the
SAME 873-file bucket, same binary otherwise (worktree at commit `8cf73229e`,
the two commits before the hook landed).

### Call sites, whole 873-file bucket

| | sites | rc != 0 |
|---|---:|---:|
| without the hook | 133,102 | 0 |
| with the hook | 141,520 | 0 |
| gain | +8,418 (+6.3%) | |

`plans/extract-macro-lab-2026-08-29/mbe.battery.py` regenerates this: one
`extract --family call FILE` per file, `timeout 10`, 8 workers.

### Spot check against the lab's named kink-2 files

PLAN.md cites four files as the whole gain; re-measured here per file
(`extract --family call FILE`, site-row count):

| file | lab's stated gain | measured gain (this PR) |
|---|---:|---:|
| `crates/intern/src/symbol/symbols.rs` | +5,391 | +5,383 |
| `crates/hir-def/src/lang_item.rs` | +992 | +992 |
| `crates/hir-expand/src/inert_attr_macro.rs` | +416 | +416 |
| `crates/rust-analyzer/src/config.rs` | +226 | +226 |

Three of four match exactly; `symbols.rs` is within 8 sites (the lab's
own single-pass `corpus` CLI mode expanded each file once, this hook runs the
full 8-pass fixpoint, so a file with macro-inside-macro nesting can gain a
few more sites here than the lab's one-shot number). Summing just these four
files (7,017-7,025) already exceeds PLAN.md's own corpus-wide total of
+4,843 sites gained across "33 gain files" — an inconsistency in PLAN.md's
own summary row, not introduced by this hook; the per-file numbers this PR
re-measured are the ones that reproduce.

### Entrypoint crawl, same 873-file bucket both sides

`resolve_edges.jsonl`/`defs.jsonl` regenerated per crate
(`extract --resolve --family call`, 35 crates, `timeout 10` each, 0
`rc != 0` either side), fed to `rust.crawl.py`:

| | defs_total | edges_total | union_reachable |
|---|---:|---:|---:|
| without the hook | 18,685 | 57,377 | 11,787 |
| with the hook | 19,249 | 60,948 | 11,792 |
| gain | +564 | +3,571 | +5 |

Closing kink 2 raises the def/edge counts substantially but moves the
entrypoint-reachable union by only 5 defs: most call sites gained inside a
macro expansion name a callee ALREADY reachable some other way (the macro
body calls a helper the surrounding module already calls directly elsewhere).
`12,221`/`12,142` from sections 11-12 are the 941-file bucket and are not
comparable to either row above.

### macro_site receipt

Two forms of the same fact, per the coordinator's follow-up (m-6b7269ab,
superseding the earlier TSV-only addendum): `CallFAux.macro_sites` now
carries it on the wire too (`record=macro_site`, `types.rs`'s dedicated
commit "types: CallFAux.macro_sites"; `source: "mbe"` distinguishes it from
the scip-macros lane's rows on the same shape). The TSV
(`plans/extract-macro-lab-2026-08-29/mbe.macro_sites.tsv`, one row per
distinct invocation actually spliced: `path`, `start`, `end`, `macro_name`,
original-file byte offsets, 1,057 rows across 51 of the 873 files) stays
as the corpus-scale receipt the coordinator diffs against
`scip.macro_sites.tsv` by span.

### Budget cap

2 of 871 files the `#[ignore]`d `corpus_wall_time_and_macro_sites_tsv` test
walked hit `budget_hit`:
`crates/ide-completion/src/completions/attribute.rs` and
`crates/intern/src/symbol/symbols.rs`. The second is the file with the
5,383-site gain above: its expansion is a PARTIAL fixpoint (8 passes still
found a pending invocation), so that gain is a lower bound, not the settled
count. `f9_recursive.rs` pins the cap's own behavior in `tests/58_rust_mbe.rs`.

### Tests

`tests/58_rust_mbe.rs`: 10 tests (module-level `expand_file` behavior, the
end-to-end `RustSource::extract` hook including the new `macro_site` wire
row, the pass-budget trip, the corpus wall-time/TSV walk), all green.
Whole-crate `cargo test --features cli --no-fail-fast`: see PR body for
the SUM.

## 15. Scip macro-span call sites (lane `feature-extract-rust-scip-macros`)

The kink-2 post-pass: a call written inside a macro invocation has no parse
site, so `Resolve<CallF>` minted nothing for it. A post-pass in
`resolve_project` (`src/lang/rust_scip_macros.rs`) joins the loaded scip
index's reference occurrences to the invocation spans of a syn re-parse and
mints one `resolved_edge` per unmatched call-shaped occurrence, kind
`scip_macro`, plus one shared `macro_site` row (`source: "scip"`) per minted
edge, rows riding their file's block of the stream. Without a scip index the
pass emits nothing.

### 15.1 Receipt

Binary: this lane's `extract` at `7ecb3122d`, rebased on main past the mbe
merge (#553): the mbe arm now expands local `macro_rules` in phase 1, so this
pass's surviving universe is builtins and proc macros, and the `macro_site`
rows ride the shared `FlatFact::MacroSiteOut` record (`source: "scip"`). Corpus `~/projects/rust-analyzer`
at `af4111f`, index `rust-analyzer 1.100.0-nightly` at
`~/projects/rust-analyzer/.dl/.state/index.scip` (72,518,685 B, the lab's).
Every `extract` call under `timeout 10`.

- defs: 941-file src bucket (540 `test_data` + 943 lab `.expanded.rs` droppings
  excluded), one `--family call` call per file, 0 rc != 0 (the mbe merge grows
  the named-def set to 19,924 with expansion-minted defs). Raw:
  `plans/extract-macro-lab-2026-08-29/scip.defs_runs.tsv`.
- resolve unions: one `--resolve --family call,type` per crate. TEN crates
  exceeded the 10s budget with the index loaded (finding, below), so those
  crates were halved recursively to fit and the SAME 96-leaf partition was
  then reused for a matched no-index baseline. Raw:
  `plans/extract-macro-lab-2026-08-29/scip.resolve_runs.{base,scip}.tsv`,
  partition `scip.resolve_leaves.tsv`.
- crawl: `rust.crawl.py` unchanged, once per union. Raw:
  `plans/extract-macro-lab-2026-08-29/scip.crawl.{base,scip}.json`.

| | base (matched partition, no index) | scip (index) |
|---|---:|---:|
| `resolved_edge` | 52,674 | 53,958 |
| - `name_resolve` | 44,245 | 48,720 |
| - `import_resolve` | 8,429 | 830 |
| - `scip_override` | 0 | 3,239 |
| - `scip_macro` | 0 | 1,169 |
| reachable, program roots (75) | 200 | 335 |
| reachable, test roots (7,905) | 11,071 | 11,604 |
| reachable, union | **11,245** | **11,877** |
| unreachable | 8,679 | 8,047 |

The `import_resolve` drop under the index (8,429 -> 830) is a KIND
reclassification, not a loss: with scip's word in hand the same bindings land
as `scip_override` (+3,239) and the `name_resolve` count rises with them. The
matched-partition baseline is 11,245, and against it the post-pass gains 632
reachable defs (635 gained, 2 lost), the two being halving-split collateral.

### 15.2 Per-file top 10 gained (defs newly reachable)

| gained | path |
|---:|---|
| 31 | crates/hir-def/src/item_tree/lower.rs |
| 26 | crates/ide-completion/src/context/analysis.rs |
| 21 | crates/rust-analyzer/src/cli/analysis_stats.rs |
| 20 | crates/rust-analyzer/src/config.rs |
| 19 | crates/syntax/src/validation.rs |
| 19 | crates/project-model/src/workspace.rs |
| 18 | crates/syntax/src/ast/node_ext.rs |
| 15 | crates/project-model/src/sysroot.rs |
| 15 | crates/ide/src/lib.rs |
| 14 | crates/ide/src/inlay_hints.rs |

### 15.3 `macro_site` rows

`plans/extract-macro-lab-2026-08-29/scip.macro_sites.tsv`
(path, start, end, macro_name): 1,169 rows over 199 files, one per minted
edge, so the coordinator can diff the mbe lane's rows by span. The stream's
`macro_site` rows carry no path seat, so each row's path is recovered by a
deterministic join over the corpus text (the span must spell the macro's
invocation, `path!` / `path::name!` / `macro_rules!`, with balanced
delimiters) cross-checked against the scip_macro edges whose call site falls
inside the span; all 1,169 resolve uniquely.

| macro | rows |
|---|---:|
| assert_eq | 238 |
| assert | 190 |
| match_ast | 187 |
| matches | 83 |
| format | 65 |
| try_default | 56 |
| from_bytes | 54 |
| debug_assert | 49 |
| vec | 48 |
| write | 47 |

### 15.4 Findings

| lang | class | path | repro | observed | expected |
|---|---|---|---|---|---|
| rust | perf | `src/lang/rust.rs:1170` (Resolve<CallF> scip leg) | `timeout 10 extract --resolve --scip-index <index> -p <root> crates/hir/src/**` | per-crate resolve with the index exceeds 10s on the ten largest crates (`hir`, `hir-def`, `hir-expand`, `hir-ty`, `ide`, `ide-assists`, `ide-completion`, `ide-db`, `rust-analyzer`, `syntax`); `definition_of` scans every document's occurrences per site | a per-crate scip resolve inside the 10s budget |
| rust | timeout | `crates/hir/src/lib.rs`, `crates/syntax/src/ast/generated/nodes.rs` | halved to a single file and still rc=124 | 2 of 96 leaves lost their resolve rows | 0 |

### 15.5 Gate

`cargo test --features cli`, SUM over the test binaries at `46c5dab0b`:
0 failed (baseline before this lane: 0 failed; the lane adds 3 tests in
`tests/59_rust_scip_macros.rs`, fail-first proven with the pass disabled).

## 16. The 52,101 `ambiguous` drops classified (lane `fix-extract-rust-cross-crate`)

The brief's first action: `extract --resolve --project-root <corpus> $(find
crates -name '*.rs' -path '*/src/*')`, one process, the 941-file src bucket
(873 source + generated/fixture files, test_data excluded). Raw:
`/tmp/rxc_after.tsv` (scratch, not committed): 153,826 rows, 70,113
`unresolved`, of which `ambiguous` **52,101** and `no_corpus_def` 17,672
(the brief's 15,184 / 65,992 come from the bench pipeline's universe, see
17.1; the CLASS mix is what this section measures and the mix is the
decision input).

300 rows sampled (seed 7). Each site's receiver expression was peeled
(method chains, `)`/`]`-balanced call tails) against the source text, then
classified by what the compiler would bind:

| class | count | projected (x/300 of 52,101) | meaning |
|---|---:|---:|---|
| (a) method call, receiver type traceable, impl in corpus | 253 | **43,940** | `x.m()` where `x`'s type is a param annotation, `let x: T`, `self`/field, or a method-chain tail, and an `impl` block (inherent or trait) defines `m` for some corpus type |
| (b) associated fn / ctor | 27 | 4,698 | `T::new()`, `Self::f()`, `ast::MethodCallExpr::cast()`, `Vec::new()` (about a third of these name external types, `Vec`/`TextSize`/`NonZero`, where no edge is correct) |
| (d) free fn, module-scoped | 4 | 695 | the completion test harness `check`/`check_edit` (each defined per test module; 2 defs of `check` in `ide-completion/src/tests/*.rs`) |
| (e) other | 16 | 2,768 | 3 struct literals (`tt::Span { .. }`, `crates/mbe/src/lib.rs`), std fns (`mem::replace`, `std::iter::from_fn`), external qualified calls (`support::token`, `crates/syntax/src/ast/generated/nodes.rs:15`), and 2 cross-crate qualified calls through a use alias (`hir::attach_db`, `crates/ide-diagnostics/src/handlers/inactive_code.rs:230`) |
| (c) trait method through a generic bound | 0 | 0 | none in the sample |

Two file:line per top class:

| class | example sites |
|---|---|
| (a) | `crates/ide-assists/src/handlers/promote_local_to_const.rs:3231` `name.syntax()`, trait method impl'd at `crates/syntax/src/ast/expr_ext.rs:66`; `crates/hir/src/lib.rs:703` `name.clone()`, inherent impl in `crates/intern/src/symbol/symbols.rs`'s sibling `crates/intern/src/symbol.rs` |
| (b) | `crates/project-model/src/sysroot.rs` caller, `Sysroot::empty` defined at `crates/project-model/src/sysroot.rs:56`; `SyntaxMappingBuilder::new` defined at `crates/syntax/src/syntax_editor/mapping.rs:194` |
| (d) | `crates/ide-completion/src/tests/record.rs:48` `check(`, `crates/ide-completion/src/tests/expression.rs:1163` `check_edit(` |
| (e) | `crates/syntax/src/ast/generated/nodes.rs:15` `support::token(`, `crates/hir-expand/src/fixup.rs:1356` `SyntaxFixupUndoInfo {` |

Reading: classes (a)+(b) are 93.3% of the sample. The receiver fix the brief
scopes (per-fn receiver table + impl map, plus `T::f()`/`Self::f()` binding)
addresses both; (d) is the module plane's glob leg, already built in #552
(`wildcard_scope`, single source binds, two globs ambiguous); (e) stays.

## 17. The receiver leg (lane `fix-extract-rust-cross-crate`)

The go #554/#562 twin: phase 1 records one `CallFAux.receivers` row per
method-call site (`Named(T)` when the receiver's type is visible in scope,
`Inferred` when it is not), and a corpus-wide (T, m) impl table binds the
site. Table owner: `src/lang/rust_receivers.rs` (new); the table rides the
module plane's second parse (`rust_modules.rs`, `impl_facts`), the resolve
legs live in `src/lang/rust.rs` `Resolve<CallF>`.

| leg | rule |
|---|---|
| receiver | `x.m()` with `Named(T)`: exactly one corpus impl of `(T, m)` -> edge, kind `name_resolve`; none -> NO row (a known type with no corpus impl is std/external/trait dispatch) |
| receiver | `x.m()` with `Inferred`: falls through to the v5-shaped legs; a site that still drops carries reason `inferred` (new in `call_drops`) |
| assoc | `T::f()` / `a::T::f()`: the last uppercase-leading segment names the impl type, same (T, m) table |
| assoc | `Self::f()`: the enclosing impl's self type, read off the file's `method_owners` rows |
| scope | `Named` sources: param annotations, `let x: T`, `self`/`Self`, struct field types, and ONE hop `let x = f()` through `f`'s declared return type (`-> Result<T, _>`/`-> Option<T>` take T) |

The (T, m) table declines 2+ corpus impls of the same pair (the ambiguity
rule). The glob class (d) of section 16 is the module plane's own star leg
(#552, single source binds, two globs ambiguous); unchanged here.

### 17.1 Receipt

Corpus `~/projects/rust-analyzer` at `af4111f`, 873 `crates/*/src` files
(`rust.receiver.files.txt`), binary at this lane's HEAD, one process per
resolve group. `before` = the numbers section 15/13 carried (baseline tsv at
`53189700b`-era binary), `after` = this lane. Raw:
`plans/extract-bench-2026-08-29/rust.parse.{call,resolve.runs}.tsv`
(overwritten), whole-corpus run `rust.receiver.resolve_whole.jsonl`, crawl
`rust.receiver.crawl.json`.

| | before | after |
|---|---:|---:|
| per-crate pipeline `|a|` (unique call rows) | 40,686 | 41,030 |
| a ∩ `ra_ap_ide` oracle | 12,624 | 13,892 |
| recall | 31.0% | **33.9%** |
| precision | 46.8% | **51.4%** |
| per-crate `ambiguous` call drops | 15,184 | **8,799** |
| per-crate `no_corpus_def` drops | 65,992 | 28,754 |
| per-crate drops reason `inferred` | (n/a) | 44,721 |
| whole-corpus `resolved_edge` | 59,921 | 83,331 |
| whole-corpus `unresolved` reason `ambiguous` | 55,073 | **15,444** |
| whole-corpus recall / precision vs oracle | 31.0% / 46.8% | 33.5% / 67.0% |
| crawl union (program 75 + test 7,774 roots) | 12,924 | **13,244** |
| program-root reachable defs | 3,117 | 3,731 |

The 65,992 -> 28,754 `no_corpus_def` move is mostly RECLASSIFICATION: method
sites on std receivers now carry the `inferred` reason (44,721 rows), the
`no_corpus_def` label stays for the non-method sites it always named. The
+320 union defs and +1,268 oracle intersections are net-new edges the
receiver and assoc legs minted; the precision gain is the ambiguous
name-match collisions those legs no longer fall into.

### 17.2 Gate

`cargo test --release --features cli --no-fail-fast`: 536 passed, 0 failed,
2 ignored. New: `tests/68_rust_receivers.rs` (10 tests, fail-first proven on
the pre-leg binary: 8 red), fixtures
`tests/fixtures/rust_findings/receivers/src/**`. Two goldens updated for the
new drop reason: `9_closure_resolved_edges.jsonl` (iter/map/collect now
`inferred`) and `wire_golden.jsonl` (regenerated; the corpus embeds the
former as a data fixture).

## 18. The receiver leg, pass 2 (lane `fix-extract-rust-receivers-2`)

### 18.1 The universe

One process, no chunking: `extract --resolve --family call,type --project-root
. $(cat rust.receiver.files.txt)` from the corpus root, 873 `crates/*/src`
files, corpus `~/projects/rust-analyzer` at `af4111f`. Wall **5.06s** before
the fix, **3.01s** after, one run each; the 10-second law holds with room.

Section 16 counted 52,101 `ambiguous` drops and section 17 left 15,444 in this
universe (8,799 in the bench's per-crate chunked universe). Both numbers are
this section's input; the whole-corpus 15,444 reproduced EXACTLY on this lane's
pre-fix binary, which is what makes the before column below comparable.

### 18.2 Why the receiver leg did not bind: all 15,444, classified

Not a projection from a sample. Every ambiguous drop carries its receiver
state, read out of a throwaway instrumented build (the drop `detail` grew
`#recv=<T>#impls=<n>`; the patch was reverted before any commit, so no `src/`
file in this PR carries it). `T` is then read against three corpus tables: the
extractor's own struct/enum/trait nodes (`--family type`, 873 serial runs),
a `type X = ..` alias scan, and a trait-body scan.

`rust.r2.ambiguous.sample300.tsv` (seed 7, committed beside this file) is a
300-row slice of the after column, one row per site with its class.

| # | class | before | after | who takes it |
|---|---|---:|---:|---|
| 1 | method site with NO receiver row at all | **2,232** | **80** | `rust_receivers.rs` `visit_local`, fixed below; the 80 left are macro-expanded spans |
| 2 | receiver type is the literal `Self` | **366** | **0** | `rust_receivers.rs` `tables`/`resolve_self`, fixed below |
| 3a | receiver type is a corpus ALIAS to an external type | 2,470 | 2,630 | nobody: `type SyntaxNode = rowan::SyntaxNode<..>`, no corpus edge is correct |
| 3b | receiver type is undeclared in the corpus | 1,370 | 1,449 | nobody: `FxHashMap`, `str`, `Arena` |
| 5 | corpus struct/enum, method from an external trait or a `Deref` | 1,501 | 1,974 | nobody: `Clone::clone`, `Hash::hash`, `Into::into` |
| 10 | `T::f()`, T external or an alias | 1,977 | 1,977 | nobody: `Arc::new`, `FxHashSet::default` |
| 8 | `T::f()`, T a corpus struct/enum | 1,366 | 1,366 | `rust.rs` `assoc_path_type` + `impl_target`: the pair has 0 or 2+ corpus impls |
| 11 | module-qualified path `mod::f()` | 1,367 | 1,367 | `rust.rs` `call_name_match_in_module`; `mem::take` and friends are external |
| 9 | free fn / bare name / struct literal | 1,364 | 1,178 | the module plane's glob leg (section 16 class d) |
| 7 | Named receiver, 2+ corpus impls of (T, m) | 759 | 815 | `rust_modules.rs` `impl_target`; 395 are `Vec::push`, where no corpus edge is correct |
| 12 | `T::f()`, T a corpus trait (`Default::default`) | 260 | 260 | trait dispatch, unbuilt |
| 4 | corpus struct/enum, method is a corpus trait DEFAULT body | 194 | 199 | `impl_facts` would need trait bodies plus a T -> traits table |
| 6 | receiver type is a corpus TRAIT (`dyn`/`impl`/bound) | 150 | 161 | trait dispatch, unbuilt |
| 6b | receiver type is a generic param name (`T`, `S`, `R`) | 52 | 60 | bound resolution, unbuilt |
| 13 | `Self::f()` | 16 | 16 | `rust.rs` `self_impl_type`; the pair has 0 or 2+ corpus impls |
| | TOTAL | **15,444** | **13,532** | |

Classes 3a, 3b, 5, 7 and 10 GROW because sites that had no receiver row now
have one, and it names an external type. 6,053 of the 13,532 that remain
(44.7%) are sites where the receiver's type is std, external, or an alias to
one: no corpus edge is correct and the drop is right. What is wrong there is
only the reason slug, which reads `ambiguous` where the tier can in fact say
"known receiver, no corpus impl".

Two file:line per class:

| # | sites |
|---|---|
| 1 | `crates/cfg/src/cfg_expr.rs:136` `keyword`; `crates/cfg/src/cfg_expr.rs:148` `next` |
| 2 | `crates/cfg/src/dnf.rs:151` `push` recv=Self; `crates/cfg/src/dnf.rs:155` `extend` recv=Self |
| 3a | `crates/base-db/src/input.rs:550` `into_iter` recv=Vec; `crates/base-db/src/input.rs:664` `insert` recv=FxHashSet |
| 3b | `crates/base-db/src/input.rs:402` `iter` recv=FxHashMap; `crates/base-db/src/input.rs:544` `shrink_to_fit` recv=FxHashMap |
| 4 | `crates/hir-def/src/attrs.rs:658` `krate` recv=GenericDefId; `crates/hir-def/src/expr_store/lower.rs:2401` `label` recv=ForExpr |
| 5 | `crates/base-db/src/editioned_file_id.rs:54` `field` recv=EditionedFileId; `crates/base-db/src/editioned_file_id.rs:59` `field` recv=EditionedFileId |
| 6 | `crates/cfg/src/lib.rs:134` `into_iter` recv=T; `crates/hir-def/src/item_tree/attrs.rs:104` `span_for` recv=S |
| 6b | `crates/hir-ty/src/diagnostics/decl_check.rs:728` `lookup` recv=L; `crates/hir-ty/src/next_solver/consts.rs:333` `consts` recv=R |
| 7 | `crates/base-db/src/change.rs:46` `push` recv=Vec; `crates/base-db/src/input.rs:769` `push` recv=Vec |
| 8 | `crates/base-db/src/change.rs:72` `LocalRoots::get`; `crates/base-db/src/change.rs:73` `LibraryRoots::get` |
| 9 | `crates/cfg/src/cfg_expr.rs:163` `query`; `crates/hir-def/src/attrs/docs.rs:975` `range` |
| 10 | `crates/base-db/src/change.rs:56` `FxHashSet::default`; `crates/base-db/src/change.rs:57` `FxHashSet::default` |
| 11 | `crates/base-db/src/input.rs:810` `mem::take`; `crates/base-db/src/input.rs:846` `std::mem::take` |
| 12 | `crates/base-db/src/input.rs:1005` `Default::default`; `crates/base-db/src/input.rs:1006` `Default::default` |
| 13 | `crates/hir-def/src/hir.rs:512` `Self::all`; `crates/hir-expand/src/proc_macro.rs:125` `Self::builder` |

### 18.3 The two fixes

| # | defect | throw site | rule now |
|---|---|---|---|
| 1 | `visit_local` returned before `syn::visit::visit_local` for every pattern that is not an ident, so the initializer of `let (a, b) = ..` and `let Some(x) = .. else` was never walked and each method call inside one got no receiver row | `rust_receivers.rs:157-177` (pre-fix) | the walk descends into every local, whatever the pattern; a destructuring pattern simply binds no name |
| 2 | `fn new() -> Self` put the literal `Self` in the same-file one-hop return table, so `let w = T::new(); w.m()` asked the impl table for `("Self", m)`, found nothing, and, the receiver type being KNOWN, emitted no row at all | `rust_receivers.rs:328` (the `let _ = &self_type;` leftover) | `-> Self` inside `impl T` reads as `T`, and `Self` reaching the walk from a param or a field resolves through the impl stack |

One more shape came out of fix 1 for free: the initializer is now walked in the
OUTER scope, so `let a = a.tick()` types its receiver through the binding it
shadows instead of through the one being introduced.

### 18.4 Step 3 of the brief: the field-shadow twin does not reproduce here

The brief asked for the rust half of ORACLES.REPORT.md section 13.4 finding 2:
17 `type` rows whose dst_path points at the referring file because a field or
token there carries the referenced type's name. Measured, it is two different
things and neither is a field shadow:

| claim | measurement | verdict |
|---|---|---|
| a same-named FIELD captures a type reference | the rust arm mints no TypeF node for a field (kinds emitted: struct, enum, trait, method, function, const), and **0 of 3,471** resolved type edges in the one-process run bind to a non-type def; same count, 0 of 1,706, in the chunked run | does not happen |
| `Resolver` binds to `hir-ty/src/infer/unify.rs` instead of `hir-def/src/resolver.rs`, 6 of the 17 rows | `unify.rs:716` declares a real `pub(super) struct Resolver`. Under one process the row is CORRECT (`crates/hir-def/src/resolver.rs`, via the module plane's import leg); under the chunked driver hir-def is not in the hir-ty chunk, so the only candidate is unify.rs | the chunked driver, finding 1, `resolve_runs.py` |
| `ProcMacroLoc -> ProcMacroKind` and `TargetFeatures -> Symbol` | one process binds the first to `crates/hir-expand/src/proc_macro.rs` (the oracle's answer) and emits no row at all for the second | the chunked driver |

Misbound dst_path against `rust.oracle.type.typedecl.tsv`, joined on
(src_path, src_name, dst_name):

| driver | ours rows | exact hits | dst_path misbound |
|---|---:|---:|---:|
| chunked, per crate (the bench universe) | 1,679 | 1,646 | **17** |
| one process, whole corpus | 2,946 | 2,184 | **11** |

The 11 that survive one process are a different defect: two REAL type
declarations sharing a name across crates (`GenericArgs` in
`hir-def/src/expr_store/path.rs` and in `hir-ty/src/next_solver/generic_arg.rs`,
5 rows; `FieldSource`, 2; `Layout`, 1, where the true target is a `type` alias
the extractor mints no node for). Filtering candidates to type declarations,
which is what the brief prescribed, changes none of them: the wrong target is
already a struct or an enum. No code change landed for step 3. What landed is a
pin, `a_field_named_like_a_type_does_not_capture_the_reference` in
`tests/68_rust_receivers.rs`, which fails the day a field starts capturing a
type reference.

### 18.5 Receipt

| | before | after |
|---|---:|---:|
| one process, `resolved_edge` | 83,331 | **83,583** |
| one process, `unresolved` reason `ambiguous` | 15,444 | **13,532** |
| one process, reason `inferred` | 41,161 | 43,396 |
| one process, reason `no_corpus_def` | 7,422 | 6,881 |
| one process, unique call rows normalized | 53,991 | 54,166 |
| one process, oracle intersection | 18,101 | **18,243** |
| one process, intersection / ours | 33.53% | **33.68%** |
| one process, intersection / oracle | 67.03% | **67.56%** |
| one process, oracle rows we miss | 8,903 | **8,761** |
| per-crate pipeline, unique call rows | 41,030 | 41,033 |
| per-crate pipeline, oracle intersection | 13,892 | **13,943** |
| per-crate pipeline, intersection / ours | 33.9% | **33.98%** |
| per-crate pipeline, intersection / oracle | 51.4% | **51.63%** |
| per-crate pipeline, `ambiguous` drops | 8,799 | **8,353** |
| type rows, dst_path misbound (chunked / one process) | 17 / (unmeasured) | 17 / **11** |

`before` for the per-crate rows is section 17.1's measurement of the same
script (`resolve_runs.py`, depth 3, 36 groups) at the same corpus sha; the
whole-corpus rows were both measured on this lane.

The oracle is `rust.oracle.call.tsv` (`ra_ap_ide`, 27,004 rows). `bench.py`
prints its two ratios as recall and precision with `a` = ours; the rows above
spell out which denominator each uses.

## 19. The path classes, lane `fix-extract-rust-paths-3` (8, 11, 9, 7)

### 19.1 What landed

Three commits on top of the receiver pass: `384336ef2` (inherent-impl-beats-trait tiebreak in `impl_target`, trait-in-scope filter through the module plane, module-qualified prefix resolution `use`/`self::`/`super::`/`crate::`/sibling-file `mod`, variant-constructor binding for `T::Variant` with 0 impls), `1d4f516a4` (use-binding cycle guard in `home_file`), `0bd4442e5` (crawl-kinks test adjustment). Tests: `tests/71_rust_paths.rs`, 8 tests, all green. The reclaimed stash ("rust-paths-3 opus partial") hunk for `ModuleTarget::covers` crate-root anchoring was superseded by the committed prefix work; the rest of the stash was census instrumentation, re-derived here. Nothing from the untracked second-reclaim files survived: `tests/71_rust_paths.rs` and `tests/fixtures/rust_findings/paths3/` were already committed by agent 3.

### 19.2 Receipt: one-process run, per-class census

Universe: section 18's, 873 `crates/*/src` files of rust-analyzer `af4111f`, one process, `--resolve --family call,type`, wall **2.06s / 1.82s / 2.39s** over three runs (section 18 measured 3.01s at #576).

`ambiguous` call drops: **13,532 -> 11,628**. Oracle overlap (`bench.py` vs `rust.oracle.call.tsv`, 27,004 rows): **18,243 -> 18,296**.

Census method: the section-18 throwaway tag rebuilt (`#recv=<T>#impls=<n>` on receiver drops, `#ty=<T>#impls=<n>` on path drops), classified by `rust.paths3.census.py` (committed beside this file; the tag patch reverted before commit). Corpus struct/enum/trait/alias tables from the same 873 files; test-module stub decls (`pub struct Arc;` in test mods) add some noise to class 8/10, so the before/after per-class numbers carry that caveat.

| # | class | before (18.2) | after | note |
|---|---|---:|---:|---|
| 8 | `T::f()`, T corpus struct/enum, 0 or 2+ corpus impls | 1,366 | 1,145 | variant ctors bind; inherent/trait tiebreak |
| 11 | module-qualified `mod::f()` | 1,367 | 1,207 | prefixes through the module plane |
| 9 | free fn / bare name / struct literal | 1,178 | 431 | glob leg + use bindings + prelude |
| 7 | named receiver, 2+ corpus impls | 815 | 194 | trait-in-scope tiebreak; the rest are external receivers (now class 3a/3b) |
| 10 | T external or alias | 1,977 | 1,765 | ceiling class, no correct corpus edge |
| 3a/3b/5 | receiver external/alias/external-trait | 6,053 | 6,881 | grew because class 7's external receivers reclassify here |

The class 7 shrink is the headline: 621 of its 815 rows named a receiver whose type has exactly one in-scope trait impl or one inherent impl, and those bind now; the remaining 194 are 2+ survivors after the tiebreak, which stay `ambiguous` by rule.

### 19.3 Gate

`cargo test --features cli --no-fail-fast`: 103 test binaries ok, 1 failure, `tests/45_emit_throughput.rs` `emit_throughput_350k_rows_under_budget` (piped emission 5.74-6.6s vs a 5.5s wall budget). Rerun 3x isolated, same outcome. The test exercises the JSONL emission path, which this lane's files (`rust.rs`, `rust_modules.rs`, `rust_receivers.rs`) do not touch; the run is borderline against the budget and sensitive to ambient machine load.

## 20. Trait dispatch, lane `fix-extract-rust-traits` (12, 4, 6, 6b, 8, 11)

### 20.1 What landed

One trait table in the module plane (`rust_modules.rs`): the second parse now
collects every trait's fn set (`TraitEntry`, declared and default bodies
alike) and the type -> traits it implements map, built from the `impl_facts`
entries the plane already walked. Three index methods do the dispatch:

| method | rule |
|---|---|
| `trait_impl_target` | the ONE corpus `impl Trait` defining the fn: class 12's impl-first arm |
| `trait_fn_target` | the trait's own fn def, declared or defaulted: classes 12 and 6 |
| `trait_default_target` | the one trait default body providing the fn for a type the caller names: classes 4 and 8's zero-impl arm |

Wiring: the assoc leg chains impl -> variant -> trait-impl -> trait-fn ->
trait-default; the receiver leg chains `impl_target` -> trait-fn (the
receiver's type IS a corpus trait) -> trait-default. Class 6b reads the bound
from the fn generics and where clause in `rust_receivers.rs`
(`trait_bounds_of_generics`); `principal_ty` now peels `dyn`/`impl Trait` to
the trait's name. Two span facts make the edges matchable: a declared trait
fn's span uses `def_span(ident, sig)` so the call facet's def for the bare
signature resolves the callee name, and `module_call` propagates
`HomeFile::External` so a `use std::mem; mem::take(..)` drop reads
`external`, never `ambiguous`.

Tests: `tests/72_rust_traits.rs`, 7 tests, fixtures under
`tests/fixtures/rust_findings/traits/`. Fail-first receipt: with the three
src files stashed, 3 of 6 red pre-fix (the impl-first and zero-impl pins were
green pre-fix because `impl_target` already bound those shapes).

### 20.2 Receipt: one-process run

Universe: section 18's, 873 `crates/*/src` files of rust-analyzer `af4111f`,
one process, `--resolve --family call,type`, wall 1.86 / 1.36 / 1.35 s over
three runs.

| | before (19.2) | after |
|---|---:|---:|
| `ambiguous` call drops | 11,628 | **11,470** |
| oracle overlap (`bench.py` vs `rust.oracle.call.tsv`) | 18,296 | **18,513** |
| recall (overlap / oracle) | 67.56% | **68.56%** |
| precision (overlap / ours) | 33.68% | 32.87% |
| `external` drops | 849 | 1,162 |
| `no_corpus_def` drops | 4,853 | 4,679 |
| ratchet rust call recall | 67.56 | **68.56** (`RATCHET_BUMP=1`) |

Census (`rust.paths3.census.py` over the throwaway-tagged run; the tag patch
was reverted before commit, same method as 19.2):

| # | class | 19.2 after | 20.2 after |
|---|---|---:|---:|
| 8 | `T::f()`, T corpus struct/enum, 0 or 2+ impls | 1,145 | 1,157 |
| 11 | module-qualified `mod::f()` | 1,207 | 1,077 |
| 9 | free fn / bare name / struct literal | 431 | 431 |
| 10+10a | T external or alias | 1,765 | 1,749 |
| 6 | receiver type is a corpus-or-external trait | 161 | 260 |
| 3a/3b/5 | receiver external/alias/external-trait | 6,881 | 6,795 |

Class 11's shape census (seed = the whole population, 1,273 `::`-shaped
ambiguous drops): 392 heads are a `use` binding to an external module (now
`external` drops), 573 are grouped-use heads of external crates (`ast::..`),
174 are multi-segment paths whose tail is an external type (class 10), 57 are
`crate::` chains needing a crate-root re-export leg, 13 are `super::`, 3 are
raw-ident mods (`r#type::`). The fixable top shape was the external one.
Class 6 grew because sites whose receiver names an EXTERNAL trait (`Into`,
`Iterator`) now carry the trait tag and classify there; those are the same
ceiling as class 5, and only corpus traits bind.

### 20.3 Gate

`cargo test --features cli --no-fail-fast`: **100 test binaries, 0 failures**
(the section 19 throughput flake did not reproduce).
`RATCHET_BUMP=1 just extract-ratchet` green; rust call recall floor
67.56 -> 68.56, rust type 26.18 -> 26.27, wall floor 960 -> 579 ms.

## 21. The trait blob, lane `fix-extract-rust-trait-blob`

Defect: `trait_fn_target` looked up `self.trait_fns[trait_name]` (bare trait
name key), found the entry whose fn name matched, and returned
`(sites[0].0, matched.span)` — the FIRST entry's blob with ANOTHER entry's
span. 139 of 390 trait names in the rust-analyzer corpus are declared in more
than one file, so those edges landed in the wrong file. `trait_impl_target`
and `trait_default_target` carried the same bare-name key.

### 21.1 Fix

`trait_fns` entries are now `TraitFnSite { blob, fn_name, span, default }`.
All three target fns take `caller: Option<&str>` and resolve a trait name
declared by several files as: the caller's own file, else the file the
caller's `use` of the name binds (`explicit_binding`), else unbound
(`unresolved{reason}`, never a guess). `trait_impl_target` keeps the
2+-remaining-unbound rule after the caller filter.

### 21.2 Numbers (single process, 873 files, rust-analyzer crates)

| leg | before | after |
|---|---|---|
| call vs `rust.oracle.call.tsv` recall | 70.23 | 70.31 |
| call vs `rust.oracle.call.tsv` precision | 43.32 | 43.35 |
| call vs `rust.scip_override.call.tsv` recall | 78.75 | 78.76 |
| call vs `rust.scip_override.call.tsv` precision | 42.44 | 42.44 |

Edge movement (before run at base vs after, same normal form):
5 edges changed `dst_path`, 19 edges appeared (previously unbound
`(trait, fn)` pairs that now bind per caller), 7 disappeared (previously
wrongly bound). `RATCHET_BUMP=1 just extract-ratchet` rust rows bumped:
70.31/43.35, 78.76/42.44, 26.27/74.20; wall 547 ms, RSS 653 MB. The first
bump run wrote wall 613 ms and FAILED the +15% tolerance; three isolated
reruns read 564/553/550/547 ms, all `ok`, so the FAIL was a wall-ratio flake
and the committed floor is the isolated median.

### 21.3 Gate

Fixture `tests/fixtures/rust_findings/trait_blob/` (`a.rs` and `b.rs` each
declare `trait Shape` with a default `area`; `a.rs` also impls it), test
`tests/75_rust_trait_blob.rs` red at HEAD with `got [("f", "a.rs"),
("g", "a.rs")]`, green after the fix.

## 22. The leak, classified and cut (lane `fix-extract-rust-grind`, arc 2)

### 22.1 The universe

One process, 873 `crates/*/src` files of rust-analyzer `af4111f`, binary at
this lane's HEAD, `--resolve --family call,type --project-root .`. The leak is
`oracle - ours` AFTER `rust.project.py --scope corpus --closure enclosing`:
**7,826** rows against `rust.oracle.call.tsv` (26,359 projected oracle rows,
18,533 overlap). **5,055** of the 7,826 are rows `rust.codeql.call.tsv` (arc 1)
also emits, so two thirds of the leak is reachable, not oracle noise.

Classifier: `rust.leak.classify.py` (committed beside this file). It joins each
leak row to the call SITES our own parse found inside the named caller whose
callee name matches, then reads the tier off our own output: `no_site`,
`misbind_*` (the site bound, elsewhere), or `drop:<reason>`. Not a projection
from a sample: all 7,826 rows are classified.

### 22.2 Tiers, before and after

| tier | before | after | meaning |
|---|---:|---:|---|
| `drop:inferred` | 3,670 | 3,670 | the receiver's type is not visible in scope |
| `misbind_name` | **1,344** | **5** | right file, wrong name: fixed below |
| `drop:ambiguous` | 1,329 | 1,362 | 2+ corpus defs of the name |
| `misbind_other_crate` | 599 | 585 | bound a same-named def in another crate |
| `no_site` | 432 | 432 | no parse site inside that caller |
| `drop:no_corpus_def` | 331 | 244 | no corpus def of the name |
| `misbind_sibling_file` | 119 | 119 | bound a same-named def in a sibling file |
| `drop:external` | 2 | 0 | |
| TOTAL | **7,826** | **6,417** | |

Two file:line per tier:

| tier | sites |
|---|---|
| `drop:inferred` | `crates/cfg/src/cfg_expr.rs:30` `cmp` -> `crates/intern/src/symbol.rs as_str`; `crates/hir-ty/src/method_resolution.rs:455` `lookup_impl_method_query` -> `next_solver/generic_arg.rs iter` |
| `misbind_name` | `crates/ide-ssr/src/parsing.rs:223` `parse_pattern` -> `Placeholder` (we said `PatternElement`); `crates/hir/src/has_source.rs:112` `source` -> `Named` (we said `FieldSource`) |
| `drop:ambiguous` | `crates/hir-def/src/attrs.rs:396` `contains_no_std` -> `syntax/src/ast/generated/nodes.rs meta`; `crates/hir-ty/src/next_solver/inspect.rs:98` `constrain_and` -> `obligation_ctxt.rs evaluate_obligations_error_on_ambiguity` |
| `misbind_other_crate` | `crates/hir/src/lib.rs:1481` `fields` -> `hir-def/src/signatures.rs fields` (we said `hir/src/lib.rs`); `crates/ide/src/inlay_hints/closure_captures.rs:27` `hints` -> `syntax/.../nodes.rs move_token` |
| `no_site` | `crates/base-db/src/input.rs` `normalize_dashes` -> `CrateName`; `crates/hir-ty/src/infer/coerce.rs` `fold_const` -> `next_solver/interner.rs iter` |
| `misbind_sibling_file` | `crates/hir-def/src/nameres.rs:318` `declaration` -> `item_tree.rs file_id`; `crates/tt/src/lib.rs:239` `fmt` -> `tt/src/iter.rs is_empty` |

### 22.3 The class that was fixed: a variant constructor named its enum

1,340 of the 1,344 `misbind_name` rows are one defect. `variant_ctor_target`
(`rust_modules.rs:702`) looked the pair up in `enum_variants`, which stored only
the DECLARING FILE, and then read the span of the def whose name is the ENUM.
The emitted `resolved_edge` therefore read `callee_name = "PatternElement"`
where every rust call oracle spells the edge `Placeholder`. Each such row cost
twice: the oracle row leaked and the enum row was excess.

| leg | before | after |
|---|---|---|
| `RustModuleFacts.enums` | `(enum, Vec<variant name>)` | `(enum, Vec<(variant name, def span)>)`, the ident span only |
| `RustModuleIndex.enum_variants` | `(enum, variant) -> Vec<path>` | `(enum, variant) -> Vec<(path, span)>` |
| `variant_ctor_target` | the enum's def span | the variant's own def span |
| `call_defs_in_items` (`rust.rs`) | no def for a variant | one `CallKind::Free` def per variant, ident span |

Three more defects fell out of minting those defs, each a pre-existing hazard
the new defs made visible:

| # | defect | throw site | rule now |
|---|---|---|---|
| 1 | a type reference resolved through `corpus_defs`'s single-site rule, which counts CALL-plane defs; a variant or fn sharing a type's name made the pick ambiguous and dropped the edge | `rust.rs:837` `resolve_type_dst` | only `FamilyTag::Type` sites are candidates |
| 2 | a type reference bound through the module plane landed on the CALL facet's span, so the row read `target_name = null` (the export table prefers the call facet, `rust_modules.rs:1243`) | `rust.rs:832` | the type leg asks `RustModuleIndex::type_target`, which re-aims at the type facet in the same file |
| 3 | every def spliced out of one macro expansion carries the macro CALL's span (156 defs share span 3136..5229 in `crates/hir/src/diagnostics.rs`), so a span-keyed name lookup returned whichever def won the slot | `rust.rs:938` `same_file_call_match`, `rust_modules.rs:1243` export table | a span several names share binds nothing: `same_file_call_match` returns None, and `named_defs` keeps a collapsed-span def only when its name has no other site |

An mbe-expanded variant reports the whole macro call as its ident span, so
`variant_def_span` returns None unless the span covers exactly the ident.

### 22.4 Receipt

Single process, 873 files, `rust-analyzer af4111f`, wall 1.45 / 0.56 / 0.56 s.

| leg | before | after |
|---|---:|---:|
| call vs `rust.oracle.call.tsv` recall | 70.31 | **75.66** |
| call vs `rust.oracle.call.tsv` precision | 43.35 | **46.29** |
| call vs `rust.scip_override.call.tsv` recall | 78.76 | **78.90** |
| call vs `rust.scip_override.call.tsv` precision | 42.44 | 42.22 |
| call vs `rust.codeql.call.tsv` recall | 64.97 | 64.96 |
| call vs `rust.codeql.call.tsv` precision | 71.51 | 71.03 |
| type vs `rust.oracle.type.typedecl.tsv` recall | 26.27 | 26.23 |
| type vs `rust.oracle.type.typedecl.tsv` precision | 74.20 | **88.19** |
| ra overlap | 18,533 | **19,942** |
| leak rows | 7,826 | **6,417** |
| unique call rows | 56,336 | 56,591 |

The two precision losses are an oracle-convention disagreement, not a
regression: codeql's `Call` class excludes tuple-struct and tuple-variant
instantiation BY CONSTRUCTION (`rust-all/codeql/rust/elements/internal/CallImpl.qll`:
"a `CallExpr` that is _not_ an instantiation of a tuple struct or a tuple
variant"), and raw scip mostly follows it, while the ra_ap_ide call hierarchy
counts the ctor as a call. The trade is +1,409 ra overlap and +22 scip overlap
against 202 extra scip-scored rows and 315 extra codeql-scored ones. Both
floors were rewritten with `RATCHET_FORCE=1`; the codeql row is the only floor
that moved DOWN (71.51 -> 71.03) and this paragraph is its receipt.

### 22.5 What is left, and what needs a type checker

`drop:inferred` is 57% of the remaining 6,417 and is the class section 18
already priced: the receiver's type is not written anywhere the parse can read
it (`x.m()` where `x` came out of a chain, a closure param, or an
`impl Trait` return). Binding it needs type inference, not more name matching.
Throw site: `rust.rs:1510` (`UnresolvedReason::Inferred`), set when
`ReceiverOutcome::Inferred` reaches `call_drops`. Nothing short of a checker
moves it.

`no_site` (432) is the mbe frontier: the caller def our parse mints does not
cover the call, or the call is inside an expansion we do not splice.

The two `misbind_*_file` tiers (704) are the corpus-unique name match binding a
same-named def in the wrong file. That is arc 3's excess class seen from the
other side and is the next largest fixable one.

## 23. The excess, classified and cut (lane `fix-extract-rust-grind`, arc 3)

### 23.1 A stricter excess set

`ours - oracle` against `rust.oracle.call.tsv` alone is 23,142 rows, and
RUST-PARITY.REPORT.md section 6 measured 80% of it as the ra call hierarchy's
own per-crate scope rather than over-emission. Arc 1's `rust.codeql.call.tsv`
gives a second opinion, so this section scores the rows NEITHER oracle emits:

```
doubly unsupported = ours_projected - rust.oracle.call.tsv - rust.codeql.call.tsv
```

**10,693** rows at arc 2's HEAD, of 43,084 emitted. Classifier:
`rust.excess2.classify.py` (committed beside this file), which joins each row
back to the `resolved_edge` rows that minted it and reports the LEG plus the
site's shape.

| tier / shape | before | after | example |
|---|---:|---:|---|
| `name_resolve` / receiver typed locally | 3,420 | 2,628 | `crates/base-db/src/input.rs:582` `add_dep` -> `input.rs into_iter` |
| `name_resolve` / bare name, other crate | 3,039 | 909 | `crates/base-db/src/editioned_file_id.rs:36` `parse_errors` -> `intern/src/symbol/symbols.rs create` |
| `name_resolve` / bare name, sibling file | 1,425 | 1,425 | `editioned_file_id.rs:49` `current_edition` -> the same file's `current_edition` |
| `no_def_in_dst` / bare name, sibling file | 1,043 | 1,043 | `input.rs:553` `add_crate_root` -> `input.rs CrateBuilder` |
| `name_resolve` / receiver is own field | 438 | 363 | `editioned_file_id.rs:69` `edition` -> the same file's `edition` |
| `name_resolve` / bare name, same crate | 403 | 402 | `hir-def/src/attrs/docs.rs:451` `get_horizontal_trim` -> `expr_store/path.rs first` |
| `import_resolve` / bare name, sibling file | 207 | 200 | `hir-def/src/lang_item.rs:128` `lang_items` -> `nameres.rs crate_local_def_map` |
| TOTAL | **10,693** | **7,681** | |

### 23.2 The class that was fixed: a collapsed macro span

2,998 of the 10,693 rows, 28%, name ONE coordinate:
`crates/intern/src/symbol/symbols.rs create`. `define_symbols!` expands to 537
defs there and `rust_mbe.rs` gives every spliced item the macro CALL's own span
(`4394..16648`), so `(blob, span)` stops naming a def: `name_at` returns
whichever def won the span slot, and `create` won. Every bare `x.into()` in the
corpus name-matched the `into` symbol const declared inside that expansion and
emitted `-> symbols.rs create`.

The rule now: a def coordinate several NAMES share names nothing, so no name
match binds there. `RustModuleIndex` carries the `(blob, span)` set at build
time (`is_collapsed`) and `Resolve<CallF>` filters `name_t` through it. Arc 2
had already applied the same rule to two narrower paths (`same_file_call_match`
and the module plane's export table); this is the corpus-wide one.

The guard is per COORDINATE, never per file: `77_rust_collapsed_span.rs` pins
that a clean def in the same file as a collapsed one still binds.

### 23.3 Receipt

| leg | arc 2 | arc 3 | vs lane start |
|---|---:|---:|---:|
| ra_ap_ide recall | 75.66 | 75.66 | 70.31 |
| ra_ap_ide precision | 46.29 | **49.77** | 43.35 |
| raw scip recall | 78.90 | 78.90 | 78.76 |
| raw scip precision | 42.22 | **45.24** | 42.44 |
| codeql recall | 64.96 | 64.96 | 64.97 |
| codeql precision | 71.03 | **76.15** | 71.51 |
| emitted rows (projected) | 43,084 | **40,072** | 42,752 |
| doubly-unsupported excess | 10,693 | **7,681** | (unmeasured) |

Zero recall cost on all three oracles: the 3,012 rows removed are rows no
oracle had. Arc 2's two forced floor drops are recovered and passed: every
precision floor now sits above where the lane found it.

### 23.4 Two rules measured and REJECTED

Both target the same class (`T::f()` falling through to a bare-name match) and
both cost more recall than they buy precision. Recorded so the next lane does
not re-derive them.

| rule | ra r/p | scip r/p | codeql r/p | verdict |
|---|---|---|---|---|
| (kept baseline) | 75.66 / 49.77 | 78.90 / 45.24 | 64.96 / 76.15 | |
| `T::f()` with any uppercase path segment never falls through to the bare-name legs | 75.22 / 50.78 | **76.65** / 45.01 | 64.59 / 77.65 | rejected: scip recall -2.25 |
| same, but only when the corpus declares no TYPE named T | 75.42 / 50.53 | **77.20** / 45.04 | 64.86 / 77.35 | rejected: scip recall -1.70 |

The `1,425 + 1,043` sibling-file rows the rules were aimed at are real
over-emission, but they must be cut by binding the qualifier, not by refusing
the fallback: raw scip agrees with the fallback on more rows than it costs.

### 23.5 What is left

| class | rows | why it stays |
|---|---:|---|
| receiver typed locally, both oracles disagree | 2,628 | the receiver's declared type has one corpus impl of the method and the compiler picks a std or trait one; needs a checker (`rust.rs:1315` `impl_target`) |
| bare name, sibling or same file | 2,468 | the qualifier resolution class of 23.4, open |
| `no_def_in_dst` (the dst def is a TYPE, not a fn) | 1,610 | struct-literal and tuple-struct ctor rows; ra and codeql both exclude a struct ctor from the call graph, and 22.4's variant decision does not extend to them |

## 24. The checker tier (lane `fix-extract-rust-checker`)

User decision, 2026-08-30: scip is the standard for modules and references;
calls and types are syntactically variable; rust is complex, so the rust arm
runs WITH the language's own machinery. go and ts stay on the syntax leg.

### 24.1 What the tier is, and what it is not

rust-analyzer is loaded in-process (`ra_ap_load-cargo` reads `cargo metadata`,
never `cargo build`) and answers the DESTINATION of a reference. Everything
else stays this crate's: the parse enumerates the sites, `covering_def` names
the caller, the site span is ours, and `call_drops` still emits a row per
unbound site. No bespoke trait solver and no bespoke inference exist anywhere
in the tier.

| piece | file | role |
|---|---|---|
| seam | `src/lang/rust_checker.rs` | plain data plus the corpus join; NO ra crate in the build graph |
| loader | `src/lang/rust_checker_ra.rs` | `#[cfg(feature = "rust-checker")]`; the walk and the ra calls |
| call leg | `src/lang/rust.rs` `Resolve<CallF>` | the checker answers before the name match and before scip |
| type leg | `src/lang/rust.rs` `resolve_type_dst` | same, keyed on (file, name) |
| drops | `src/lang/rust.rs` `call_drops` | a checker-external site reads `external` |
| plumbing | `src/project.rs` `load_rust_checker` | build, log, and fall back |

Three answers, not two. `Corpus(blob, span)` is a corpus definition;
`External` is the checker resolving the reference OUTSIDE the corpus, which is
KNOWLEDGE and suppresses the name match; absent is the syntax leg's turn.
`External` alone moved ra precision 55.40 -> 55.98 and codeql precision
78.03 -> 78.78 at zero recall cost.

### 24.2 Two offset planes joined

`syn_span` writes a line's start byte plus its CHARACTER column; rust-analyzer
writes raw byte offsets. `OffsetMap` converts every ra offset into the parse
plane's unit, so a reference joins to a site by containment with no second
index. The call plane keys on the reference's own range (rightmost match
inside the site span, name-checked); the type plane keys on (file, name)
because a `TypeEdgeCandidate` carries no reference span, and a name one file
resolves two ways binds nothing.

### 24.3 Receipt

873 `crates/*/src` files of rust-analyzer `af4111f`, one process,
`--resolve --family call,type`, ratchet projection (`--scope corpus
--closure enclosing`, the Rust port in `tests/bench/mod.rs`).

| oracle | syntax leg | checker tier | delta |
|---|---|---|---|
| `rust.oracle.call.tsv` (ra_ap_ide) | 75.66 / 49.77 | **93.68 / 55.98** | +18.02 / +6.21 |
| `rust.codeql.call.tsv` | 64.96 / 76.15 | **73.36 / 78.78** | +8.40 / +2.63 |
| `rust.scip_override.call.tsv` (raw scip) | 78.90 / 45.24 | 77.89 / 41.02 | -1.01 / -4.22 |
| `rust.oracle.type.typedecl.tsv` | 26.23 / 88.19 | **27.33 / 89.31** | +1.10 / +1.12 |

**ra_ap_ide is the checker, so its row is partly a self-comparison.** The
independent receipt is codeql: a different tool with its own extractor, whose
recall moves 64.96 -> 73.36 and whose precision moves 76.15 -> 78.78 at the
same time. Both directions on an independent oracle is the claim that survives.

Row movement, unprojected, checker minus syntax:

| set | rows | in ra | in codeql | in raw scip | in no oracle |
|---|---:|---:|---:|---:|---:|
| added | 6,099 | 4,755 | 4,353 | 287 | 1,273 |
| removed | 513 | 3 | 9 | 315 | 192 |

### 24.4 The one floor that moved down

Raw scip loses 1.01 pt of recall and 4.22 pt of precision. Both oracles in
that comparison are rust-analyzer: `rust.scip_override.call.tsv` is its scip
output and `rust.oracle.call.tsv` is its call hierarchy, and they disagree by
convention at method sites. 315 of the 513 removed rows are scip rows, and
their shape is one class: a same-file name match (`editioned_file_id.rs
current_edition -> editioned_file_id.rs current_edition`, `find_path.rs
find_path -> find_path.rs crate_root`) that the checker re-aims at the
declaring crate. The precision drop is arithmetic: the tier emits 2,420 more
projected rows against a 15,647-row oracle.

Section 23.4 rejected two heuristics for costing scip recall. This is not a
heuristic; it is the compiler's own answer, and the user decision names it as
the rust door. Both floors are written with `RATCHET_FORCE=1` and this section
is their receipt.

### 24.5 Cost

| leg | measure |
|---|---|
| `cargo metadata` + salsa workspace load | **0.50-0.53 s** |
| the resolve walk over the loaded workspace | **9.6-10.8 s** |
| whole run, 873 files, one process | **10.4-10.7 s** median of 3, over 5 repeats (was 0.54 s) |
| process-peak RSS | **2,122-2,537 MB** over the same 5 repeats (was 597 MB) |
| cold build of the ra crate graph | ~380 s, 239 crates, `--features rust-checker` |

The load is index-build class and carries the SCIP exception to the 10-second
law. The WALK is not: 9.6 s of per-run resolve is the tier's real price and it
recurs on every run, because `resolve_project` holds no state between calls.
RSS is 3.5x the syntax leg and over the 700 MB working ceiling; the salsa
database for a 30-crate workspace is the whole of it.

Recall and precision are byte-stable across all 5 repeats; only wall and RSS
move, and the RATCHET.tsv ceilings carry the worst of the 5.

The tier is OFF by default and out of the `cli` feature. `--rust-checker` on a
binary built without `--features rust-checker` logs one line and changes
nothing. `just extract-ratchet` builds the rust leg with the feature and the
other two legs without it.

### 24.7 The trap: `project_root` is not a free parameter

The tier's first ratchet measurement read ra 93.71 / codeql 86.36, and it was
wrong. Giving the ratchet's rust request a `project_root` so the checker could
find its workspace also tripped `load_scip`'s informed-by-default leg
(`project.rs:844`), which adopts any FRESH cached index for the file set. The
run was no longer diet: 12,234 `scip_override` and 4,403 `scip_macro` edges
rode in with it, and codeql recall inherited most of the difference.

The tier now carries its own root (`ResolveRequest.rust_checker:
Option<&Path>`) and the ratchet keeps `project_root: None`. The check that
catches this class is a record-kind census of the raw JSONL: a diet run emits
`name_resolve`, `import_resolve` and nothing else.

### 24.6 What the tier does not fix

| class | count | why |
|---|---:|---|
| answers naming a corpus file whose parse minted no def there | 14,069 | the coordinate join misses (mbe-expanded defs, defs our parse does not mint); those sites fall back to the syntax leg |
| type recall | 27.33 | the checker re-aims destinations and cannot add candidates; `type_edge_candidates` enumerates 2,553 rows against an 8,343-row oracle, so candidate coverage is the ceiling, not resolution |
| calls inside macro invocations | unchanged | the parse mints no site there, so there is nothing for the checker to answer; `scip_macro` is still the only leg that reaches them |
