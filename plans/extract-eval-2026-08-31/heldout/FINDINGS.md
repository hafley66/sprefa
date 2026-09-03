Hand-written beside the generated sections. Every number is from `SCORES.tsv`, `SCORES.reference-graph.tsv`, or the command named in the row.

### 4.1 The reference-graph oracle was the whole "overfit"

| control | reference-graph recall | callable-callee recall | RATCHET.tsv floor (other oracle) |
|---|---|---|---|
| go.call.syntax.scip, typescript-go | 35.27 | 97.12 | 98.96 (codeql2) |
| ts.call.syntax.scip, TypeScript-5.9 | 1.61 | 61.97 | 88.20 (tsc oracle) |
| ts.call.checker.scip, TypeScript-5.9 | 1.78 | 68.61 | 95.02 (tsc oracle) |
| rust.call.syntax.scip, rust-analyzer | 26.01 | 66.38 | (RATCHET has no scip row) |

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

### 4.4 rust checker rows carry a decline: the binary was built without `rust-checker`

Every `rust.call.checker.scip` row reads `tier.rust-analyzer: the rust checker tier needs --features rust-checker; falling back to the syntax leg`. The row is the syntax leg run with `--witness --project-root`; on rust-analyzer that alone reads 90.06 against 66.38 for the plain syntax run (the project root unlocks the workspace module plane), on rspack, CodexPlusPlus and clippy the two rows are byte-equal. A `--features cli,ts-checker,rust-checker` build was started in the lane (cold, ra_ap crates) and did not land inside the lane's window; rerun `run.py tuning --lang rust` and `run.py heldout --lang rust` with that binary to replace these rows.

### 4.5 Cache and reuse notes

- `--indexer rust` scopes its index to `<root>/.dl/.state/indexer-rust/`; rust-analyzer's existing `index.scip` (Aug 29, same sha `af4111f0bf85`) was hard-linked there so the control did not re-index.
- `HELDOUT_KEEP_WORK=1` keeps `/tmp/heldout-checkouts/<repo>` and `/tmp/heldout-work/<repo>/{oracle.jsonl,ours.<tier>.jsonl,files.txt}`; a scoring change re-reads them and a rerun skips the clone.
- The TypeScript-5.9 `--family scip` dump is a 3.1 GB jsonl per run; the index itself is 1.1 GB.
