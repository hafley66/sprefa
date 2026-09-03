# Held-out oracle report

Every number below is read from `SCORES.tsv` (callable-callee oracle) and
`SCORES.reference-graph.tsv` (the prior run against the unfiltered
`scip_fn_edge` reference graph). `python3 run.py report` regenerates this file.

## TOC

1. Overfit gap per measure id
2. Per language: tuning corpus beside every held-out repo
3. Skipped
4. Findings (FINDINGS.md, hand-written)

## 1. Overfit gap, recall / precision

| measure id | tuning corpus | held-out median | gap | n held-out |
|---|---|---|---|---|
| go.call.syntax.scip | 97.12% / 78.35% | 92.96% / 66.47% | -4.16 pt / -11.88 pt | 3 |
| python.call.syntax.scip | not measured | 61.66% / 54.32% | no tuning row | 3 |
| rust.call.checker.scip | 90.06% / 71.24% | 89.36% / 64.14% | -0.70 pt / -7.10 pt | 3 |
| rust.call.syntax.scip | 66.38% / 67.02% | 86.79% / 49.89% | +20.41 pt / -17.13 pt | 3 |
| ts.call.checker.scip | 68.61% / 29.41% | 65.31% / 25.33% | -3.30 pt / -4.08 pt | 3 |
| ts.call.syntax.scip | 61.97% / 28.58% | 58.91% / 23.84% | -3.06 pt / -4.74 pt | 3 |

## 2. Per language

### go

| measure id | repo | class | files | recall | precision | oracle calls / refs | tuning recall | gap pt | reference-graph recall | tier decline |
|---|---|---|---|---|---|---|---|---|---|---|
| go.call.syntax.scip | typescript-go | tuning | 4870 | 97.12% | 78.35% | 74856 / 211558 | 97.12% | - | 35.27% | none |
| go.call.syntax.scip | fatedier/frp | heldout | 351 | 92.96% | 66.47% | 3350 / 15352 | 97.12% | -4.16 | 22.29% | none |
| go.call.syntax.scip | gitleaks/gitleaks | heldout | 204 | 93.53% | 90.28% | 1699 / 5490 | 97.12% | -3.59 | 29.07% | none |
| go.call.syntax.scip | mickael-kerjean/filestash | heldout | 369 | 79.29% | 55.10% | 2941 / 14212 | 97.12% | -17.83 | 18.44% | none |

### python

| measure id | repo | class | files | recall | precision | oracle calls / refs | tuning recall | gap pt | reference-graph recall | tier decline |
|---|---|---|---|---|---|---|---|---|---|---|
| python.call.syntax.scip | getredash/redash | heldout | 292 | 61.66% | 63.45% | 3427 / 11538 | not measured | - | 18.34% | none |
| python.call.syntax.scip | oraios/serena | heldout | 439 | 52.57% | 41.12% | 4841 / 21473 | not measured | - | 11.83% | none |
| python.call.syntax.scip | ultralytics/yolov5 | heldout | 52 | 63.93% | 54.32% | 610 / 3373 | not measured | - | 11.71% | none |

### rust

| measure id | repo | class | files | recall | precision | oracle calls / refs | tuning recall | gap pt | reference-graph recall | tier decline |
|---|---|---|---|---|---|---|---|---|---|---|
| rust.call.checker.scip | rust-analyzer | tuning | 873 | 90.06% | 71.24% | 52081 / 149045 | 90.06% | - | 33.13% | tier.rust-analyzer: /Users/chrishafley/projects/rust-analyzer/crates/proc-macro-srv/proc-macro-test/imp/src/lib.rs: owns no module in the loaded crate graph (cfg-gated, or outside every crate root) |  |
| rust.call.checker.scip | BigPizzaV3/CodexPlusPlus | heldout | 121 | 89.36% | 81.84% | 6920 / 18647 | 90.06% | -0.70 | 36.54% | tier.rust-analyzer: /private/tmp/heldout-checkouts/BigPizzaV3_CodexPlusPlus/crates/codex-plus-core/src/windows_integration.rs: owns no module in the loaded crate graph (cfg-gated, or outside every cra |
| rust.call.checker.scip | rust-lang/rust-clippy | heldout | 2345 | 92.32% | 46.21% | 7680 / 18579 | 90.06% | +2.26 | 41.24% | tier.rust-analyzer: /private/tmp/heldout-checkouts/rust-lang_rust-clippy/clippy_dev/src/deprecate_lint.rs: owns no module in the loaded crate graph (cfg-gated, or outside every crate root) | tier.rust |
| rust.call.checker.scip | web-infra-dev/rspack | heldout | 1367 | 83.74% | 64.14% | 25604 / 94058 | 90.06% | -6.32 | 25.89% | tier.rust-analyzer: /private/tmp/heldout-checkouts/web-infra-dev_rspack/crates/rspack_core/src/debug_info.rs: owns no module in the loaded crate graph (cfg-gated, or outside every crate root) | tier.r |
| rust.call.syntax.scip | rust-analyzer | tuning | 873 | 66.38% | 67.02% | 52081 / 149045 | 66.38% | - | 26.01% | none |
| rust.call.syntax.scip | BigPizzaV3/CodexPlusPlus | heldout | 121 | 86.79% | 71.30% | 6920 / 18647 | 66.38% | +20.41 | 35.38% | none |
| rust.call.syntax.scip | rust-lang/rust-clippy | heldout | 2345 | 91.64% | 45.03% | 7680 / 18579 | 66.38% | +25.26 | 40.67% | none |
| rust.call.syntax.scip | web-infra-dev/rspack | heldout | 1367 | 63.29% | 49.89% | 25604 / 94058 | 66.38% | -3.09 | 19.90% | none |

### ts

| measure id | repo | class | files | recall | precision | oracle calls / refs | tuning recall | gap pt | reference-graph recall | tier decline |
|---|---|---|---|---|---|---|---|---|---|---|
| ts.call.checker.scip | TypeScript-5.9 | tuning | 600 | 68.61% | 29.41% | 31711 / 112706 | 68.61% | - | 1.78% | none |
| ts.call.checker.scip | trpc/trpc | heldout | 864 | 44.70% | 9.74% | 1405 / 8296 | 68.61% | -23.91 | 8.61% | tier.tsc: the driver failed: Node.js v20.20.2 |
| ts.call.checker.scip | umami-software/umami | heldout | 1044 | 96.40% | 55.70% | 4144 / 15774 | 68.61% | +27.79 | 25.92% | none |
| ts.call.checker.scip | vitejs/vite | heldout | 557 | 65.31% | 25.33% | 2390 / 9699 | 68.61% | -3.30 | 15.62% | none |
| ts.call.syntax.scip | TypeScript-5.9 | tuning | 600 | 61.97% | 28.58% | 31711 / 112706 | 61.97% | - | 1.61% | none |
| ts.call.syntax.scip | trpc/trpc | heldout | 864 | 44.70% | 9.74% | 1405 / 8296 | 61.97% | -17.27 | 8.61% | none |
| ts.call.syntax.scip | umami-software/umami | heldout | 1044 | 96.24% | 59.10% | 4144 / 15774 | 61.97% | +34.27 | 25.92% | none |
| ts.call.syntax.scip | vitejs/vite | heldout | 557 | 58.91% | 23.84% | 2390 / 9699 | 61.97% | -3.06 | 15.62% | none |

## 3. Skipped

No repo skipped.

## 4. Findings

Hand-written beside the generated sections. Every number is from `SCORES.tsv`, `SCORES.reference-graph.tsv`, or the command named in the row.

### 4.1 The reference-graph oracle was the whole "overfit"

| control | reference-graph recall | callable-callee recall | RATCHET.tsv floor (other oracle) |
|---|---|---|---|
| go.call.syntax.scip, typescript-go | 35.27 | 97.12 | 98.96 (codeql2) |
| ts.call.syntax.scip, TypeScript-5.9 | 1.61 | 61.97 | 88.20 (tsc oracle) |
| ts.call.checker.scip, TypeScript-5.9 | 1.78 | 68.61 | 95.02 (tsc oracle) |
| rust.call.syntax.scip, rust-analyzer | 26.01 | 66.38 | (RATCHET has no scip row) |
| rust.call.checker.scip, rust-analyzer | 33.13 | 90.06 | (RATCHET has no scip row) |

Two changes moved the ts control, both in `run.py`: the callee filter (`is_callable_symbol`, `().` descriptor) and the file scope (the tuning run now walks `src/**` minus `src/lib` the way `tests/bench/mod.rs` `wants` does; the whole-repo walk sampled 4,864 files of which 600 were under `src/`, and read 5.37 after the filter alone).

### 4.2 ts control gap to the tsc oracle: 26 pt, 57% of it is caller attribution

12,060 oracle rows missed on the 600-file src set, by which column disagrees (`hr.tsdiag2`, the kept work dir):

| miss category | rows | share |
|---|---|---|
| caller name differs, same (src, dst, callee) | 6,903 | 57.2% |
| file pair present, both names differ | 2,740 | 22.7% |
| callee name differs, same (src, caller, dst) | 1,311 | 10.9% |
| file pair absent from ours | 1,106 | 9.2% |

Caller-name rows read `oracle=transformES2018 ours=visitFunctionExpression`, `oracle=createLanguageService ours=getCodeFixesAtPosition`: scip-typescript emits a nested function as a `local N` symbol, `scip_v5_rels.rs:340` `usable_symbol` drops locals from the callable index, and `:361` `enclosing_fn` then attributes the call to the nearest preceding non-local callable, the outer function. The tsc oracle and the extractor both name the innermost function. This is the scip oracle's protocol, so the 10-pt receipt against `ts5.call.syntax.oracle` is not met by this oracle as built; next action is a `local` callable in the `fn_defs` pass of `scip_v5_rels.rs` (needs `SymbolInformation.kind` or the occurrence's `syntax_kind`, both decoded per `types.rs:2433`), with `scip_def`/`scip_name` rows for the same locals so the join in `oracle_rows` can place them.

### 4.3 ts checker tier: two of three held-out repos now answer; the third and the whole-repo control decline on a tsc stack overflow

| run | syntax | checker | decline |
|---|---|---|---|
| umami | 96.24 | 96.40 | none |
| vite | 58.91 | 65.31 | none |
| trpc | 44.70 | 44.70 | `tier.tsc: the driver failed: Node.js v20.20.2` |
| TypeScript-5.9, whole-repo walk (superseded row) | 5.37 | 5.37 | same |
| TypeScript-5.9, src scope | 61.97 | 68.61 | none |

The driver's stderr (`$TMPDIR/sprefa-ts-checker-*/indexer.stderr.log`) reads `RangeError: Maximum call stack size exceeded` at `typescript.js:60064 getNameOfSymbolAsWritten` (trpc) and `:121490 pipelineEmitWithHintWorker` (TypeScript-5.9 tests/cases). The ledger only keeps the last stderr line (`scip_ensure.rs:642` `stderr_tail`). Site: `src/lang/ts_checker.mjs` (the driver) and the spawn at `src/lang/ts_checker.rs:410` `run_capped(&["node", script, request], ...)`; a `--stack-size` on the node argv is the shape, and neither file is this lane's. Recorded here, not fixed.

### 4.4 rust checker rows: real answers with per-file partial declines

The first pass ran a binary built without `rust-checker`; every checker row carried `tier.rust-analyzer: the rust checker tier needs --features rust-checker; falling back to the syntax leg` and equalled its syntax row on rspack, CodexPlusPlus and clippy (rust-analyzer alone read 90.06 against 66.38, the `--project-root` module plane). The `--features cli,ts-checker,rust-checker` release build landed (cold, 510 units, one attempt) and the rust rows were replaced:

| run | syntax | checker (feature off) | checker (rust-analyzer) | decline now |
|---|---|---|---|---|
| rust-analyzer (control) | 66.38 / 67.02 | 90.06 / 67.64 | 90.06 / 71.24 | `crates/proc-macro-srv/proc-macro-test/imp/src/lib.rs: owns no module in the loaded crate graph` |
| rspack | 63.29 / 49.89 | 63.29 / 49.89 | 83.74 / 64.14 | `crates/rspack_core/src/debug_info.rs: owns no module ...` |
| CodexPlusPlus | 86.79 / 71.30 | 86.79 / 71.30 | 89.36 / 81.84 | `crates/codex-plus-core/src/windows_integration.rs: owns no module ...` |
| rust-clippy | 91.64 / 45.03 | 91.64 / 45.03 | 92.32 / 46.21 | `clippy_dev/src/deprecate_lint.rs: owns no module ...` |

The surviving decline text is a per-file partial (cfg-gated files, or files outside every crate root), one `tier.rust-analyzer` diagnostic per such file, truncated to 200 chars in the ledger column. Checker wall on rspack: 98.9 s for 1,367 files (`wall_ms` column), over the 10-second law and filed as a cost row, not a budget.

### 4.5 Cache and reuse notes

- `--indexer rust` scopes its index to `<root>/.dl/.state/indexer-rust/`; rust-analyzer's existing `index.scip` (Aug 29, same sha `af4111f0bf85`) was hard-linked there so the control did not re-index.
- `HELDOUT_KEEP_WORK=1` keeps `/tmp/heldout-checkouts/<repo>` and `/tmp/heldout-work/<repo>/{oracle.jsonl,ours.<tier>.jsonl,files.txt}`; a scoring change re-reads them and a rerun skips the clone.
- The TypeScript-5.9 `--family scip` dump is a 3.1 GB jsonl per run; the index itself is 1.1 GB.
