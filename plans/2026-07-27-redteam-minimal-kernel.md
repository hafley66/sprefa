# RED TEAM: the minimal-kernel claim (switch_flow section 7)

Claim under attack, `plans/2026-07-27-switch-flow.md` HEADLINE: the subscription
kernel needs **zero stored engine rels and zero new tick phases**; switchMap is
keyed replace on an ordinary program rel, flattening policy is the scope row's
primary key shape, teardown is the ordinary IVM retraction cascade.

Probes: `v6/prolog/labs/redteam_kernel.pl`, 27 checks, all PASS, self-contained.
DELETED per lab protocol; last copy at commit d655bb2d, recover via
`git show d655bb2d:v6/prolog/labs/redteam_kernel.pl`.
(`swipl -q -l v6/prolog/labs/redteam_kernel.pl -g go -g halt`; `-g report` prints
the round table). Lab under attack recovered and re-run green at 89/89 from
`git show ac2aafdc:v6/prolog/labs/switch_flow.pl`.

## VERDICT TABLE

| # | attack | verdict |
|---|---|---|
| A1 | teardown cost, cascade vs range-DELETE | **DENTED**: cost is f(recursion depth in the cone), and it is the SAME in both models |
| A2 | the moved storage / forensics | **DENTED**: three questions the forest answered from rows, the kernel cannot |
| A2b | "nesting needs no path" (section 7.2) | **BROKEN**: one deleted body atom leaks a scope forever, nothing checks it |
| A3 | is the self-completion law decidable | **DENTED**: decidable because it IS stratification; both failure directions built |
| A4 | silent serialization on a multi-key register | **HOLDS**: the forest is equally blind; kernel loses one weak artifact |
| A5 | sugar round-trip / per-program rel count | **DENTED**: zero rels per KERNEL, 50 rels + 100 rules per 50-site PROGRAM |
| A6 | prior art | **DENTED**: no system found derives a scope tree; the demand half has pedigree |

## A1. THE UNMEASURED COST, NOW MEASURED

Ambiguity 9 said a Prolog interpreter cannot measure this. It does not have to:
`v6/sprefa-store/js/src/engine/engine.ts` already ships four SQLite retraction
cascades (`retract`, `retract_scc`, `retract_dred`, `retract_dred_cte`), each
returning **rounds**, and `engine/measure.ts` `benchgraph.gen_multi(layers,
width)` already generates the exact adversarial shape (layers = cone depth,
width = cone breadth). `engine/counter.ts` `stmt_counter` counts statements.
Measured on libsql in-memory, retracting ONE root, killing TWO rows:

| cone depth | counting rounds | counting stmts | DRed rounds | DRed stmts | DRed ms |
|---|---|---|---|---|---|
| 1 | 2 | 21 | 1 | 23 | 1 |
| 8 | 2 | 21 | 15 | 114 | 2 |
| 32 | 2 | 21 | 63 | 426 | 7 |
| 128 | 2 | 21 | 255 | 1674 | 30 |
| 256 | 2 | 21 | 511 | 3338 | 58 |

Breadth, depth fixed at 2, 16386 rows with 8192 killed: counting 3 rounds / 29
statements, DRed 3 rounds / 36 statements. **Flat in rows, linear in depth.**
DRed rounds = 2\*depth - 1; DRed statements ~= 13\*depth + 10.

Three consequences.

1. **Counting retraction is O(1) statements at every depth and every width.** If
   the scoped cone is acyclic, cascade teardown is 21 statements, full stop, and
   the forest's 3-statement range DELETE bought nothing.
2. **A recursive rel in the cone forces DRed**, because counting over-keeps on a
   phantom cycle. That is not my claim; `tests/engine/golden.test.ts:339`
   ("cascade: phantom cycle — counting over-keeps, DRed kills") already asserts
   it. Recursion depth inside a recursive stratum is DATA-dependent, so at
   depth 256 the teardown issues 3338 statements. Ruling `n1_statement_budget`
   says statements per tick = f(rules, strata), **never f(rows)**. A scope whose
   cone crosses a recursive stratum violates that ruling on teardown.
3. **The forest never escaped it either.** The 3 range DELETEs cover
   `sub`/`sub_path`/`demand` and stop at `demanded/2`; every scoped view row
   below still dies by the same cascade. Probe
   `the_range_delete_never_covered_the_view_cone` grades the forest and kernel
   round counts identical at depths 2/4/8/16, and
   `recompute_teardown_is_identical_in_both_models` grades the recompute path
   the same way.

**What the forest actually bought, isolated:** nested SCOPES, not the cone.
`the_forest_advantage_is_exactly_the_scope_nesting_depth` grades the kernel at
depth+1 rounds for D nested scopes ([3,5,9,17] at D = [2,4,8,16]); the forest
kills D nested scopes in one prefix range scan. Scope nesting depth is lexical
and statically countable. Cone depth is not. So the honest restatement is:

> Teardown cost is f(recursion depth in the scoped cone) in BOTH models, plus
> f(scope nesting depth) in the minimal kernel only. The first term dominates
> and the forest never addressed it.

Two side receipts. `v6/sprefa-store/js/src/lower/lowerSql.ts:80` currently does
a **full recompute** every run (`clearStatements()` issues `DELETE FROM` per IDB
table, then re-derives), so the SQL lowering has no incremental retraction path
at all today; the cascade lives on the separate `cx_*` plane in `engine.ts`.
Under full recompute, cascade teardown settles in ONE round at every depth
(probe `recompute_teardown_is_one_round_at_every_cone_depth`) because the answer
is empty, and the statement budget is met while the WORK stays O(all rows). The
statement budget cannot see that cost.

## A2. THE MOVED STORAGE

Survivor list item 1 is "the demand projection + bind lifecycle, one engine
BEHAVIOR, not a rel". Three forensic questions the forest answered from rows.

**(a) Who supports this demand row?** `two_scope_rules_producing_one_demand_row_are_unattributable`
grades the store with two supporters and with one as **byte-identical**:
`[demanded(feed(alpha), 1)]` either way. The forest kept one `demand(DemandId,
SubId, Target, Salt)` row per subscriber, so both the count and the owner were
on disk (`the_forest_kept_one_demand_row_per_supporter`).

**(b) Who is the parent of this scope?** `the_store_cannot_name_the_parent_of_a_nested_scope`
grades that no row in the model relates `live_detail` to `open_pane`. Nesting is
a body atom in a rule, so answering "what died because pane_one closed" needs the
rule graph, not the store. `dl daemon why` reads the on-disk trail.

**(c) When did the effect start and stop?** The bind lifecycle is behavior with
no row. The forest emitted `+sub`/`-sub` deltas.

Restoring parity needs `scope_started`, `scope_ended`, `demand_started`,
`demand_ended` on the tracing spine (`src/eventlog.rs`, `EventLayer`). Four
event kinds is not less storage than three rels; it is storage in a different
file, one the query language cannot join. This is the same class as the
`"15 changed path(s)"` incident: the arguments were in memory and the trail kept
a count.

## A2b. THE STRUCTURAL GUARANTEE THAT WAS TRADED AWAY: BROKEN

Section 7.2 says the path row is unnecessary because "the inner scope's liveness
JOINS the outer's row". That is true of the programs the lab wrote. It is a
convention, not a mechanism.

Run against the live 89/89 lab, `derived_nesting` with exactly one body atom
removed (`open_pane(PaneId, _)` dropped from `live_detail`):

```
1  +demanded(detail(item_a),pane_one) +live_detail(pane_one,detail(item_a)) ...
2  +detail_row(item_a,body_a) +detail_view(item_a,body_a)
3  -open_pane(pane_one,item_list)                        <- outer closes, nothing else moves
4  +detail_row(item_a,late_body) +detail_view(item_a,late_body)   <- LATE FILL ADMITTED
```

The inner scope outlives its parent forever and the demand gate lets the stale
fill through. Reproduced self-contained by
`dropping_one_body_atom_leaks_the_inner_scope_forever`;
`the_leak_is_one_atom_of_difference_and_no_check_sees_it` grades the two rules as
head-variant-equal with body lengths 2 and 1. In the forest a plant is parented
by construction (`the_forest_cannot_express_the_leak`): the child's path is the
parent's path plus its id, so prefix teardown covers it and there is no spelling
that leaks.

This is the strongest finding. The kernel did not delete the scope tree; it
moved it into rule text where no check reads it.

## A3. IS THE STATIC LAW DECIDABLE

Law: "the completion condition may not be a level rule over rows produced under
its own scope". **Yes, decidable**, because as the lab implements it (7.4,
`self_completion_negation_is_stratified_by_construction`) it is exactly
predicate-level stratification, which the engine already owes for negation
(`the_new_law_is_the_stratification_check_the_engine_already_owes`). Zero new
machinery. But the reduction is lossy in both directions.

**Unsound program that slips through.** Write the fork's completion as a level
rule. The real cycle is
`live_fork -neg-> fork_closed -> result_a -DEMAND-> demanded -> live_fork`.
`result_a` is bind-filled and has no rule, so the rule graph has no outgoing edge
from it and the check PASSES (`rule_graph_stratification_accepts_the_unsound_self_completion`).
Adding the bind edge as a rule makes it fire
(`the_demand_gate_edge_is_what_makes_the_cycle_visible`). **The dependency graph
must gain an edge from every bind-filled rel to the `demanded/2` atoms that gate
it, or the law is unenforced on exactly the programs it exists for.** No such
edge exists in `lower/rulegraph.ts` today.

**Sound program that is refused.** Generation-indexed self-completion, the shape
section 7.1's own `scope_instance` counter produces:

```prolog
done(Session, Gen)   <- result(Session, Prev), Prev < Gen.
live(Session, Gen)   <- open_gen(Session, Gen), not(done(Session, Gen)).
result(Session, Gen) <- demanded(target(Session), Gen), row(Session).
demanded(target(Session), Gen) <- live(Session, Gen).
```

The generation strictly decreases around the cycle, so the instance graph is
well founded and the model settles (`the_generation_indexed_program_has_a_settled_model_anyway`
grades `live(session_one, 0)` and `demanded(target(session_one), 0)` present).
Predicate-level stratification refuses it
(`predicate_stratification_refuses_the_generation_indexed_program`). So the law
refuses the idiom the minimal kernel introduced to make stale fills refusable.

## A4. SILENT SERIALIZATION: the attack fails

`a_losing_keyed_plant_emits_no_boundary_delta`: two keyed writes into one slot
inside one tick produce `[+open_tab(session_one, tab_b)]` and nothing else,
because r7 diffs tick start against tick end. Invisible.

The forest is **not** better. The lab's own
`one_parent_scope_is_one_flattening_slot` grades `scope_birth_ticks(...) == []`,
an invisible scope. The forest leaves exactly one artifact the kernel does not:
its dense id sequence advances past the number of live subs, so a gap is
auditable (`the_id_sequence_gap_is_the_forests_only_artifact_of_a_losing_plant`
grades 2 planted, 1 alive, sequence at 1002). Grade of that artifact: **weak and
non-attributable**: a gap says a plant lost, never which target.

Rx receipt for the shape: `switchMap` has no key concept at all; it holds one
`innerSubscriber` for the whole outer stream, so `groupBy` piped straight into
one `switchMap` cancels across keys. That is a documented rx footgun, not a
feature, and the language is reproducing it. Both models need a `diag` row here
(ruling `a6_diag`); neither has one.

Divergence worth flagging under the RXJS FIRST law: rxjs `switchMap.ts` `next`
calls `innerSubscriber?.unsubscribe()` **before** `from(project(value,
index)).subscribe(...)`, and a synchronous emission from the dying inner is
dropped (rxjs PR #4037 exists to guarantee that). Section 8's preferred
stale-fill reading, `orphan-as-a-row`, ADMITS it. Also, rxjs `switchMap` does
not complete downstream while an inner is live (`checkComplete`); the kernel's
outer row leaving kills the inner immediately. Two contract mismatches the js
leg will hit first.

## A5. SUGAR ROUND-TRIP

Section 7.7's recipe per site: one keyed rel decl, one edge rule writing the
scope row, one level rule projecting `demanded/2`.
`desugaring_fifty_switch_sites_mints_fifty_rels_and_a_hundred_rules` grades 50
unique rel names, 50 key declarations, 100 rules for a 50-site program.

Can sugar share ONE scope rel? Only with a site discriminator column.
`sharing_one_scope_rel_keyed_by_the_outer_key_alone_collides_across_sites`
grades site two silently replacing site one's scope. With the site column the
shared rel is `scope(SiteId, Key, Target)`, arity 3, the same arity as
`sub(SubId, ParentId, Target)` (`the_site_column_restores_the_forests_scope_id_by_another_name`).
**The sugar that rescues the 1-line surface re-mints the scope id section 7
deleted.**

The policy word also survives:
`the_policy_word_survives_desugaring_as_four_distinct_expansions` grades four
distinct (key, guard) pairs, and
`concat_is_the_one_policy_whose_expansion_needs_a_second_program_rel` grades
concat alone needing `pending_tab/2`. Storage moved into the key declaration;
the surface still needs four words. The construct budget did not drop by one.

So: **zero rels per KERNEL, true. Zero rels per PROGRAM, false.**

## A6. PRIOR ART

| system | scope tree | cost model | source |
|---|---|---|---|
| Kotlin coroutines | STORED: `ChildHandleNode` in a `NodeList` off `JobSupport._state` | O(direct children) traversal in `notifyCancelling()`, cooperative stop | `kotlinx-coroutines-core/common/src/JobSupport.kt` |
| Trio nurseries | STORED: `Nursery._children: set[Task]` | O(descendant cancel scopes), explicit-stack DFS in `CancelStatus.recalculate()`, comment says the stack exists to avoid stack overflow | `trio/_core/_run.py` |
| Rust NLL / region inference | derived STATICALLY, erased at runtime | zero runtime cost, zero runtime inspectability | Tofte & Talpin, Inf. Comput. 132 (1997); RFC 2094 |
| refcount vs tracing GC | refcount = derived liveness by support | cascading decrements give unbounded pauses; Blackburn & McKinley OOPSLA 2003 defers decrements precisely to bound it | OOPSLA 2003 |
| magic sets / demand transformation | demand IS materialized as ordinary IDB rows | the win is smaller rows, not avoided materialization | Bancilhon/Beeri/Ramakrishnan/Ullman lineage |

**No system found derives its runtime scope tree.** Absence of a counter-example
is not proof, and the researching agent flagged that specific line as low
confidence, but both live structured-concurrency implementations store it and
both pay a traversal, and Trio's authors anticipated deep trees explicitly.

Deletion asymmetry, the cleanest citation found (Motik et al.,
arXiv:1811.02304 section 7, read directly): *"Incremental insertions are in
general easier to handle than deletions since during insertion the algorithms
can rely on the whole materialisation to prune the propagation of facts whereas
during deletion the algorithms can only rely on the nonrecursive counters of
facts to do the same."* DRed's two phases are Gupta, Mumick, Subrahmanian,
SIGMOD 1993. No published closed form was found for DRed's round count on an
N-chain; the measurement above supplies 2N-1 for this implementation, and the
Prolog proxy agrees (`dred_rederivation_walks_the_cone_a_second_time`: 10
delete rounds + 9 rederive rounds against 10 plant rounds at depth 8).

The kernel's split is derive-not-store on the axis where every prior system
stores (the scope tree), store-not-derive on the axis where prior art also
stores (demand). The demand half has pedigree. The scope-tree half has none
found, and its closest analogue is the mechanism the GC literature added
deferral to precisely because of cascade cost.

## EXPERIMENT DESIGN FOR WHAT IS STILL UNMEASURED

The A1 numbers above are the `cx_*` cascade plane, not a lowered datalog
program. To close the last gap, add to `tests/engine/golden.test.ts` a case that
lowers a real program instead of a synthetic graph:

- **Tables**: one scope rel `open_scope(key, target)` keyed [1]; a chain
  `view_1..view_D` where `view_1 <- demanded(target, _), base(value)` and
  `view_k <- view_{k-1}`; one recursive rel `reach(a, b) <- edge(a, b) |
  reach(a, m), edge(m, b)` inside the cone so DRed is forced.
- **Statements**: reset `stmt_counter`, delete the one `open_scope` row, run the
  cascade, read `stmt_counter.get()`.
- **Curves expected**: acyclic cone, statements flat in D and in row count.
  Recursive cone, statements linear in the recursion's fixpoint depth, which for
  `reach` over a path graph of length L is L, i.e. **f(rows)** and a
  `n1_statement_budget` violation.
- **Control**: the same measurement with the forest's 3 range DELETEs standing
  in for the scope-root delete, to confirm the two differ by a constant.

Housekeeping flag: `v6/sprefa-store/js/src/engine/measure.ts:23` and `:51` use
a banned word in comments; the plain word there is "critical".
