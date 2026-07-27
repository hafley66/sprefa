# mode_lab: (cardinality, lifetime) with dominance

Lab: `v6/prolog/labs/mode_lab.pl` (80 checks, all PASS, 43ms).
Run: `swipl -q -l v6/prolog/labs/mode_lab.pl -g go -g halt`
Table: `swipl -q -l v6/prolog/labs/mode_lab.pl -g report -g halt`

Contract: `plans/2026-07-27-mode-dominance.md` (the five grading cases at the
bottom), `labs/AGGREGATE.md` T6, `labs/AUDIT.md` finding 13,
`labs/shell_stream.md` sections 1 and 2, `labs/check_eventing.md` ask table,
`conformance/rulings.pl` q8, q10, r4.

This is a static analysis. It computes modes over program facts plus bind
facts. It runs no ticks and spawns no processes. 18 programs are represented
as facts; 30 asks and 8 rule bodies are graded.

## 1. Verdict

The mode type survives contact with all five plan cases, the five eventing ask
rows, the three shell result-type cells, the AUDIT counterexample, and a new
forkJoin row. Three things in the plan had to change to make that work, and
all three are AUDIT finding 13 in some form.

**The plan uses one word, `min`, for two operations that point opposite ways.**
Split and named:

```prolog
% DOMINANCE (switch_map nesting). The inner ends when its own binding ends
% OR the enclosing scope ends. Disjunction.
scope_min(Left, Right, Result) :-
    (   Left == finite  -> Result = finite
    ;   Right == finite -> Result = finite
    ;   Left == never   -> Result = Right
    ;   Right == never  -> Result = Left
    ;   Left = until(LeftClauses), Right = until(RightClauses),
        append(LeftClauses, RightClauses, Combined),
        normalize_clauses(Combined, Normalized),
        dnf_lifetime(Normalized, Result)
    ).

% A RULE BODY (or a rel's several rules). New derivations keep arriving until
% EVERY input has stopped producing. Conjunction.
join_max(Left, Right, Result) :-
    (   Left == never   -> Result = never
    ;   Right == never  -> Result = never
    ;   Left == finite  -> Result = Right
    ;   Right == finite -> Result = Left
    ;   Left = until(LeftClauses), Right = until(RightClauses),
        findall(Product,
                ( member(LeftClause, LeftClauses),
                  member(RightClause, RightClauses),
                  append(LeftClause, RightClause, Product) ),
                Products),
        normalize_clauses(Products, Normalized),
        dnf_lifetime(Normalized, Result)
    ).
```

**Lifetime is not a 3-point total order.** It is the free distributive lattice
over the set of end-signals, with two constants:

| lifetime | as a boolean formula | reading |
|---|---|---|
| `finite` | TRUE | something always ends it |
| `until(F)` | the monotone formula F | ends exactly when F becomes true |
| `never` | FALSE | nothing ever ends it |

`scope_min` is OR, `join_max` is AND, and the canonical form is DNF reduced to
an antichain of clauses by absorption. That makes both operators total,
commutative, associative, idempotent and mutually distributive, with `finite`
the identity of `join_max` and the annihilator of `scope_min`, and `never` the
other way round (12 algebraic checks). `until(a)` and `until(b)` stay
incomparable, which is correct, and the analysis never has to compare them
because it combines them:

```
scope_min(until(disconnect), until(document_closed)) = until(any_of([disconnect, document_closed]))
join_max(until(disconnect), until(outer_next))       = until(all_of([disconnect, outer_next]))
```

**Mode analysis is a post-link pass.** `every` is a bind, not a program
construct. Three checks hold the line: an unlinked program produces
`reject(no_bind_for(every_300))` for every ask; relinking the same program text
with `bind every_300 = shell('seq 1 10')` flips `? change_log` from
`(multi, never)` to `(multi, finite)` with no rule change; and a bind whose
lifetime exceeds the signature's claim is a link error
(`reject(bind_outlives_claim(extract, never, finite))`, the T5 obligation).

The link rule is an inequality, not an equality:
`lifetime_leq(BoundLifetime, DeclaredLifetime)`. A bind may end sooner than the
signature promises and never later. `tail -f` into a `Stream`-typed rel is
rejected; `shell` into a `Tail`-typed rel is allowed.

## 2. The mode table

Every graded ask. `warn` states the mode and names the reason; `reject` refuses
to state a mode and names the defect.

| program | ask | form | card | lifetime | verdict |
|---|---|---|---|---|---|
| ghcacher | ask_fetch_request | request | det | finite | ok |
| ghcacher | ask_cache_bound | snapshot | semidet | finite | ok |
| ghcacher | ask_change_log_tail | tail | multi | never | warn(tail_never_terminates([every_300])) |
| ghcacher_scoped | ask_fetch_request | request | det | finite | ok |
| ghcacher_scoped | ask_cache_bound | snapshot | semidet | finite | ok |
| ghcacher_scoped | ask_change_log_tail | tail | multi | until(outer_next) | ok |
| ghcacher_unlinked | ask_fetch_request | request | undetermined | undetermined | reject(no_bind_for(every_300)) |
| ghcacher_unlinked | ask_cache_bound | snapshot | undetermined | undetermined | reject(no_bind_for(every_300)) |
| ghcacher_unlinked | ask_change_log_tail | tail | undetermined | undetermined | reject(no_bind_for(every_300)) |
| ghcacher_relinked | ask_fetch_request | request | det | finite | ok |
| ghcacher_relinked | ask_cache_bound | snapshot | semidet | finite | ok |
| ghcacher_relinked | ask_change_log_tail | tail | multi | finite | ok |
| eventing | ask_hook_write | write | det | finite | ok |
| eventing | ask_hook_snapshot | snapshot | multi | finite | ok |
| eventing | ask_lsp_tail | tail | multi | until(any_of([disconnect, document_closed])) | ok |
| eventing | ask_commit_gate | snapshot | multi | finite | ok |
| eventing | ask_dashboard_tail | tail | multi | never | warn(tail_never_terminates([file_change])) |
| shell_modes | ask_fetch_env | request | det | finite | ok |
| shell_modes | ask_extract_lines | request | multi | finite | ok |
| shell_modes | ask_log_tail | tail | multi | never | warn(tail_never_terminates([log_tail])) |
| shell_mislinked | ask_extract_lines | request | undetermined | undetermined | reject(bind_outlives_claim(extract, never, finite)) |
| timer_alone | ask_timer_tail | tail | multi | never | warn(tail_never_terminates([every_300])) |
| timer_scoped | ask_timer_tail | tail | multi | until(outer_next) | ok |
| audit_join | ask_job_tail | tail | multi | never | warn(tail_never_terminates([timer])) |
| fork_join_det | ask_combined | snapshot | semidet | finite | ok |
| fork_join_timer | ask_combined | tail | multi | never | warn(tail_never_terminates([every_60])) |
| fork_join_timer_scoped | ask_combined | tail | multi | until(outer_next) | ok |
| retention_unbounded | ask_feed_tail | tail | multi | never | warn(tail_never_terminates([tick])) |
| retention_bounded | ask_feed_tail | tail | multi | never | warn(tail_never_terminates([tick])) |
| sse_out | ask_wire_tail | tail | multi | until(disconnect) | ok |

Rule bodies, where cardinality comes from the conjunction rather than from a
key lookup:

| program | rule | card | lifetime |
|---|---|---|---|
| fork_join_det | r_combined | det | finite |
| fork_join_timer | r_combined | multi | never |
| fork_join_timer_scoped | r_combined | multi | until(outer_next) |
| fork_join_semidet | r_combined | semidet | finite |
| audit_join | r_job | multi | never |
| departure | r_unwatched | multi | finite |
| departure | r_diagnostic | multi | never |
| departure | r_cleared | multi | never |

### How each row is derived

Cardinality is read off the declaration, never guessed:

| input | card | source |
|---|---|---|
| `-> FetchResult` det envelope, program columns bound | det | the Error arm makes failure a value, so the call cannot NOT produce a row |
| keyed rel, every key column bound | semidet | the key is a functional dependency, so 0 or 1 |
| keyed write, key bound | det | the write lands exactly one row |
| unkeyed read, any tail ask, `Stream`, `Tail` | multi | no FD caps the answer |

Conjunction is max on `det < semidet < multi`.

Lifetime is a fold over the rel graph, iterated to a least fixpoint from the
bottom (everything `finite`), with dominance applied at every step:

```
own(effect)   = join_max(protocol_lifetime(bind), lifetime of its demand rows)
own(register) = lifetime of its `over` stream
own(derived)  = join_max over every atom of every rule heading it
own(fact)     = finite
effective(N)  = scope_min(own(N), lifetime of every scope N is in)
```

The demand half of the effect rule is what makes plan case 5 work. `fetch` is
bound to `shell`, so its protocol lifetime is `finite` per request, but its
demand rows come from `poll`, which joins the 300s clock. `join_max(finite,
never) = never`, so the register over it is `never`, and dominating the clock
with a `switch_map` scope walks the whole chain back to `until(outer_next)`
(graded across `every_300`, `poll`, `fetch`, `cache`, `change_log`).

Ask lifetime by form: `snapshot` and `write` are `finite` by construction (a
SELECT completes, a keyed write completes), so they consult nothing.
`request` takes the effect's bound lifetime. `tail` takes the target rel's
fixpoint lifetime. Both are then dominated by whatever scope holds the ask.

## 3. Static lifetime is not runtime lifetime

The static lifetime is per-rel and per-ask, computed at compile time, and it
answers one question: does this ask complete on its own. The runtime object is
per-subscription and lives in the `runtime_subs` forest, and it answers a
different one: did this subscription's demand rows get deleted.

They are allowed to disagree, and the lab grades a case where they do. The
dashboard tail is statically `never`, and its runtime subscription is
`ended(teardown)`. Teardown is a range-DELETE of a path prefix, not a
completion notification, so no `ended(complete)` row exists for it
(`static_never_coexists_with_an_ended_runtime_sub`,
`runtime_completion_never_happens_on_a_never_ask`). A CLI that reports "the
stream finished" on teardown is reporting the wrong object.

## 4. Deviations from plans/2026-07-27-mode-dominance.md

1. **`min` splits into `scope_min` and `join_max`.** The plan's line 37
   ("derived rule: join of body inputs") and line 41 (`lifetime(inner) =
   min(...)`) are two different operators. The AUDIT counterexample is
   reproduced as the `audit_join` program: `job(name, bucket) <- config(name),
   timer(bucket)` with `config` finite and `timer` never. `scope_min` says
   `finite`, which is false, and `join_max` says `never`, which is right
   (`scope_min_would_get_the_audit_case_wrong`).
2. **The 3-point total order at line 25 is replaced by a lattice.** `finite <
   until(S) < never` is true only at the ends. `until(a)` and `until(b)` are
   incomparable (`until_signals_are_incomparable`), and the order used
   internally is derived from the operators rather than asserted:
   `lifetime_leq(L, R)` holds when `join_max(L, R) == R` and
   `scope_min(L, R) == L`.
3. **The LSP tail row gains a signal.** `check_eventing.md` prints
   `(multi, until(disconnect))`. Two scopes are live there: the connection
   (`disconnect`) and the `switch_map` on the open-document set
   (`document_closed`). Either one ends the subscription, so `scope_min` gives
   `until(any_of([disconnect, document_closed]))`. The doc's row names the
   outer scope and drops the inner one, which is the scope that actually fires
   most often.
4. **Rule mode and ask mode are different questions.** The forkJoin rule proves
   `det` (three det envelopes, conjunctively), and a keyed snapshot ask on the
   rel it heads is `semidet`, because the row may not have landed yet. The plan
   has one table and no place for this distinction
   (`fork_join_snapshot_ask_is_semidet_not_det`).
5. **The `external = shell {...}` row splits three ways**, as shell_stream.md
   already found: det envelope is `(det, finite)`, `Stream(Item, End)` is
   `(multi, finite)`, `Tail(Item)` is `(multi, never)`. The plan's single row
   ("finite per request (det: 1 next + complete)") is wrong for any process
   that writes more than one line.
6. **The effect rule gains a demand term.** Plan line 34 gives `shell` the
   lifetime "finite per request" and stops. That is only the protocol half.
   With the demand half missing, plan case 5 cannot be derived at all, because
   `fetch` would come out `finite` and the register over it with it.
7. **Two ask forms have no row in the plan.** `write` (the eventing hook's
   window row, marked "n/a" in check_eventing.md) is graded `(det, finite)`: a
   keyed write lands exactly one row and completes. `request` (a one-shot ask
   that supplies its own demand row) is what plan case 1 actually describes,
   and it is not the same form as `snapshot`.
8. **The species is a fold iterated to a fixpoint.** ARCH.pl files
   `mode_analysis` under `fold`. The rel graph is cyclic in ghcacher (`poll ->
   fetch -> cache -> cache_tag -> poll`), so a single pass cannot terminate the
   fold. Both operators are monotone and the lattice is finite for a fixed
   signal set, so iterating from the bottom converges at the least fixpoint
   (`fixpoint_converges_on_a_cyclic_program`). This is still one solver
   species, but the ARCH row should read "fold to fixpoint" or move to
   `monotone_fixpoint`.
9. **The link obligation is an inequality.** shell_stream.md states it as "a
   bind must discharge its rel's finiteness claim". Discharging is
   `lifetime_leq(bound, declared)`, not equality: a bind that ends sooner than
   claimed is safe, and the signature is an upper bound on how long the effect
   may run.
10. **A three-value verdict.** The plan mentions a CLI warning only. The
    analysis produces `ok | warn(Reason) | reject(Reason)`, because two of the
    failure cases (no bind, bind outlives its claim) are not warnings: no mode
    exists at all.

## 5. Ambiguities found (numbered)

1. **Does `keep(count(N))` interact with lifetime? Argue no, and the lab holds
   that line.** Ruling q10 makes `keep` a per-rel retention clause on Log rels,
   discharged as a tick-prefix DELETE. It bounds how much of the past the store
   holds, and says nothing about whether new rows keep arriving. Two identical
   programs differing only in `keep all` vs `keep count(100)` produce identical
   modes (`keep_bound_does_not_change_lifetime`). The one thing that would
   change this is a `keep` that could make a tail ask FAIL rather than lag (a
   subscriber falling behind the prune window), which is a delivery-guarantee
   question, not a lifetime one, and it has no home in either doc.
2. **Is `until(S1)` vs `until(S2)` ordered or incomparable?** Incomparable, and
   the lattice removes the need to decide. What remains open is presentation:
   the CLI now has to print `until(any_of([disconnect, document_closed]))`.
   Options are (a) print the formula, (b) collapse every non-constant lifetime
   to "conditional, terminates when: <signal list>", (c) print only the
   nearest-scope signal (which is what check_eventing.md's table did by hand
   and got wrong per deviation 3). This is a user call.
3. **Where does `departed/1` (ruling r4) sit?** The lab takes
   `lifetime(departed(R)) = lifetime(R)`: a departure cannot happen more often
   than the arrival that preceded it, so a departure stream cannot outlive its
   source rel, and `unwatched <+ departed(watch)` over a fact rel is `finite`
   while `cleared <+ departed(diagnostic)` over a never rel is `never`. Two
   things are unresolved. First, this is an upper bound and not tight: a rel
   whose rows arrive forever but never leave has a departure stream that is
   silent forever, and nothing in the analysis can tell those apart, so a live
   `never` gets reported where `finite` might be true. Second, r4 says only
   Set/level rels can depart, so `departed(SomeLogRel)` should be a type error
   before mode analysis ever sees it, and no lab owns that check yet.
4. **How does forkJoin's completion map to the terminal-enum reading?**
   shell_stream.md makes `finite` at `multi` cardinality mean exactly "the
   result type names a terminal enum". A conjunctive body is `finite` for a
   different reason: all its inputs are. It has no terminal constructor and
   nothing to match on, so a consumer that wants "the fork joined and will
   produce nothing more" has no arm to write. Either (a) `finite` has two
   unrelated witnesses and only one of them is observable in the program, or
   (b) a derived rel with a `finite` lifetime should acquire a synthesized
   terminal, at which point the `Stream(Item, End)` wrapper leaks out of effect
   signatures into ordinary rules. Neither doc chooses.
5. **Mercury's `nondet` has no cell.** `semidet` conjoined with `multi` is 0 or
   many, which is `nondet`, and the plan's cardinality type has three
   constructors. The lab flattens it to `multi` (`nondet_flattens_to_multi`).
   That loses "this ask can legitimately return zero rows", which is exactly
   what a commit gate wants to know. A fourth constructor, or a second boolean
   column (can-fail), would carry it.
6. **When does a rel's mode exist at all?** The lab rejects EVERY ask in an
   unlinked program, including the snapshot ones, on the grounds that an
   unbound effect anywhere in the ask's dependency cone is a program-level
   defect. The weaker reading is defensible: a snapshot is a SELECT and is
   finite whatever the binds say, so only tail and request asks need the link.
   Choosing the weaker reading means the CLI can answer snapshot asks against a
   partially linked program, which may be wanted for tooling.
7. **How is scope membership computed?** The lab takes `in_scope/3` as given
   and grades what dominance does with it. The `switch_map` sugar has to
   produce those facts, and nothing states how far the scope reaches: only the
   rel directly under the sugar, or its whole downstream cone, or its
   downstream cone up to the first rel that another subscriber also reads. The
   third reading is the only one that is sound under row sharing (rows are
   shared across subscribers, so a scope cannot delete rows another
   subscription still demands), and it makes scope membership depend on the
   whole `static_subs` graph rather than on the sugar site.
8. **One effect, several deployments.** Protocols bind per deployment, and the
   mode table is a function of the binds. So there is no such thing as "the
   mode table for this program", only one per deployment. Where the table lives
   (compiled artifact per bind file, or recomputed at ask time) is unstated,
   and it decides whether `dl` can answer "will this ask terminate" without the
   deployment's bind file in hand.
9. **`request` versus `snapshot` on an effect rel.** Plan case 1 asks `fetch`
   and gets `(det, finite)`. That is a one-shot ask supplying its own demand
   row. Asking the same rel as a tail follows the program's demand and is
   `never` under ghcacher's clock. Same rel, two modes, and the surface has one
   spelling (`? fetch(...)`). The form has to be explicit at the CLI or the
   mode is ambiguous.
10. **Does dominance apply upward?** The lab applies `scope_min` at the node
    that is declared in the scope and lets the value propagate downstream
    through `join_max`. A rel that is read by both a scoped and an unscoped
    subscriber therefore reports one lifetime, when the honest answer is one
    per subscription path. This is the same object confusion as section 3: the
    static table is per-rel, the real answer is per-path in the
    `runtime_subs` forest.

## 6. What this means for the tier order

T6 can be built as specified with the two operators substituted for the plan's
`min`, and it needs T5's bind facts present at analysis time (not T5's runtime),
which the plan's tier row already says. Three items move:

- The lifetime lattice belongs in a shared module, not in T6. `scope_min` and
  `join_max` are the same algebra the retention fold and the teardown planner
  need, and the `until` formula is the thing the CLI prints.
- The link check (`lifetime_leq(bound, declared)`) is a T5 obligation the lab
  implements, and it is the only check that can catch `tail -f` in a
  `Stream`-typed rel. It should ship with T5's bind grammar, not with T6.
- Ambiguity 7 (scope reach) blocks the `switch_map` sugar, not mode analysis.
  Mode analysis consumes `in_scope` facts and does not care where they came
  from; the sugar cannot be written until the reach rule is chosen.
