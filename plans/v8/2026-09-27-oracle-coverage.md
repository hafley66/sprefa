# Oracle construct coverage

The evaluation column names `tests/it/_9_ladder.rs` rungs. A size is the input cardinality exercised by the rung. `all` means `rust`, `sqlite`, `dd`, `ir:dd`, and `ir:sqlite`. Rung 11 varies program seed count and compares full-size closure rows to byte-preserved frozen expectations under `fixtures/ladder/`. `kept` means the oracle fixture and its test remain because the observation is outside a sized evaluation case.

| Oracle fixture | Constructs observed | Ladder rung, size, engines or disposition |
|---|---|---|
| `eval/0_transitive.json` | linear transitive closure and cycle | rung 5, 0..3, all; nonlinear recursion also rung 5, 0..3, all |
| `eval/1_negation_cons_intern.json` | stratified negation with nil, cons, intern | rung 11, seed counts 0, 1, 3, all; `fixtures/ladder/` input |
| `eval/2_strict_cycle.json` | negative recursion diagnostic | kept: `_2_eval_oracles::every_oracle_fixture_matches_v7` and engine parity tests |
| `eval/3_count.json` | grouped count feeding `int.gt` | rung 11, seed counts 0, 1, 3, all; `fixtures/ladder/` input |
| `eval/4_cons_lists.json` | cons build and deconstruction, recursive suffix, improper lists | rung 11, seed counts 0, 1, all; `fixtures/ladder/` input |
| `eval/5_int_compare.json` | `int.lt`, `eq`, `ge`, `ne`, `le`, negative comparison, noninteger input | rung 11, seed counts 0, 1, 4, all; `fixtures/ladder/` input |
| `eval/6_malformed_aggregate.json` | malformed aggregate head diagnostic | kept: `_2_eval_oracles::every_oracle_fixture_matches_v7` and engine parity tests |
| `eval/7_aggregate_cycle.json` | aggregate recursion diagnostic | kept: `_2_eval_oracles::every_oracle_fixture_matches_v7` and engine parity tests |
| `eval/8_mixed_lower_recursion.json` | recursive reachability with lower stratum negation and final antijoin | rung 11, seed counts 0, 1, 9, all; `fixtures/ladder/` input |
| `eval/9_edge_ref.json` | `edge_ref`, cons and intern with literal variable terms | rung 11, seed counts 0, 1, 3, all; `fixtures/ladder/` input |
| `eval/10_empty.json` | empty program and kernel closure | rung 0, size 0, all |
| `eval/11_nonground_negation.json` | negative goal preceding positive binder | rung 11, seed counts 0, 3, 5, all; `fixtures/ladder/` input |
| `eval/12_repeated_vars_and_arity.json` | repeated variable, three-way join, zero-arity fact, constant head | rung 11, seed counts 0, 1, 5, all; `fixtures/ladder/` input |
| `eval/13_deep_terms_order.json` | deep heterogeneous terms and deterministic ordering | rung 11, seed counts 0, 1, 15, all; `fixtures/ladder/` input |
| `eval/14_demand_recursion.json` | demand through constructors and recursive chain | rung 11, seed counts 0, 1, 6, all; `fixtures/ladder/` input |
| `eval/15_aggregate_tables_only.json` | count over kernel generated nil/cons values and grouped count with negative `int.eq` | rung 11, seed counts 0, 1, 3, all; `fixtures/ladder/` input |
| `eval/16_intern_row_reuse.json` | intern reuse across rules and readback with negation | rung 11, seed counts 0, 1, 2, all; `fixtures/ladder/` input |
| `eval/c0_minimal_0.json` | generated macrotime IR: long joins, graph terms, `:` and `node` kernels | kept: `_2_eval_oracles` whole-program parity tests (generated macrotime IR; catches silent row loss) |
| `eval/c14_syntax_macros_0.json` | same round-zero macrotime IR shape | kept: `_2_eval_oracles` whole-program parity tests (generated macrotime IR; catches silent row loss) |
| `eval/c14_syntax_macros_1.json` | macrotime IR with count, negation and constructor chains | kept: `_2_eval_oracles` whole-program parity tests (generated macrotime IR; catches silent row loss) |
| `eval/c15_standard_plus_0.json` | same round-zero macrotime IR shape | kept: `_2_eval_oracles` whole-program parity tests (generated macrotime IR; catches silent row loss) |
| `eval/c15_standard_plus_1.json` | intermediate macrotime IR with graph joins | kept: `_2_eval_oracles` whole-program parity tests (generated macrotime IR; catches silent row loss) |
| `eval/c15_standard_plus_2.json` | application terms and graph joins | kept: `_2_eval_oracles` whole-program parity tests (generated macrotime IR; catches silent row loss) |
| `eval/c15_standard_plus_3.json` | macrotime IR with count, negation and constructor chains | kept: `_2_eval_oracles` whole-program parity tests (generated macrotime IR; catches silent row loss) |
| `eval/c2_partial_0.json` | same round-zero macrotime IR shape | kept: `_2_eval_oracles` whole-program parity tests (generated macrotime IR; catches silent row loss) |
| `eval/c2_partial_1.json` | prelude IR with `edge_snapshot`, `intern_snapshot`, constructors and graph joins | kept: `_2_eval_oracles` whole-program parity tests (generated macrotime IR; catches silent row loss) |
| `eval/c2_partial_2.json` | same prelude IR with application terms | kept: `_2_eval_oracles` whole-program parity tests (generated macrotime IR; catches silent row loss) |
| `eval/c2_partial_3.json` | same prelude IR with generated binding terms | kept: `_2_eval_oracles` whole-program parity tests (generated macrotime IR; catches silent row loss) |
| `eval/c2_partial_4.json` | same prelude IR after another comptime round | kept: `_2_eval_oracles` whole-program parity tests (generated macrotime IR; catches silent row loss) |
| `eval/c2_partial_5.json` | same prelude IR after another comptime round | kept: `_2_eval_oracles` whole-program parity tests (generated macrotime IR; catches silent row loss) |
| `eval/c2_partial_6.json` | same prelude IR after another comptime round | kept: `_2_eval_oracles` whole-program parity tests (generated macrotime IR; catches silent row loss) |
| `eval/c2_partial_7.json` | same prelude IR at fixpoint | kept: `_2_eval_oracles` whole-program parity tests (generated macrotime IR; catches silent row loss) |

| Oracle directory | Output that requires its existing test | Disposition |
|---|---|---|
| `read/` | tokens, spans, source form order, reader errors | kept: `_0_read_lower_check::every_oracle_fixture_matches_v7` |
| `macrotime/` | expansion rows, caret and list rewrite, macro diagnostics | kept: `_0_read_lower_check::every_macrotime_case_matches_v7` |
| `lower/` | lowered graph, deferred calls and binding diagnostics | kept: `_0_read_lower_check::every_lowering_case_matches_v7` |
| `check/` | checked rules, type and stratification diagnostics | kept: `_0_read_lower_check::every_checker_case_matches_v7` and diagnostic status test; `check/cases/` is also read by `_7_book` |
| `load/` | accepted project and TSI rows, loader errors | kept: `_1_compile_oracles::every_loader_case_matches_v7` and byte input test |
| `comptime/` | rounds, generated program, validation and diagnostics | kept: `_1_compile_oracles::every_comptime_case_matches_v7` and round count test; rung 9 also exercises stable round 1 on all engines |
| `reify/` | reified rule and call rows, diagnostics | kept: `_1_compile_oracles::every_reify_case_matches_v7` and status tests |
| `compile/cases/*.json` | compile rows, names entries, diagnostics and exit code | kept: `_1_compile_oracles::every_committed_case_matches_v7` and index test |
| `compile/sources/` | sources for the compile cases and other phase cases | kept with compile cases |

The current eval oracle inputs have no `fold`, effect request, or window rule. The ladder exercises effects at rung 8 with sizes 0..3 on all engines. Rung 7 checks declared folds at size 3 on `rust`, `ir:dd`, and `ir:sqlite`, and frontier growth at sizes 8 and 80 on both IR engines. Its `delta_rows.window` counter is the window observation. Comptime replay is exercised by rung 9 on all engines; compile-time effect answer and retained IR installation are also checked there.

`_2_eval_oracles` reads every `eval/*.json` fixture with four parity tests covering all five engines. It also has generated differential and retraction tests independent of `oracle/`. `_7_book` reads `check/cases/` through fixture markers. No oracle expected output is edited by this map.
