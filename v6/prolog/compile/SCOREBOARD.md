# TSV2 Phase C scoreboard

Sweep of `v6/prolog/compile/` over every fixture in
`v6/prolog/conformance/fixtures/*.pl`, per
`plans/2026-07-27-tsv2-compile-target-header.md`'s PHASE C CONTRACT. Driver:
`v6/tsv2/scripts/sweep.sh` (compile every fixture -> `v6/prolog/compile/out/`,
run the oracle over the same fixtures, run every compiled program on the
phase-A runtime, diff tick logs byte-for-byte).

Regenerate: `cd v6/tsv2 && bash scripts/sweep.sh`. Raw data:
`v6/prolog/compile/out/manifest.json` (compile bucket + refusal reason per
fixture) and `v6/prolog/compile/out/run-results.json` (run bucket + diff
excerpt per compiled fixture).

## Totals (current)

| bucket | count |
|---|---|
| fixtures swept | 109 |
| UNSUPPORTED (compiler refuses, named construct) | 79 |
| compiled (lowering + emission succeeded) | 30 |
| — of which IDENTICAL (tick log byte-identical to oracle) | 27 |
| — of which WRONG (diff, crash, or silent gap vs oracle) | 3 |

IDENTICAL + WRONG + UNSUPPORTED = 27 + 3 + 79 = 109.

Two of the 27 IDENTICAL rows are **vacuous passes**, not verified correctness
— see Findings 1 and 2. Read as "27 nominal, 25 genuinely exercised and
correct."

**PHASE C2 RULING 1 (typed columns) landed** (commit `tsv2 C2a`): the 5
int-vs-string WRONGs Finding 3 documented are now IDENTICAL — see the
widening history's entry 4 and Finding 3's resolution note below.

**PHASE C2 RULING 2 (unmarked edge triggers) landed** (commit `tsv2 C2b`):
13 more fixtures went UNSUPPORTED -> IDENTICAL (compiled 17 -> 30, identical
14 -> 27, WRONG stays 3 — all three pre-existing and out of this ruling's
scope, unchanged from Ruling 1's landing). unsupported 92 -> 79. See the
widening history's entry 5 for the mechanism, the two stop-and-report sites
this widening surfaced (both left refused, not hacked around), and the
`edge_marked_with_extra_goal`/`level_body_goal` bucket count changes (more
fixtures now REACH those checks since their earlier-in-program unmarked
edge rule no longer blocks them first — not new gaps, existing ones now
visible under a precise name instead of a blanket `edge_body_shape`).

## Per-fixture table: compiled (30)

| fixture | file | run bucket | cause |
|---|---|---|---|
| switch_as_keyed_replace | scopes.pl | IDENTICAL | phase A/B exemplar, independently reconciled |
| demand_laziness_effect_rows | scopes.pl | IDENTICAL | phase A/B exemplar, independently reconciled |
| fill_as_cache_update_swr | scopes.pl | IDENTICAL | |
| shared_demand_refcount | scopes.pl | IDENTICAL | |
| zombie_scope_negative_case_a2b | scopes.pl | IDENTICAL | needed the `declared_refs` keyed/keep fix (Finding 4) |
| retraction_only_tick_retracts_level_view | engine_core.pl | IDENTICAL | |
| fork_join_is_a_conjunctive_body | operators.pl | IDENTICAL | |
| retention_count_prunes_oldest | engine_core.pl | IDENTICAL (deltas-blind) | see Finding 1 — retention is not lowered at all; the gap is invisible to a delta-trace diff |
| braces_in_head_position | json_arm.pl | IDENTICAL (vacuous) | see Finding 2 — empty Schedule, zero oracle lines, zero actual lines |
| log_deltas_follow_arrival_order | occurrence_identity.pl | IDENTICAL | PHASE C2 RULING 1 (typed columns) — `line/3`'s `stream_id`-equivalent column is now INTEGER; was WRONG (`"1"` vs `1`), see Finding 3 |
| shuffled_arrival_reorders_log_deltas | occurrence_identity.pl | IDENTICAL | same fix as above |
| level_view_reads_set_projection_not_occurrences | occurrence_identity.pl | IDENTICAL | same fix as above |
| terminal_is_terminal | shell_stream.pl | IDENTICAL | same fix as above (multi-clause-per-head union already correct — Finding 4b — the residual diff was purely the integer-representation gap, now closed) |
| live_nonzero_exit_keeps_rows | shell_stream.pl | IDENTICAL | same fix as above |
| unmarked_edge_replays_backlog | engine_core.pl | IDENTICAL | PHASE C2 RULING 2 flagship case: 2-atom unmarked body (`change_ev(Item), subscriber(Client)`), each atom its own trigger arm joined against the other's current table — see Finding 7's resolution and the widening history entry 5 |
| set_dedups_log_stacks | engine_core.pl | IDENTICAL | unmarked single-atom trigger on a SET rel (`seen/1`) — exercises `triggerOccurrences`'s dedup branch (a duplicate same-tick `+seen(alpha)` mints only ONE occurrence, matching `heard/1`'s Log-rel two occurrences in the same fixture) |
| merge_batches_per_tick | merge_family.pl | IDENTICAL | two unmarked single-atom edge rules sharing one Log head (`out/1`); needed the global per-arm Index naming fix (same-head naming collision) |
| merge_never_retracts | merge_family.pl | IDENTICAL | same shape as above |
| merge_policy | scopes.pl | IDENTICAL | marked_single trigger writing into a Log head (`closed/2`) — the SCOREBOARD's own former 1-fixture "edge writes into a Log-kind head" bucket, now closed by the Log-head write generalization (`edge_statement_single`'s HeadKind branch) |
| key_last_write_wins | merge_family.pl | IDENTICAL | two unmarked single-atom edge rules, DIFFERENT triggers (`from_poll`/`from_push`), sharing a keyed Set head — no conflict risk (disjoint trigger refs) |
| key_identical_write_is_silent | merge_family.pl | IDENTICAL | same shape, equal-row no-op case |
| key_same_tick_ordered_not_conflict | merge_family.pl | IDENTICAL | same shape, both triggers arrive in ONE tick — grouped-by-rule concatenation happens to preserve arrival order here (rule order matches schedule order in this fixture; a stop-and-report risk if it hadn't, see Finding 7's residual note) |
| new_salt_refires_fresh_stream | shell_stream.pl | IDENTICAL | single-rule unmarked single-atom trigger |
| worktree_edit_replaces_digest_and_flips_kind_view | spine_semantics.pl | IDENTICAL | unmarked single-atom edge rule + a level rule reading its output |
| worktree_edit_identical_resave_is_silent | spine_semantics.pl | IDENTICAL | same shape, equal-row no-op |
| head_move_replaces_key | spine_semantics.pl | IDENTICAL | unmarked single-atom edge rule, keyed replace |
| head_move_flips_current_tree_in_one_tick | spine_semantics.pl | IDENTICAL | needed the boot-time level-closure fix (Finding 8) — the first compiled fixture with BOTH non-empty Initial data and a level rule reading it |
| fork_join_error_arm_is_a_value | operators.pl | WRONG (crash: `SQLITE_ERROR: malformed JSON`) | compound arrival VALUE stored as canonical text, read back via `json_extract` which expects json1 — Finding 5 |
| log_retraction_rejected | engine_core.pl | WRONG (crash, matches oracle's *intent*) | oracle throws `retract_from_log`; emitted program also throws (a JS `Error`, not a byte-comparable log) — Finding 6 |
| log_without_retention_rejected | engine_core.pl | WRONG (silent gap) | oracle throws `missing_retention`; emitted program has no such validation and runs "successfully" — Finding 6 |

Four fixtures reached compilation briefly during this widening, came out
WRONG, and are now refused at compile time by a new named check rather than
left silently wrong (not in the 30 above): `pin_to_unknown_repo_derives_repo_candidate`
and `xref_rev_is_pin_data_not_live_head` (spine_semantics.pl,
`edge_head_column_type_mismatch` — Finding 9), `edge_chain_hops_tick_per_stage`
(`edge_trigger_is_derived` — Finding 7's residual, a genuine runtime-generality
gap), and `one_occurrence_two_rows_still_conflicts` (`edge_head_conflict_risk`
— Finding 10). Three distinct stop-and-report findings across these four
fixtures.

## Per-construct blocked tally (UNSUPPORTED, ranked) — post PHASE C2 RULING 2

| construct | fixtures blocked | example |
|---|---:|---|
| edge rule with `only(trigger)` **plus an extra guard goal** (`not/1`, `pre/1`, `now/1`, `decode/2`, `stars_of/2`... mixed in) | 21 | `only(demand_row(...)), decode(Response, fresh(_, Body)), stars_of(Body, Stars)` |
| **comparison operators** in a level body (`<`, `=<`, `>`, `>=`, `==`, `\==`) | 14 | `Union > 0, Shared*100/Union >= 40` |
| **aggregate head** (`count`, `sum`, `min`, `max`, `json_array`, `json_object` as GROUP BY) | 9 | `hits(Repo, count(Item))` |
| unmarked edge trigger needing `pre/1` (a same-tick fold chain) | 8 | `increment(Name, _), pre(counter(Name, Total)), Next := Total + 1` |
| **arithmetic bind** (`:=` / `is`) in a level body | 5 | `Sum := Base + Extra, Sum > 10` |
| unmarked edge trigger needing `now/1` (the current tick number) | 5 | `worktree_edit(Path, Digest), now(Tick)` |
| **JSON destructuring** (`decode/2`, `json_each/2`) in a level body | 4 | `decode(Doc, {name: Name})` |
| unmarked edge trigger referencing a **derived** (edge- or level-headed) ref | 2 | `stage_two(Item) <+ stage_one(Item)`, `stage_one` itself edge-headed |
| unmarked edge trigger needing `departed/1` | 2 | `departed(latest(Key, OldValue))` |
| **arithmetic in a rule head** (`+`,`-`,`*`,`/`,`mod` as a head argument, not evaluated) | 2 | `jaccard(L, R, Shared*100/Union)` |
| edge-derived head column **type mismatch** (value flows from an int-typed source column into a text-typed head column) | 2 | `xref(FromSpanId, ...) <+ pin_extracted(FromSpanId, ...)`, `xref`'s own column never sees a literal int |
| `keyed(Ref, _)` on a **Log rel** | 1 | `keyed(latest/2, [1])` where `latest/2` is `kind(latest/2, log)` |
| edge into an **unkeyed Set head** | 1 | `sink(Item) <+ ping(Item)`, `sink/1` an unkeyed Set |
| unmarked edge trigger needing `not/1` | 1 | `increment(Name, _), not(pre(counter(Name, _)))` |
| edge head **conflict risk** (two rules/arms sharing a keyed head AND a trigger ref) | 1 | `(latest(cli,a) <+ ping(_)), (latest(cli,b) <+ ping(_))` |
| unmarked edge trigger needing **JSON destructuring** | 1 | (a body combining an unmarked atom with `decode/2`) |

Total: 21+14+9+8+5+5+4+2+2+2+2+1+1+1+1+1 = 79.

**The 48-fixture unmarked-trigger bucket is CLOSED** (PHASE C2 RULING 2,
commit `tsv2 C2b`): every fixture in it was reclassified, either into
IDENTICAL (13 of them) or into one of the precise construct-specific
refusals above (a fixture co-blocked by ANOTHER rule's marked+extra-guard
or comparison/aggregate/etc shape, or genuinely needing `pre`/`now`/
`departed`/`not` support this ruling did not add). See the widening history
entry 5 and Finding 7's resolution.

## Widening / fix history (chronological, each transition measured)

1. **Baseline**: `sweep.pl` written, zero changes to `analyze.pl`/`lower.pl`/
   `emit_ts.pl`. `SWEEP total=105 compiled=16` — **4 fixtures silently
   vanished** (neither compiled nor named-unsupported): `sweep_one/5` itself
   failed as a bare Prolog goal rather than throwing, so `findall/3` just
   skipped them. Root cause: `emit_ts.pl:recompute_levels_fn_lines/2` had no
   clause for `LevelStatements == []` (a program with zero level rules,
   e.g. an EDB-only fixture with an empty `Rules` list), so `emit_program/5`
   failed outright with no error term.
2. **Fix, commit `tsv2 C: fixture-corpus sweep harness + 3 lowering gaps it
   surfaced`**: three lowering gaps the sweep surfaced immediately, none a
   new "construct":
   - `emit_ts.pl`: added the `LevelStatements == []` fallback (emits
     `of(undefined)` — the one-value-then-complete shape the async-becomes-
     rxjs law calls for) and the matching `DeltaStatements == []` fallback
     for `readSnapshot` (`forkJoin({})` completes without emitting, same
     hazard as the edge resolver's documented `forkJoin([])` guard).
   - `analyze.pl`: added `declared_refs/2` — a `kind(Ref, _)` declaration
     with **zero rule readers** (e.g. `engine_core.pl`'s
     `retention_count_prunes_oldest`: `kind(event/1, log), keep(event/1,
     count(2))`, no rules at all) was invisible to `program_refs/2` (which
     only walks `Rules`), so the rel got no DDL and no arrival handling.
   - `analyze.pl`: `declared_refs/2` further widened to also scan
     `keyed(Ref, _)` and `keep(Ref, _)` declarations, not just
     `kind(Ref, _)` — `scopes.pl`'s `zombie_scope_negative_case_a2b`
     declares `keyed(open_pane/2, [1])` with **no** `kind/2` and **no**
     rule reader at all (the fixture's own comment: "REJECTED READING
     dropped on purpose").
   - `lower.pl`/`emit_ts.pl`: **multiple level-rule clauses sharing one
     head ref** (`shell_stream.pl`'s `terminal_is_terminal`:
     `stream_status(Args, running) <- ...` and
     `stream_status(Args, done) <- ...`, standard datalog union-of-clauses)
     were each lowered to their own `DELETE FROM ...; INSERT ...` pair, so
     the second clause's `DELETE` silently wiped the first clause's just-
     inserted rows. `level_statement_group/3` now groups adjacent same-head
     rules into one `DELETE` + N `INSERT`s. (`levelstmt/3`'s third field
     changed shape from one SQL string to a list of them; `emit_ts.pl`,
     `test/plunit_tests.pl`, and `test/run_sql_check.pl` updated to match.
     The two phase A/B exemplar fixtures — one clause per head each — still
     emit byte-identical text, confirmed by re-diffing against
     `gen_emitted/*.ts`.)
   - `emit_ts.pl`: a `bindArgs` helper wraps every raw arrival/edge-
     projection value before it becomes `SqlStatement` args. Root cause
     (verified with a throwaway `open_db` call, not assumed):
     `@libsql/client` binds a JS `number` parameter as SQLite REAL, never
     INTEGER; a bound `1` lands as the TEXT value `"1.0"` in a TEXT-affinity
     column (every column here is `TEXT NOT NULL` — `lower.pl:column_def/2`),
     `1n` (bigint) lands as `"1"`. `bindArgs` converts any
     `Number.isInteger(value)` argument to `BigInt(value)` before binding.
   - Result at that commit: `SWEEP total=109 compiled=20`,
     `RUN total=20 identical=12 wrong=5 run_error=2 no_oracle_log=1`.
3. **Safety fix, this commit — NOT a widening**: `analyze.pl`'s
   supported-subset gate now refuses two shapes it previously accepted
   silently:
   - a **comparison operator** (`<`,`=<`,`>`,`>=`,`==`,`\==`) or a `:=`/`is`
     **bind** anywhere in a level rule body (`body_ref_uses/2` already
     returned zero `Uses` for both — nothing downstream ever compiled them
     into a WHERE clause or learned the bound variable; a filter condition
     was silently dropped rather than enforced).
   - **arithmetic as a head argument** (`+`,`-`,`*`,`/`,`mod`, e.g.
     `jaccard(L, R, Shared*100/Union)`) — `compile_head_expr/3` renders
     *any* compound head argument as a json1 "construct a tagged term"
     expression; correct for a genuine domain compound like
     `route_data(RouteId)`, silently wrong for an arithmetic expression
     (the stored value is a json1-encoded EXPRESSION TREE, never the
     computed number).
   - Before: `SWEEP compiled=20`, `RUN identical=12` (3 of those 12 —
     `comparison_filters_rows`, `range_join_over_arithmetic`,
     `head_expression_evaluates_derived_column` — were **false positives**:
     confirmed by hand that their emitted SQL is wrong, invisible to the
     diff only because all three fixtures also have an empty `Schedule`,
     so the tick-log comparison is vacuous — see Finding 2). After:
     `SWEEP compiled=17`, `unsupported=92` (up from 89), `RUN identical=9`.
   - Presented separately because it is the opposite move from the ones
     above: **fewer** fixtures compile after this commit, not more. It
     converts three previously-undetectable silent-WRONG results into
     clean, named refusals, matching the contract's "Refusal must be a
     clean named error, never wrong output."
4. **PHASE C2 RULING 1, commit `tsv2 C2a`: typed columns (flat compounds).**
   Every column was `TEXT NOT NULL` regardless of its Prolog-side type
   (`lower.pl:column_def/2`); Finding 3 named this as the cause of all 5
   remaining WRONGs and left it open pending a user ruling on how a column's
   type should be decided, since Decls carries no type syntax at all. The
   ruling: infer int-vs-text per column from the fixture's own concrete
   literal values, never from a new declaration. `analyze.pl:
   rel_column_types/5` (new) scans every literal, atomic argument observed
   at a given (Ref, Position) across three sources — rule head/body atom
   occurrences (`ref_occurrence_args/3`, already used for column naming),
   `Initial` seed rows, and `Schedule` arrivals (either sign) — and marks the
   column `int` only when EVERY literal witness found is a Prolog
   `integer/1`; `text` otherwise, including "zero witnesses at all" (a
   column reached only through variables or nested inside a compound
   argument, which is never `atomic/1` and so never contributes a witness —
   this is exactly how a compound-term column keeps the ruling's flat-punt
   without a special case). `relplan/4` widened to `relplan/5` (`RelPlans`
   entries now carry a `ColumnTypes` list parallel to `Columns`);
   `lower.pl:column_def/3` emits `INTEGER NOT NULL` for `int`, `TEXT NOT
   NULL` unchanged for `text`. No change was needed on the TypeScript side:
   `emit_ts.pl`'s existing `bindArgs` helper (bigint-converts any
   integer-valued `IRowValue` before binding, the earlier REAL-vs-INTEGER
   fix) already produces correct results against BOTH column types —
   verified empirically with a throwaway `open_db` probe before relying on
   it, not assumed: binding a JS `bigint` or a plain JS `number` into an
   `INTEGER` column both land as SQLite `integer` storage and read back as a
   plain JS `number` (this driver's default `intMode` is `"number"`, not
   `"bigint"`), so `runtime/rows.ts` and `runtime/ticklog.ts` needed no
   changes either — `ticklog.ts:encodeValue` already rendered a JS `number`
   as a bare JSON number. The entire fix is therefore two Prolog files
   (`analyze.pl` + `lower.pl`'s DDL path) plus the mechanical `relplan/4` ->
   `relplan/5` arity change through `compile.pl`, `emit_ts.pl`, and both test
   harnesses. Before: `RUN identical=9 wrong=5`. After: `RUN identical=14
   wrong=0` (2 `run_error` + 1 `no_oracle_log` remain, both pre-existing and
   out of this ruling's scope — Finding 6). `compiled=17`,
   `unsupported=92` unchanged (this is a DDL/runtime-typing fix, not a
   supported-subset widening). Two incidental fixes landed in the same
   commit, both pre-existing and orthogonal to typing: `test/plunit_tests.pl`
   and `test/run_sql_check.pl` each hardcoded an absolute `fixture_file/1`
   path into a stale, since-deleted agent worktree (both tests were passing
   only because that worktree happened to still exist on disk by
   coincidence); both now resolve the path at load time via
   `prolog_load_context/2`, matching `sweep.pl`'s own `compile_dir/1`
   pattern.
5. **PHASE C2 RULING 2, commit `tsv2 C2b`: unmarked edge triggers.** Grounded
   in `engine.pl:trigger_items/2` (:136-145: `marked_items/2` if nonempty,
   else `unmarked_items/2`) and `body.pl:body_atoms/2` (:112-126, the exact
   goal classification the unmarked fallback walks) plus `occurrence_
   trigger/4` (engine.pl :162-166) and `solve/2` (body.pl :96-110): an
   unmarked edge body (no `only/1` anywhere) makes EVERY plain positive body
   atom its own independent trigger; firing binds ONLY that atom's own
   arguments (via unification with the arrived row) and then solves the
   WHOLE body — the other atoms are read against the CURRENT store, a real
   join, which can produce zero, one, or many derived rows per single
   triggering arrival (the rendezvous case). Hand-traced against
   `engine_core.pl:unmarked_edge_replays_backlog`
   (`sent(Client,Item) <+ change_ev(Item), subscriber(Client)`) tick by tick
   before writing any lowering code, confirmed byte-identical after.
   - `analyze.pl:edge_trigger_shape/2` (new) classifies a body into
     `marked_single(Atom)` (unchanged), `unmarked_conjunction(Atoms)` (N >= 1
     plain positive atoms, no `only/1`), or `unsupported(Reason)` with a
     PRECISE reason per blocking construct (`edge_marked_with_extra_goal`,
     `edge_body_needs_pre/now/departed/negation/bind/comparison/json_destructure`)
     instead of the old blanket `edge_body_shape`.
   - `lower.pl:edge_statements_for_rule/3` + `edge_statement_single/5`: one
     `edgestmt/6` PER CANDIDATE TRIGGER ATOM (N=1 for `marked_single` or a
     single-atom unmarked body — byte-identical ProjectSql/UpsertSql to
     round 2, verified via the plunit SQL-text snapshot tests, unchanged; N
     arms for an N-atom unmarked body). The non-triggering atoms compile via
     `compile_positive_uses/6` (REUSED from the level-rule side, not
     reimplemented), seeded with the trigger atom's own numbered-placeholder
     `Bound` so a shared variable becomes a join equality, not a fresh
     column.
   - `emit_ts.pl`: new shared `triggerOccurrences(kind, relName, beforeRows,
     arrivals)` helper — a Log-kind trigger is unconditionally every
     matching `+Row` arrival (unchanged); a Set-kind trigger is dedup-aware
     (engine.pl `absorb_arrivals/8`: an outside `+Row` into a Set rel mints
     an occurrence only when the row was not already present, checked
     PROGRESSIVELY across the tick's own arrival list) — exercises real:
     `set_dedups_log_stacks`'s two identical `+seen(alpha)` arrivals in one
     tick mint exactly ONE occurrence. Resolvers now iterate every row of
     `result.rows` (not just `rows[0]`) per triggering arrival, and Global
     Index (0-based position in the flattened `EdgeStatements` list, not
     per-head) disambiguates names since multiple rules or arms can now
     share a `HeadRef` (`merge_family.pl`'s `out(Item) <+ event_a(Item)` /
     `out(Item) <+ event_b(Item)`).
   - **Log-kind edge heads, a bonus widen beyond the ruling's own ask**: the
     round-2 code required every edge head to be a keyed Set rel
     (`edge_write_log_head`); `engine.pl:apply_edge_writes/6` (:236-254)
     always supported a Log head too (unconditional append, no key concept
     at all). `edge_statement_single/5` now branches on `HeadKind`; the
     emitted resolver ALSO branches (log: `written.push(...)` for every
     projected row, no dedup; set: unchanged `Map`-keyed last-write-wins) --
     collapsing a Log head's writes through a key `Map` would be wrong
     (`KeyColumns` is `[]` there, so every row would collapse to one key).
     Closes the SCOREBOARD's own former "edge writes into a Log-kind head"
     1-fixture bucket (`merge_policy`) plus unlocks `merge_batches_per_tick`,
     `merge_never_retracts`, `set_dedups_log_stacks`.
   - **Boot-time level closure, a gap Ruling 2 SURFACED, not caused**:
     `engine.pl:run_program/5` computes the t=0 level closure once, right
     after seeding `Initial` rows, before tick 1 exists; this compiler never
     had, since NO prior fixture combined non-empty `Initial` data with a
     level rule reading it (marked_single-only chains never reached that
     shape). `head_move_flips_current_tree_in_one_tick` is the first to.
     `lower.pl:boot_statements/4` (widened from `/3`) now appends the SAME
     `LevelStatements` SQL (no params, a literal DELETE/INSERT-SELECT) to
     the `boot` array once, after row-seeding.
   - **Findall-copies-its-template bug, caught and fixed before landing**:
     an early draft built the non-trigger atoms' `use(...)` list via
     `findall`, which (per `analyze.pl:ref_occurrence_args/3`'s own standing
     comment about this EXACT hazard) copies its template per solution and
     severs `OtherArgs` from the SAME variable objects the head shares —
     `head_select_list`'s `bound_lookup` then never finds them, throwing
     `unbound_head_var` even though the variables genuinely ARE bound.
     Fixed by `maplist`, matching the codebase's own established rule for
     this. Caught by `unmarked_edge_replays_backlog` itself failing before
     the fix (hand-verified both arms independently, confirmed the exact
     ProjectSql each arm should emit, then found the fix).
   - Two NEW static refusals guard against silently-wrong output the wider
     acceptance would otherwise let through (Findings 9 and 10 below):
     `check_edge_head_column_types` (edge-derived head column typed
     inconsistently with its source) and `check_no_edge_head_conflict_risk`
     (two rules/arms sharing a keyed head and a trigger ref, engine.pl's
     `check_occurrence_conflicts` territory this compiler has no runtime
     equivalent for).
   - Result: `compiled=17 -> 30`, `RUN identical=14 -> 27, wrong=3 -> 3`
     (unchanged set: `fork_join_error_arm_is_a_value`, `log_retraction_
     rejected`, `log_without_retention_rejected` — all pre-existing, out of
     this ruling's scope). `unsupported=92 -> 79`. The 48-fixture unmarked-
     trigger bucket is CLOSED (Finding 7's resolution): every fixture in it
     is now either IDENTICAL or refused under a precise, different-named
     construct (a co-blocking marked+extra-guard/comparison/aggregate rule
     elsewhere in the same program, or a genuine `pre`/`now`/`departed`/
     `not` gap this ruling did not add). Zero fixtures went UNSUPPORTED ->
     WRONG in the FINAL landed state (three did transiently during
     development -- Findings 7, 9, 10 -- and are refused by name, not
     hacked around).

## Findings

1. **Retention (`keep(Ref, count(N))`) is not lowered at all, and the sweep
   is blind to the gap.** `lower.pl` never reads `decl_keep/3` anywhere — no
   DDL, no pruning SQL, nothing. `retention_count_prunes_oldest`
   (`engine_core.pl`) shows IDENTICAL in this sweep because
   `conformance/ticklog.pl`'s own delta trace **never reflects retention
   pruning either**: `engine.pl` enforces `keep(count(N))` in a way that
   only shows up in `final(...)` row state (which `go.pl`'s conformance
   checker verifies separately, 109/109 green), never as a `-Row` delta in
   the per-tick trace this sweep diffs. Confirmed against the actual oracle
   file (`out/retention_count_prunes_oldest.oracle.jsonl`): three ticks,
   three plain adds, no deletes, even though the fixture's own comment says
   the oldest row should be pruned by tick 3. **This sweep's tick-log-only
   grading cannot detect a retention-lowering gap; a stricter grading pass
   would need to additionally compare final-row state per fixture (this
   sweep does not do that).**
2. **Four compiled fixtures have an empty Schedule** (`braces_in_head_position`
   is the one still in the IDENTICAL bucket; `comparison_filters_rows`,
   `range_join_over_arithmetic`, and `head_expression_evaluates_derived_column`
   were moved to UNSUPPORTED by the Finding-5-driven gate before this
   became visible in their bucket, but the underlying vacuousness is the
   same). These fixtures grade entirely via a `final(...)` expectation over
   the `t=0` closure of `Initial` rows — no ticks ever run.
   `conformance/ticklog.pl`'s `print_ticklog/3` only prints
   `DeltaTicks`, one per `Schedule` entry; an empty `Schedule` prints **zero
   lines**. The tsv2 side, run over the same empty schedule, also prints
   zero lines. **Zero lines equals zero lines is a vacuous pass, not a
   verified one.** `braces_in_head_position` was hand-checked well enough to
   confirm its head (a plain `{...}` -> JSON braces literal, no arithmetic)
   does not hit the same known-wrong pattern Finding 5 describes, but its
   IDENTICAL status here should be read as "not disproven," not "proven."
3. **RESOLVED by PHASE C2 RULING 1 (commit `tsv2 C2a`).** Residual
   number-vs-string representation gap (5 WRONG fixtures, all the same root
   cause). After the `bindArgs` fix (history step 2), an integer arrival
   value was stored correctly as SQLite-TEXT `"1"` (not `"1.0"`), but every
   column in this compiler's schema was declared `TEXT NOT NULL`
   (`lower.pl:column_def/2`) with no per-column type information anywhere
   upstream (`engine.pl` is untyped — see `fixtures/expressions.pl`'s own
   header comment: "no HM/enum type checker... no `rel_decl`/column type").
   Reading that TEXT value back (`runtime/rows.ts:selectRows`) returned a JS
   **string** `"1"`; `runtime/ticklog.ts:encodeValue` renders any
   non-`number` value as a quoted JSON string, so the tick log printed
   `"1"`. The oracle's Prolog term is a genuine `integer/1`, so
   `ticklog.pl:value_json/2` prints the bare JSON number `1`. This was the
   sole remaining cause for `log_deltas_follow_arrival_order`,
   `shuffled_arrival_reorders_log_deltas`,
   `level_view_reads_set_projection_not_occurrences`, `terminal_is_terminal`,
   and `live_nonzero_exit_keeps_rows` — all five now IDENTICAL.
   **This was a structural consequence of the TEXT-only column design, not a
   bug in any one lowering rule.** The ruling picked option (b) from the two
   named here — a heuristic, not a real type system — but grounded it in
   ALL of a column's concrete literal occurrences across Rules, Initial, and
   Schedule (`analyze.pl:rel_column_types/5`), not just "an all-digit TEXT
   value renders as a JSON number" at the SELECT boundary; see the widening
   history entry 4 above for the mechanism and why it stays sound for this
   corpus (Prolog's own reader makes a bare digit token an `integer/1`,
   never an atom, unless explicitly single-quoted — no fixture here quotes a
   digit string).
4. **Two independent `declared_refs/2` gaps, both real corpus fixtures, not
   hypotheticals.** (a) a `kind(Ref, _)`-declared rel with zero rule
   readers (`retention_count_prunes_oldest`,
   `log_without_retention_rejected`, `log_retraction_rejected`,
   `live_nonzero_exit_keeps_rows` all needed this). (b) a rel declared
   *only* via `keyed(Ref, _)` or `keep(Ref, _)`, no `kind/2` at all
   (`zombie_scope_negative_case_a2b`'s `open_pane/2`, deliberately unread by
   any rule per the fixture's own comment). Both are now covered; a THIRD
   theoretical gap — a rel that appears in neither `Rules` nor any `Decls`
   entry at all (referenced only by the raw Schedule, no declaration
   whatsoever) — was not found in this corpus and is not handled; it would
   currently surface as the same "undeclared rel" runtime error
   `zombie_scope_negative_case_a2b` hit before its fix.
   4b. **`terminal_is_terminal`'s multi-clause-per-head fix is verified
   correct**, not just no-longer-crashing: the emitted `stream_status`
   delta now genuinely includes `{"add":[["src","running"]],"del":[]}` at
   tick 1, matching the oracle's own trace exactly except for Finding 3's
   unrelated integer-representation gap.
5. **Compound-valued arrivals are stored as canonical text, but pattern
   matching expects json1.** `operators.pl`'s `fork_join_error_arm_is_a_value`
   arrives as `+outcome_a(ok(body_one))` — a Schedule row whose value is
   itself a compound term. `sweep.pl:row_value_json/2` (mirroring
   `ticklog.pl`'s own `value_json/2`, the OUTPUT-side convention) renders it
   as canonical Prolog text (`"ok(body_one)"`), which `lower.pl`'s
   `arrival_statement/2` binds verbatim into the TEXT column. But the level
   rule `both_ok(BodyA, BodyB) <- outcome_a(ok(BodyA)), outcome_b(ok(BodyB))`
   pattern-matches `ok(BodyA)` via `compile_pattern_arg/6`'s compound
   branch, which assumes the STORAGE encoding is json1
   (`json_extract(col, '$.fn') = 'ok'`) — the encoding `compile_head_expr/3`
   uses for rule-COMPUTED compounds, not the encoding a literal arrival gets.
   `json_extract` on non-JSON text throws `SQLITE_ERROR: malformed JSON`,
   confirmed as the exact and only cause (isolated by hand: the emitted
   DDL/SQL for this fixture, read directly, shows the mismatch). **A real
   fix needs a `json1_encode_term/2` Prolog predicate shared between
   "encode a literal Prolog term for storage" and
   `compile_head_expr/3`'s existing "encode a computed compound expression
   for storage" — scoped but not implemented this pass** (arity-N and
   nested-compound-argument cases need the same recursive care
   `compile_sub_args/6` already has for the computed-expression side).
6. **The three "throws" fixtures inside the compiled set have no tick log
   to grade against.** `conformance/fixtures/engine_core.pl` includes
   several fixtures whose entire purpose is exercising an `engine.pl`
   REJECTION path (`Expectations = [throws(...)]`); `oracle_dump.pl`
   correctly reports these as `ORACLE_THROW`, not an oracle log. This
   compiler has no equivalent "validate and refuse at compile/boot time"
   layer, so the emitted program's behavior on the SAME schedule varies
   fixture by fixture: `log_retraction_rejected`'s emitted
   `arrivalStatement` function DOES throw a JS `Error` naming
   `retract_from_log` (a deliberate, matching-in-spirit rejection —
   `lower.pl`/`emit_ts.pl` already refuse retraction-from-log structurally),
   but `log_without_retention_rejected` has **no such check for a missing
   `keep/2` declaration** and runs to completion silently, which the oracle
   never would. Bucketed WRONG for both (see the per-fixture table) since
   neither produces a byte-comparable tick log, but they are not the same
   kind of gap: one matches the oracle's rejection, the other is a genuine
   missing validation.
7. **RESOLVED by PHASE C2 RULING 2 (commit `tsv2 C2b`), with one residual
   stop-and-report.** Why the 48-fixture unmarked-trigger bucket was
   originally not attempted: an edge rule body that is a bare atom
   (`ping(Item)`, no `only(...)` wrapper) looks structurally identical to
   `only(Atom)` for a SINGLE-atom body, but `engine_core.pl`'s own paired
   fixtures (`marker_stops_backlog_replay` vs `unmarked_edge_replays_backlog`)
   prove the semantics genuinely differ once a SECOND body goal is present:
   an unmarked body atom is *also* a trigger candidate, so a late arrival on
   ANY body rel re-fires the rule over the full current join (backlog
   replay), not just over the newly-arrived row. The widening history's
   entry 5 has the full mechanism and citations. **Residual**: a trigger
   firing off a rel this SAME program derives via ANOTHER edge rule needs
   engine.pl's CarryIn/CarryOut threading (tick/7:299-312, "carry-out is
   boundary-observable writes only", fed forward as T+1 occurrences) — this
   compiler's `triggerOccurrences` (emit_ts.pl) only ever reads the tick's
   OWN `arrivals` parameter, which is always `[]` on a drain tick, so a
   chained edge trigger off ANOTHER edge rule's head can never fire.
   Confirmed WRONG, not theorized: `edge_chain_hops_tick_per_stage`
   (`stage_two(Item) <+ only(stage_one(Item))`, `stage_one` itself
   `<+ source_ev(Item)`) compiled clean and produced an empty tick 2 where
   the oracle shows `+stage_two(alpha)`, once this ruling lifted the
   unmarked-shape refusal that had masked the gap the whole time (no
   fixture with an all-marked_single/unmarked, no-extra-guard edge CHAIN
   had ever reached compilation before — every candidate in the corpus with
   this shape has a marked+extra-guard companion rule, itself still
   refused, EXCEPT this one). Fixing it is a real `IGenProgram`/
   `tickLoop.ts` change (threading carry occurrences into the next tick
   call) — STOP-AND-REPORT per the phase C2 contract, refused by name
   (`edge_trigger_is_derived`) rather than attempted.
8. **Boot-time level closure was missing entirely, surfaced (not caused) by
   Ruling 2.** `engine.pl:run_program/5` computes the t=0 level closure
   ONCE, immediately after seeding `Initial` rows and before tick 1's
   `state(...)` exists; this compiler's `boot` sequence only ever seeded
   base Set/Log rows (`boot_seed_statement/3`), never ran a level
   recompute. No PRIOR fixture combined non-empty `Initial` data with a
   level rule reading it (every marked_single-only chain in the corpus
   avoided that shape), so the gap was latent until
   `head_move_flips_current_tree_in_one_tick` (a genuine unmarked-single-
   atom win from this ruling) reached compilation and showed an ADD-only
   delta where the oracle shows DEL-then-ADD (the level view started empty
   instead of at its real t=0 rows). Fixed: `lower.pl:boot_statements/4`
   (widened from `/3`) appends the SAME `LevelStatements` SQL text
   `recomputeLevels` runs inside a tick, once more, with no bind params, to
   the `boot` array.
9. **Edge-derived head column type mismatch — a Ruling 1 x Ruling 2
   interaction, stop-and-report.** An edge rule's head column can inherit
   its VALUE from a body atom via a shared variable
   (`spine_semantics.pl`'s `xref(FromSpanId, ...) <+
   pin_extracted(FromSpanId, ...)`), but `analyze.pl:rel_column_types/5`
   (Ruling 1) infers each ref's OWN column types from its OWN literal
   occurrences alone — `xref/6` never appears as a raw Schedule arrival (it
   is edge-headed), so its `from_span_id` position never sees the literal
   integer values that only ever arrive via `pin_extracted`'s arguments, and
   defaults to `text`. The stored column is TEXT while the flowing value is
   a genuine integer, so the tick log prints the quoted string form the
   oracle prints as a bare number. `pin_to_unknown_repo_derives_repo_
   candidate` and `xref_rev_is_pin_data_not_live_head` both hit this and
   were WRONG before the fix. A real fix needs cross-rule type propagation
   (Ruling 1 only ever reasons about one ref's own literal occurrences,
   never traces a shared variable back to another ref's already-inferred
   type) — out of THIS ruling's scope. `analyze.pl:check_edge_head_column_
   types/2` (new, run in `compile.pl:program_plan/2` after `RelPlans`
   exists, since the earlier `check_supported_subset/1` call runs before
   `RelPlans` is built) detects the specific mismatch by direct term
   inspection (Head's args vs the source atom's args, matched by variable
   identity) and refuses by name
   (`edge_head_column_type_mismatch(HeadRef, Position, SourceType,
   HeadType)`) rather than emitting the wrong storage type.
10. **Edge head conflict risk — no runtime equivalent of engine.pl's
    per-occurrence conflict check, stop-and-report.**
    `engine.pl:check_occurrence_conflicts/2` runs once per OCCURRENCE,
    across every rule in the program, and throws `keyed_conflict/3` when
    the SAME occurrence satisfies two rules heading the same keyed rel with
    two DIFFERENT derived rows for the same key
    (`merge_family.pl:one_occurrence_two_rows_still_conflicts`:
    `(latest(cli,a) <+ ping(_)), (latest(cli,b) <+ ping(_))`, both rules
    triggered by the SAME `ping/1` occurrence). This compiler resolves each
    edge rule/arm independently with no equivalent validation, so it would
    silently let the LAST-running arm's write win instead of throwing. The
    conflict can only arise when two edge rules/arms sharing a KEYED head
    also share a trigger ref (`analyze.pl:shape_trigger_refs/2`); Ruling
    2's own comparison/last-write-wins fixtures (`key_last_write_wins` and
    siblings) stay safe because their two rules are triggered by DIFFERENT
    refs (`from_poll` vs `from_push`), so no single occurrence can ever
    satisfy both. `analyze.pl:check_no_edge_head_conflict_risk/2` (new)
    refuses the specific at-risk configuration by name
    (`edge_head_conflict_risk(HeadRef, SharedTriggerRefs)`) rather than
    implementing the full per-occurrence runtime check.
11. **Open semantics questions for a future widening pass** (in the
    priority order the phase C contract itself suggests, updated post
    Ruling 2 — the unmarked-trigger bucket is now closed, its residuals
    folded into Findings 7/9/10 above):
    - Comparison operators (14 fixtures) and arithmetic bind `:=`/`is`
      (5 fixtures) are both cleanly refused (a Finding from the pre-Ruling-2
      safety fix) rather than silently wrong; implementing them is the
      highest-value next step (19 fixtures combined, plus it likely also
      fixes the 2 `head_arithmetic` refusals once expression compilation
      exists for level bodies, since the same expression compiler would
      serve both).
    - Aggregates (9 fixtures: `count`/`sum`/`min`/`max`/`json_array`/
      `json_object` as GROUP BY) — `level_eval.pl`'s own aggregate handling
      was not read closely enough this pass to judge lowering-ambiguity;
      flagged as the next research target, not attempted.
    - `only(trigger) + extra guard goal` (21 fixtures, up from 9 now that
      earlier-blocking unmarked edge rules no longer mask them: `not/1`,
      `pre/1`, `now/1`, `decode/2` + a follow-on goal mixed into an
      otherwise-marked edge body) is likely the next highest-value target
      given its size — the trigger IS still singular and marked, only the
      extra guard goal needs a WHERE-clause equivalent on the edge-write
      path, which does not exist today.
    - `pre/1` in an unmarked edge body (8 fixtures) is the same-tick fold
      chain shape (`increment(Name,_), pre(counter(Name,Total)), Next :=
      Total + 1`) — needs `pre/1`'s "evolving pre-state" read PLUS
      arithmetic bind support in edge bodies, both new.
    - `now/1` in an unmarked edge body (5 fixtures) needs the current tick
      number to reach the SQL/JS lowering; `IGenProgram`'s `tick(seam,
      arrivals)` carries no tick-number slot at all (round 2's own choice)
      — a real seam change, not a compiler-only widen.
    - JSON destructuring (`decode/2`, `json_each/2`, 4 fixtures in level
      bodies + 1 in an unmarked edge body) needs array-explode semantics
      this compiler has no SQL shape for yet (mirrors the v5 F3 finding
      recorded in the project ledger: "no json term-extract, array-explode
      inexpressible").
    - `departed/1` in an unmarked edge body (2 fixtures) needs the
      departure-occurrence mechanism (r4: a Set/level row's `-delta` as a
      T+1 occurrence) — a genuinely separate feature from arrival triggers,
      unimplemented in this compiler at any level.
