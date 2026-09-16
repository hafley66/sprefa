# DL7 alias and performance finish

Status: alias patch reproduced and validated; one bounded performance arc applied
(syntax-expander one-pass row append). Uncommitted, no commit per instruction.
Base sha: `b900b80f2`.

## TOC

1. [Scope](#scope)
2. [Files](#files)
3. [Alias corrections vs the source patch](#alias-corrections-vs-the-source-patch)
4. [Performance arc: expander one-pass row append](#performance-arc-expander-one-pass-row-append)
5. [Before and after per phase](#before-and-after-per-phase)
6. [Cache and table lifetime and reset](#cache-and-table-lifetime-and-reset)
7. [Exact validation](#exact-validation)
8. [Goal status](#goal-status)
9. [Checkpoint decisions](#checkpoint-decisions)
10. [Remaining blocker](#remaining-blocker)
11. [Hail fields](#hail-fields)

## Scope

Reproduce the justified changes from the sibling dirty diff, apply the required
corrections, then run one performance arc aimed at the two goals (2_partial cold
under 88,000,000 inferences, every lexical fixture cold under 3 seconds).

| file | change |
|---|---|
| `v7/src/0_reader/1a_syntax_grapher.pl` | keyed `source_row_index/2` thread-local replaces the per-node `SourceRows` scan |
| `v7/src/0_reader/1_expander.pl` | `append_rows/3` one-pass fast path replaces the per-node `AvailableRows ++ GeneratedRows` traversal |
| `v7/src/2_comptime/0_lowerer.pl` | atom target under a compound label reads the nearest deferred binding; WIP atom special-case removed |
| `v7/test/19_lexical_binding.test.pl` | adds the nearest-shadow regression test |
| `v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7` | new fixture |
| `v7/bench/0_compiler_performance.pl` | row and closure checkpoints; cold budget restored to 88,000,000 |
| `v7/AGENTS.md` | new, V7-local gates only |

## Files

```text
 M v7/bench/0_compiler_performance.pl
 M v7/src/0_reader/1_expander.pl
 M v7/src/0_reader/1a_syntax_grapher.pl
 M v7/src/2_comptime/0_lowerer.pl
 M v7/test/19_lexical_binding.test.pl
?? v7/AGENTS.md
?? v7/receipts/7_dl7_alias_performance_finish.md
?? v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7
```

`git diff --check` exits 0.

## Alias corrections vs the source patch

| # | requirement | source patch | this worktree |
|---|---|---|---|
| 1 | alias semantics + nearest-shadow regression | present | preserved |
| 2 | keep the reader index only if reader/syntax outputs identical | present | kept; exact-output assertions pass |
| 3 | clear before install and at the `setup_call_cleanup/3` boundary | install only asserts | `install_source_row_index/1` clears then asserts; cleanup clears |
| 4 | cold budget stays 88,000,000 | raised to 95,000,000 | restored to 88,000,000 |
| 5 | per-body test-19 timing | not measured | measured; bodies <= 8 ms |
| 6 | 20 s command cap | n/a | honored for every foreground command |
| 7 | `v7/AGENTS.md` V7-specific gates only | mixes reusable SWI technique | trimmed to V7 gates |

## Performance arc: expander one-pass row append

Profiling (`profile/1` around `expand_dl7/6` on the 1,112-line type prelude)
named the repeated predicate before any edit:

| predicate | calls | self | children |
|---|---:|---:|---:|
| `lists:append/3` | 23,887 | 1.34 s | 1.54 s |
| `dl7_expander:dl7_syntax_rewrite/3` | 3,981 | 0.17 s | 0 s |
| `$garbage_collect/1` | 5 | 1.54 s | 0 s |

`dl7_syntax_rewrite/3` had 0 exits on the prelude, so no rewrite generated rows.
The cost was `expand_dl7` threading `AvailableRows` (the whole reader row set) to
every node and running `append(AvailableRows, [], _)` per node, which traverses
the full row list each time (O(nodes x rows); the recursion inside `append/3`
accounts for the 31.8M inferences).

The fix adds `append_rows/3` in `v7/src/0_reader/1_expander.pl` and uses it at
the four `Base ++ Extra` sites:

```prolog
append_rows(Base, Extra, Combined) :-
    (   Extra == []
    ->  Combined = Base
    ;   append(Base, Extra, Combined)
    ).
```

`Extra == []` is the identity and skips the traversal; a non-empty `Extra` (a
real rewrite) appends in the same order as before. No `findall` is introduced,
variable identity is untouched, and no fallback for a non-ground key applies
(all sets here are proper ground lists).

Measured stage effect (`probe_prelude.pl`, prelude expand only):

| stage | before | after |
|---|---:|---:|
| `read_dl7` | 31 ms / 347,917 | 31 ms / 347,917 |
| `reify_syntax` | 14 ms / 130,068 | 14 ms / 130,068 |
| `expand_dl7` | 2,470 ms / 31,785,958 | 19 ms / 89,236 |

## Before and after per phase

`DL7_TRACE=steps` phase summary. Inferences unchanged in every phase except
`expand`; the compiled program is unchanged.

`0_nearest_and_chains.dl7`:

| run | read | expand | lower | check | comptime | total |
|---|---:|---:|---:|---:|---:|---:|
| before | 3 / 3,325 | 3,224 ms / 32,280,931 | 1 / 11,002 | 5 / 41,040 | 6,237 ms / 33,100,942 | 11,760 ms / 67,316,255 |
| after | 3 / 3,325 | 68 ms / 577,247 | 1 / 11,002 | 4 / 41,040 | 6,250 ms / 33,100,942 | 6,555 ms / 35,244,489 |

`6_deferred_alias_measure.dl7`:

| run | expand | comptime | total |
|---|---:|---:|---:|
| before | 2,851 ms / 32,273,737 | 6,297 ms / 32,811,590 | 11,007 ms / 67,022,399 |
| after | 68 ms / 573,973 | 6,303 ms / 32,811,590 | 6,623 ms / 34,954,553 |

`2_partial.dl7` (bench, cold):

| run | expand | comptime | cold inferences | rows | rounds |
|---|---:|---:|---:|---:|---:|
| before | 2,647 ms / 32,364,744 | 39,122 ms / 57,365,156 | 91,732,265 | 15,542 | 8 |
| after | 67 ms / 594,294 | 32,388 ms / 57,365,156 | 59,593,733 | 15,542 | 8 |

The `comptime` inferences and the 15,542 rows are identical before and after, so
the arc changed only the expand phase. Warm `2_partial` is 2,343 inferences.

Per-fixture cold wall after the arc (fresh process, `probe_one.pl`):

| fixture | wall ms | inferences | rows | evaluate rounds |
|---|---:|---:|---:|---:|
| `0_nearest_and_chains` | 6,560 | 35,245,324 | 14,515 | 2 |
| `5_generated_callable` | 9,172 | 40,024,263 | 14,867 | 3 |
| `6_deferred_alias_measure` | 6,628 | 34,955,388 | 14,602 | 2 |
| `7_nearest_shadow` | 6,399 | 34,671,190 | 14,586 | 2 |

## Cache and table lifetime and reset

| unit | kind | owner | lifetime | reset |
|---|---|---|---|---|
| `source_row_index/2` | thread-local dynamic | `reify_syntax/4` | one reify call | `clear_source_row_index/0` before install and in the `setup_call_cleanup/3` cleanup |
| `append_rows/3` | pure, stateless | `expand_dl7/6` | none | none (no state added) |
| `cached_prelude/3`, `cached_compilation/3` | process-local dynamic | `dl7_compiler_cacher` | process | `clear_compiler_caches/0` |
| `dl7_evaluator:proves/2` | scoped SLG table | `dl7_evaluator` | one stratum evaluation | `setup_call_cleanup/3` plus `abolish_table_subgoals(proves(EvaluationId,_))` in `clear_evaluation/2` |

The arc adds no cache or table. The reader index remains torn down at the reify
boundary. The evaluator table lifetime is unchanged.

## Exact validation

- `timeout 20 swipl -q -s v7/test/0_reader.test.pl -g run_tests -t halt`: 12/12, exit 0.
- `timeout 20 swipl -q -s v7/test/1a_syntax_expander.test.pl -g run_tests -t halt`: 4/4, exit 0, including the exact `macro_result/13` snapshot and `Expanded == Rows`.
- `6_deferred_alias_measure.dl7` compiled rows dumped before and after the arc: 14,602 lines, `diff` clean (`sha256` equal).
- Test-19 bodies, invoked through plunit `current_test/5` against precompiled fixtures: 10/10 pass with their exact edge assertions.

| test | ms | inferences |
|---|---:|---:|
| `nearest_non_callable_blocks_outer_callable` | 0 | 21 |
| `same_owner_product_precedence_is_unchanged` | 0 | 15 |
| `promoted_alias_uses_nearest_owner_index_and_alias_origins` | 0 | 389 |
| `nearest_ordinary_binding_controls_atom_and_argument_targets` | 8 | 16,208 |
| `missing_name_uses_existing_checker_diagnostic` | 0 | 12 |
| `self_cycle_terminates_with_existing_checker_diagnostic` | 0 | 12 |
| `two_name_cycle_terminates_with_existing_checker_diagnostics` | 0 | 12 |
| `generated_callable_keeps_final_application_identity` | 2 | 16,415 |
| `deferred_expression_aliases_reuse_final_identity` | 8 | 15,994 |
| `nearest_non_deferred_shadow_blocks_deferred_alias_promotion` | 8 | 15,873 |

Shared setup (`compile_fixtures/0`, 7 fixtures) is 42,623 ms; the whole file
cannot run under the 20 s foreground cap, so setup and bodies were measured in
separate bounded runs.

## Goal status

| goal | result |
|---|---|
| 2_partial cold under 88,000,000 inferences | met: 59,593,733 (was 91,732,265), rows 15,542 unchanged, rounds 8 unchanged, diagnostics clean |
| every lexical fixture cold under 3 seconds | not met: `0` 6.6 s, `5` 9.2 s, `6` 6.6 s, `7` 6.4 s; `2`, `3`, `4` are under 3 s |

The expand arc removed the expand phase from every fixture. The floor that
remains is the stratified comptime closure.

## Checkpoint decisions

The row and closure checkpoints moved from the committed `12716`/`7` to
`15542`/`8` because the alias feature changes the compiled program of
`2_partial.dl7`, measured:

| checkpoint | committed | final | measured reason |
|---|---:|---:|---|
| compiler rows | 12716 | 15542 | pre-patch HEAD compiles `2_partial.dl7` to 0 rows with `unsafe_head_var(derived_edge_target(reader_node(path,90)))` and `unsafe_head_var(derived_edge_target(reader_node(path,101)))`; patched yields 15,542 rows, 0 diagnostics |
| closure rounds | 7 | 8 | pre-patch compile aborts before a complete fixpoint; patched runs 8 rounds |

Root cause: `b900b80f2` added a `node(_, atom(_))` special-case to
`compound_bind_target_result/4`, which made a compound-label atom target such as
`(: (Key "account" PrimaryKeyOptions) int)` a deferred expression. The patch
removes it, so an atom target reaches `lower_target/4`, matching the atom-label
path at `lower_bind/5`. The new `compound_edge_target/5` clause handles the read
side and lets a nearer non-deferred binding win.

## Remaining blocker

The stratified comptime fixpoint owns the lexical-fixture floor. `/usr/bin/time`
is not needed to see it: the `evaluate_round` steps are 2.4 to 2.5 s each and
fixtures run 2 or 3 rounds (`5_generated_callable` needs a third round for its
generated relation and rule). The closure is produced by the scoped `proves/2`
SLG table in `dl7_evaluator`; the time is engine table work, not a list scan that
an index can remove.

Getting under 3 s requires reusing or restructuring the evaluation itself
(round reuse, table strategy, or fewer rounds), which changes evaluation
semantics. Per the instruction, no such change was made. The safe, non-semantic
candidates left are smaller than the gap: group-and-adjacent-scan in
`validate_functional_rows/3` (about 1.1 s) and a combined key on
`evaluation_lower/3` (about 0.3 to 0.5 s per round). Neither reaches 3 s.

## Hail fields

- status: alias patch validated plus one performance arc; uncommitted; reader 12/12, syntax 4/4, test-19 10/10
- sha: `b900b80f2` (base; no commit created)
- files: `1a_syntax_grapher.pl`, `1_expander.pl`, `0_lowerer.pl`, `0_compiler_performance.pl`, `19_lexical_binding.test.pl`, `7_nearest_shadow.dl7` (new), `v7/AGENTS.md` (new)
- reader_validation: 12/12 pass; exact reader/syntax output unchanged
- syntax_validation: 4/4 pass; exact snapshot and `Expanded == Rows`
- test19_timings: bodies max 8 ms; shared setup 42,623 ms
- inference_budget: 88,000,000 kept; `2_partial` cold 59,593,733 (was 91,732,265); warm 2,343
- row_checkpoint_reason: pre-patch 0 rows + `unsafe_head_var`; patched 15,542 rows clean; alias atom-target fix
- closure_checkpoint_reason: patched compile runs 8 rounds; pre-patch aborts before the fixpoint completes
- remaining_blocker: stratified comptime `evaluate_round` (2.4 to 2.5 s, 2 or 3 rounds) keeps `0`, `5`, `6`, `7` above 3 s; reducing it changes evaluation semantics, so stopped
- next: either approve an evaluation change (round reuse or table scope) or accept the floor and split the 42.6 s test-19 shared setup
