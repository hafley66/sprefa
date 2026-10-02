# Engine parity report: ir:dd vs the Rust evaluator (2026-10-02, main 9d93cb706 + sqlite_ivm b00497c)

User (2026-10-02): no Rust-only engine; ir:dd matched 1-1 to the Rust evaluator, both behind the same
trait, completely traited; check with ryi; report before work.

## 1. Tests: full suite under `DL8_ENGINE=ir:dd`

`cargo test --workspace --no-fail-fast -- --skip _7_book --test-threads=1`: 148 passed, 14 failed,
367 s (default Rust engine: 162 passed, 224 s).

| group | tests | first error |
|---|---|---|
| harness pins Rust | `_2_eval_oracles::random_programs_match_all_evaluators`, `random_seed_retractions_match_fresh_dd_and_rust` | `Rust path requires DL8_ENGINE unset` (`_2_eval_oracles.rs:946`, `:1106`) |
| floats | `_3_eval_fixtures::every_literal_fixture_matches_expected`, `every_term_lt_fixture_matches_expected`, `_4_sqlite::reopening_the_db_leaves_the_closure_identical` | `unsupported_construct(float, _9a_ir_lower.rs:1946)`; `dl8 eval` prints `program is not an object` |
| effects at comptime | `_5_store_host::a_once_executor_answers_its_effect_row_at_comptime`, `answered_rows_feed_a_rule_that_mints_an_edge` | count `Some(2)` vs expected |
| runtime host | `a_second_run_against_the_db_answers_nothing_and_inserts_nothing` | second run ticks `1` |
| runtime host | `timer_fires_once_per_tick_and_the_count_reads_every_fire`, `timer_numbering_continues_past_the_ticks_in_the_db` | `run no JSON (trailing characters at line 2 column 1)` |
| runtime host | `git_refs_answers_the_snapshot_then_the_ref_that_moved`, `org_program_reads_the_head_of_each_required_repository` | `dl8 run exceeded 10s` |
| runtime host | `git_refs_on_a_directory_that_is_no_repository_is_an_error_row` | errors `0` |
| rxjs emit | `_9a_ir_emit::emitted_rxjs_keeps_source_outputs_and_relation_arguments` | `1_relation_argument: Path rows` empty |

Engine coverage of tests: the full suite runs on the default engine only, except tests with their own
engine loop (`_9_ladder` ENGINES, `_9b`-`_9h`, `_2_eval_oracles`). Files with zero engine loops:
`_1_compile_oracles` (10 tests), `_6_imports` (8), `_0_read_lower_check` (5), `_8_contracts` (3),
`_7_book` (24, skipped).

## 2. Kernel ops: three implementations

| engine | how a kernel goal runs | file |
|---|---|---|
| Rust | `kernel::solve` (shared Rust kernel) | `_5_evaluate.rs:241` |
| dd (direct) | same `kernel::solve` / `*_row` functions | `_9_dd_eval.rs:551,703` |
| ir:dd / ir:sqlite | each `Kernel` variant lowered to an `ivm_ir` op; ivm-dd and ivm-sqlite each re-implement it | `_9a_ir_lower.rs:409-520`; `ivm-dd/src/2_dd.rs`, `ivm-sqlite/src/3_nodes.rs` |

ryii (`ryii graph --from <fn> src`, syntax resolve): `_4_kernel.rs` functions reached from
`evaluate_dd` 26, from `evaluate_ir` 6 (`Int`, `Str`, `kernel_ref`, `linear`, `of`, `semantic`).
So every kernel op exists as: Rust `_4_kernel.rs`, ivm-dd operator, ivm-sqlite SQL. The string split
divergence (fixed in sqlite_ivm b00497c) came from that third copy.

## 3. Traits

- `ivm_engine::Engine` (`sqlite_ivm/crates/ivm-engine/src/1_rel.rs:90-127`): install, settle,
  snapshot, intern_text/text, get/mint/str_cons/str_op/mfp/union/negate/join/antijoin/reduce/
  threshold/topk/window. Implemented by `ivm_dd::Dd`, `ivm_sqlite::Sqlite`.
- dl8 does not dispatch through it: `_5_evaluate.rs:933` reads `DL8_ENGINE` into an enum, `:966`
  matches five arms to free functions (`evaluate_into`, `evaluate_sqlite`, `evaluate_dd`,
  `evaluate_ir`).
- Inside `evaluate_ir`, concrete types are named: `_9b_ir_eval.rs:96-97` (fields per backend),
  `:1188-1319` (`run::<ivm_dd::Dd>` / `run::<ivm_sqlite::Sqlite>` pairs).

## 4. Strings

| place | storage | cost |
|---|---|---|
| dl8 `Universe` `_0_term.rs:29` | `IndexSet<String>`, identifiers and text values together | one heap String per value, never freed |
| ivm `Interner` `1_terms.rs` | `by_text: BTreeMap<String,Cell>` + `by_id` text copy | each string stored twice; O(log n) string compares |
| ivm-sqlite | `ivm_term_dict.text` + unique index | copy in table + index |
| `doc.join` (`std/doc.dl7:25`) | fold of `str.cons` | O(n^2) bytes copied, every prefix kept |

Direction: `lasso` interner (one arena, `Spur` keys), byte spans over source blobs instead of copies,
output as a minted Doc tree rendered once by `pretty` at the `fs.file` sink (Biome `FormatElement`
/ Prettier Doc model). No `lasso`/`bytes`/`ropey` dependency exists today.

## 5. Recursion

37 self-calling functions in dl8 `src/` (ryii syntax resolve, lower bound):
`2026-10-02-two-engines.recursion.txt`. ivm-rxjs `src/1_runtime.ts:105` `mintText` still splits
recursively.

## Proposed order (fresh agent each, sequential)

1. ir:dd parity: the 14 failures to zero, Rust results as the reference; floats in IR.
2. One trait: dl8 dispatches only through `ivm_engine::Engine`; no concrete backend type outside
   one constructor; `DL8_ENGINE` = `ir:dd` | `ir:sqlite`.
3. Delete `evaluate_into`, `_9_dd_eval.rs`, `_7_sqlite_eval.rs`; frozen goldens remain the oracle;
   kernel ops exist once per backend.
4. Strings: lasso + byte spans + Doc-tree output.
5. Kernel recursion purge.
