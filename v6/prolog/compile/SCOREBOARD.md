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
| UNSUPPORTED (compiler refuses, named construct) | 92 |
| compiled (lowering + emission succeeded) | 17 |
| — of which IDENTICAL (tick log byte-identical to oracle) | 14 |
| — of which WRONG (diff, crash, or silent gap vs oracle) | 3 |

IDENTICAL + WRONG + UNSUPPORTED = 14 + 3 + 92 = 109.

Two of the 14 IDENTICAL rows are **vacuous passes**, not verified correctness
— see Findings 1 and 2. Read as "14 nominal, 12 genuinely exercised and
correct."

**PHASE C2 RULING 1 (typed columns) landed** (commit `tsv2 C2a`): the 5
int-vs-string WRONGs Finding 3 documented are now IDENTICAL — see the
widening history's entry 4 and Finding 3's resolution note below. Totals
above are post-landing; the pre-landing numbers (9 IDENTICAL / 8 WRONG) are
preserved in git history at the commit before `tsv2 C2a`.

## Per-fixture table: compiled (17)

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
| fork_join_error_arm_is_a_value | operators.pl | WRONG (crash: `SQLITE_ERROR: malformed JSON`) | compound arrival VALUE stored as canonical text, read back via `json_extract` which expects json1 — Finding 5 |
| log_retraction_rejected | engine_core.pl | WRONG (crash, matches oracle's *intent*) | oracle throws `retract_from_log`; emitted program also throws (a JS `Error`, not a byte-comparable log) — Finding 6 |
| log_without_retention_rejected | engine_core.pl | WRONG (silent gap) | oracle throws `missing_retention`; emitted program has no such validation and runs "successfully" — Finding 6 |

## Per-construct blocked tally (UNSUPPORTED, ranked)

| construct | fixtures blocked | example |
|---|---:|---|
| edge rule with an **unmarked trigger** (bare atom body, no `only(...)`) | 48 | `sink(Item) <+ ping(Item)` |
| **comparison operators** in a level body (`<`, `=<`, `>`, `>=`, `==`, `\==`) | 12 | `Union > 0, Shared*100/Union >= 40` |
| edge rule with `only(trigger)` **plus an extra guard goal** (`not/1`, `pre/1`, `now/1` mixed in) | 9 | `only(open_request(...)), not(live_tab(...))` |
| **aggregate head** (`count`, `sum`, `min`, `max`, `json_array`, `json_object` as GROUP BY) | 9 | `hits(Repo, count(Item))` |
| **arithmetic bind** (`:=` / `is`) in a level body | 5 | `Sum := Base + Extra, Sum > 10` |
| **JSON destructuring** (`decode/2`, `json_each/2`) | 4 | `decode(Doc, {name: Name})` |
| **arithmetic in a rule head** (`+`,`-`,`*`,`/`,`mod` as a head argument, not evaluated) | 2 | `jaccard(L, R, Shared*100/Union)` |
| `keyed(Ref, _)` on a **Log rel** | 1 | `keyed(latest/2, [1])` where `latest/2` is `kind(latest/2, log)` |
| edge trigger that is not a Log rel | 1 | `demand_view_fires_its_consumer_once` |
| edge writes into a Log-kind head | 1 | `merge_policy`'s `closed/2` |

Total: 48+12+9+9+5+4+2+1+1+1 = 92.

The **unmarked-trigger** bucket dwarfs everything else (48/92, 52% of all
refusals) — see Finding 7 for why it is not a quick widen despite looking
like "the same shape as `only(Atom)` minus a wrapper."

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
7. **Why the 48-fixture unmarked-trigger bucket was not attempted.** An
   edge rule body that is a bare atom (`ping(Item)`, no `only(...)` wrapper)
   looks structurally identical to `only(Atom)` for a SINGLE-atom body, but
   `engine_core.pl`'s own paired fixtures
   (`marker_stops_backlog_replay` vs `unmarked_edge_replays_backlog`) prove
   the semantics genuinely differ once a SECOND body goal is present: an
   unmarked body atom is *also* a trigger candidate, so a late arrival on
   ANY body rel re-fires the rule over the full current join (backlog
   replay), not just over the newly-arrived row. Lowering this correctly
   needs the edge-write SQL to re-derive from a fresh JOIN keyed by
   whichever rel fired, not the current "project one arrival row via
   numbered placeholders" shape (`lower.pl:edge_statement/3`), which only
   makes sense when there is exactly one, MARKED trigger. This is real
   design work, explicitly the kind of judgment call the contract says to
   stop at, not a mechanical widen — left UNSUPPORTED, most valuable next
   target given its fixture count.
8. **Open semantics questions for a future widening pass** (in the
   priority order the phase C contract itself suggests):
   - Comparison operators (12 fixtures) and arithmetic bind `:=`/`is`
     (5 fixtures) are both now cleanly refused (Finding 5's gate) rather
     than silently wrong; implementing them is the highest-value next step
     (17 fixtures combined, plus it likely also fixes the 2 `head_arithmetic`
     refusals once expression compilation exists for level bodies, since
     the same expression compiler would serve both).
   - Aggregates (9 fixtures: `count`/`sum`/`min`/`max`/`json_array`/
     `json_object` as GROUP BY) — `level_eval.pl`'s own aggregate handling
     was not read closely enough this pass to judge lowering-ambiguity;
     flagged as the next research target, not attempted.
   - `only(trigger) + extra guard goal` (9 fixtures: `not/1`, `pre/1`,
     `now/1` mixed into an otherwise-marked edge body) is a smaller,
     likely-more-tractable slice of the unmarked-trigger problem (Finding
     7) since the trigger IS still singular and marked — only the extra
     guard goal needs a WHERE-clause equivalent on the edge-write path,
     which does not exist today (`lower.pl:edge_statement/3` has no WHERE
     clause at all). Worth a dedicated look before the full unmarked-trigger
     problem.
   - JSON destructuring (`decode/2`, `json_each/2`, 4 fixtures) needs
     array-explode semantics this compiler has no SQL shape for yet
     (mirrors the v5 F3 finding recorded in the project ledger: "no json
     term-extract, array-explode inexpressible").
