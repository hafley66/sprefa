# IMPACT: demand-driven laziness against the engine as built

Analysis lane `lab/impact-lazy`, base `feb14d8d`, worktree
`/Users/chrishafley/projects/sprefa-impact-lazy`. Every claim carries
`file:line` from that tree. Forks are priced; nothing here is a decision.

Companion: `IMPACT.fused.visual.human.unga.md` (plain words, no citations).

## 0. What is already ruled, and what this analysis may not touch

Six user rulings and two ARCH rows already fix parts of this design. They
are cited here so the sections below stay inside them.

| ruling / row | says | file:line |
|---|---|---|
| `clock_residency = world_fed_bind_not_construct` | wall-clock cadence enters as a world-fed BIND, never a new language construct; rules cannot observe time passing | `v6/prolog/conformance/rulings.pl:198-209` |
| `spine_residency = stdlib_rels_and_binds_not_kernel` | the git/fs spine is hosted in the language, never kernel | `rulings.pl:196-207` (ruling term at `:203`) |
| `subscription_kernel = minimal_with_coverage_check_and_ghost_view` | zero stored semantic rels, zero new tick phases; teardown = ordinary IVM retraction; demand-row deletion IS the abort | `rulings.pl:170-190` |
| `effect_abort = best_effort_cancel_on_support_zero` | cancellation never carries a correctness guarantee | `rulings.pl:158-168` |
| `salt_minting = content_addressed` | the salt is witness data, never a subscription id | `rulings.pl:149-156` |
| `n1_statement_budget = flat_per_tick_statement_count` | per-tick statement count is a graded conformance check | `rulings.pl:124-125` |
| `technique(laziness, demand_rows_as_clock, 'magic set = subscriber table')` | the chosen technique, recorded | `v6/prolog/ARCH.pl:261` |
| `task(demand_clocking, labbed, [kernel_sql_lowering])` | labbed, not built | `ARCH.pl:661` |
| `algorithm(magic_demand, static_subs, rewrite, 'books/v6/algos/magic_sets.pl')` | the reference transform, executable, 33 lines | `ARCH.pl:164`, `books/v6/algos/magic_sets.pl` |

Consequence for section 3: a generic external event source that arrives as a
new **construct word** contradicts `clock_residency`. Every fork in section 3
is therefore either a bind row, a decl that desugars to one, or nothing new.

## 1. Every eager assumption, enumerated

The recon established the headline (100% eager, queries are dead metadata).
This section is the itemized bill: each row is one place the engine assumes
"compute it, someone might look", with what demand does to it.

### 1.1 The tick cascade

`runIncrementalTick` runs eight phases over the program's WHOLE statement
lists on every tick, with no reference to any query
(`v6/tsv2/gen_emitted/native_ts_query_term.ts:375-388`). The phases and
their per-tick cost:

| phase | what it touches | receipt |
|---|---|---|
| `prepareTick` | 2 `DELETE`s per relation (delta + nextFrontier) | `v6/tsv2/runtime/1_incremental.ts:619-627` |
| `applyArrivals` | one grouped statement per arrival run | `1_incremental.ts:629-669` |
| `applyLevelsBeforeEdges` | every non-aggregate level statement | `1_incremental.ts:778-794` |
| `applyEdges` | every edge statement, in order | `1_incremental.ts:766-776` |
| `recomputeLevelsAfterEdges` | every level statement's 5-statement refCount reconcile | `1_incremental.ts:913-980`, `500-531` |
| `readBoundary` | one boundary select per relation | `1_incremental.ts:993` |
| `promoteFrontiers` | 3 statements per relation, unconditional | `1_incremental.ts:1082-1093` |

Two narrowings already exist and both are cone-shaped in spirit, which is
the precedent: `recomputeLevelsBeforeEdges` returns immediately on an empty
batch (`1_incremental.ts:832`), and the retraction guard skips the reconcile
when no `-1` is staged and no level body is negative
(`1_incremental.ts:852-857`, guard SQL at `:537-543`).

**Under demand:** the statement lists become cone-filtered lists. Semantics
inside the cone are untouched, because each entry is an independent record
built per rule (`v6/prolog/lower.pl:3379`, `:3398`); a shorter list is a
subset of identical statements, never an edited one.
**Migration:** filter at emission, not at runtime, so the emitted module
stays a literal readable list.
**Breaks:** the `n1_statement_budget` receipts and `memory-soak`'s
statements/tick flatness check (`v6/justfile:190-193`) both measure a number
that will drop. Dropping is the point; the gates assert flatness, not a
value, so they should survive. Verify, do not assume.

### 1.2 `sql_rule_order` strata

`stratum_groups/2` includes EVERY level rule (`v6/prolog/strat.pl:19`) and
`sql_rule_order/2` topologically orders all of them
(`strat.pl:81-84`). The stratum numbers mirror the oracle's `relax_strata`
exactly (`strat.pl:16-17`, `:56-77`), and the cap is `DerivedCount + 1`
(`strat.pl:31-32`).

**Under demand:** the order over a subset is the same order restricted, so
nothing reorders. Two second-order effects are real:

1. The cap moves with the subset. Harmless: it only bounds the relaxation
   loop before `throw(not_stratified)` (`strat.pl:74-75`).
2. Pruning a rule can remove the program's only NEGATED level body ref,
   which flips `reconcileEveryTick` from true to false
   (`1_incremental.ts:819-823` names `emit_ts.pl:reconcile_every_tick/2` as
   the source). That changes which reconcile policy runs and therefore the
   per-tick statement count and, in principle, the drain boundary.
   **This is the one place where pruning changes a policy rather than a
   list.** It needs a fixture: same program, one query cone with the
   negation and one without, tick logs graded.

### 1.3 Host-demand rows generated unconditionally

Every `probe(...)` in a rule body is split into a LEVEL RULE heading
`__host_demand_<name>` plus the joined body
(`v6/prolog/1_host_expand.pl:501-517`, the split at `:517`
`DemandRule = (DemandAtom <- DemandBody)`). That level rule runs every tick
like any other. The emitted form is an ordinary level statement with the
full refCount five-statement block
(`gen_emitted/native_ts_query_term.ts`, the `__host_demand_tree_sitter`
entry in `INCREMENTAL_LEVEL_STATEMENTS`).

Serving then does two more eager things:

- `HostRunner` boot replay reads EVERY live demand row of every plan at
  subscribe and re-runs the ones the durable witness cache does not answer
  (`v6/tsv2/serve/1_hosts.ts:534-556`, the read at `:543`).
- Live demands ride each tick's `+deltas` on each demand rel
  (`1_hosts.ts:560-572`).

**This is where laziness has teeth.** Today an undemanded cone still spawns
subprocesses. `sh` is the only surface that leaves the machine
(`registry.pl:284-332`), and its trigger is a level rule nobody asked for.

**Under demand:** demand rules are ordinary level rules, so they prune with
the cone for free. No new mechanism.
**Note the vocabulary collision, and keep it:** `__host_demand_*` is already
a magic rel by another name, with a `__support_count` refCount column on its
own table (`gen_emitted/native_ts_query_term.ts:134`). Query demand and host
demand are the same shape at two scales; a second word for the second scale
would be a second thing to keep in sync.

### 1.4 Boot statements

`boot_statements/5` seeds Initial rows and then runs
`boot_level_recompute_statements/2` over EVERY level statement, once, with
no bind params (`v6/prolog/lower.pl:3431-3436`, `:3447-3451`). Serving runs
`[...program.ddl, ...WitnessCache.ddl()]` then `BootRunner.run(seam, program.boot)`
(`v6/tsv2/serve/3_engine.ts:224-232`).

So a program's whole level closure is computed at t=0 before any arrival and
before any query. The header comment says why it exists
(`lower.pl:3422-3430`): a level view over Initial-seeded data must start at
its real t=0 rows.

**Under demand:** boot recompute must run only the cone, or not at all until
first demand. The user's own phrasing ("lazy init one global") is the second
reading. The second reading also removes boot's from-scratch cost, which is
the largest single cost in a cold serve.
**Fork, priced:**
- **B1 boot the cone at load.** Cost: one filtered `findall` in
  `boot_level_recompute_statements/2`. Keeps t=0 semantics for demanded
  rels; an undemanded rel's t=0 rows never exist.
- **B2 boot nothing; recompute on first demand.** Cost: the recompute SQL
  already exists per level statement (`recomputeSql` in every emitted
  `INCREMENTAL_LEVEL_STATEMENTS` entry), so this is a call-site move, not
  new SQL. Buys: cold serve pays zero. Costs: the first query pays the whole
  cone's from-scratch recompute inside one tick, which is a latency spike the
  10-second law will notice on a large cone.
- Recommendation: B1 first (mechanical, gate-friendly), B2 as a later step
  once a cone-cost measurement exists.

### 1.5 DDL is total

`lower_program/2` builds DDL from every RelPlan
(`lower.pl:3366-3417`), and RelPlans covers rule refs UNION declared refs
UNION seeded refs (`v6/prolog/compile.pl:148-168`). Per surviving rel that
is the base table plus delta, frontier, nextFrontier, refCount, aggregate
scope and `__pre_` tables (`lower.pl:3366-3417`,
`gen_emitted/native_ts_query_term.ts:133-193`).

**Under demand, DDL should NOT be pruned, and the reason is a ruling, not a
preference.** The module-catalog stance says the catalog is written at
compile and dynamic DDL never happens (`2026-08-03-module-catalog-ruling.md`
F1 at `:115`), and M4 says no mid-tick table creation (`:127-128`). Tables
are cheap and one-time; statements are per-tick. The honest split:

| pruned | not pruned |
|---|---|
| level / edge / boot / prepareTick / promoteFrontiers statement lists | `CREATE TABLE` |

Consequence: `prepareTick` and `promoteFrontiers` iterate `relations`, not
statements (`1_incremental.ts:619-627`, `:1082-1093`), so they need their
own cone-filtered relation list or the per-tick DELETE storm survives the
pruning. That is a separate emitted constant, and it is easy to miss.

### 1.6 Conformance fixture semantics: what does `final/2` mean under demand

The oracle's `FinalAll` is the sorted union of the whole store plus the
whole level closure (`v6/prolog/conformance/engine.pl:558-560`).
`final(Ref, Expected)` then FILTERS that union to one ref
(`engine.pl:604-608`); `deltas(Ref, Expected)` filters the tick stream the
same way (`engine.pl:609-613`); `ticks(N)` asserts the tick COUNT
(`engine.pl:614-617`).

A contract term needs correcting here. Where a contract phrase says a
`final/1` assertion "asserts the union of ALL rels", that reading is
imprecise; reality is narrower. `expectation_holds(final(Ref,...), ...)`
calls `rel_rows/3` to select one named rel out of a union the oracle always
computes in full (`engine.pl:604-608`). The "always-evaluate-everything"
half is grounded; the per-assertion scope is a filter, not an assertion of
the union. UNKNOWN as a literal description of one assertion; what is true
is that the oracle always computes every rel and each assertion then filters
to the rel it names.

Corpus census (`v6/prolog/conformance/fixtures/`, this tree):

| item | count |
|---|---|
| fixtures | 281 |
| `final(` expectations | 306 |
| `deltas(` expectations | 163 |
| `ticks(` expectations | 69 |
| `throws(` expectations | 60 |
| fixtures carrying a `query(...)` | **5** |

The five: `ghcacher_host_program_term`, `extraction_fork_callgraph`,
`extraction_fork_span_line`, `native_ts_query_term`
(`fixtures/2_hosts_wiring.pl:66,138,179,234`) and
`struct_host_output_schedule_answer_interned`
(`fixtures/4_struct_values.pl:421`). They carry them because the fixture
term can be `program(Decls, Rules, Queries)` as well as `prog(Decls, Rules)`
(`1_host_expand.pl:64-65`), so the migration needs no fixture-term change.

**The finding:** a fixture's expectation list IS its query set. `final(Ref,_)`
and `deltas(Ref,_)` name exactly the rels the fixture asks about. Under a
flipped default, 276 of 281 fixtures demand nothing and every non-empty
`final/2` goes red.

**Fork, priced:**
- **C1 compat rule: no query lines means everything is demanded.** Cost:
  zero fixture edits, whole battery stays green, laziness is opt-in. Buys
  the whole migration ladder a green floor. Costs: the eager default is now
  a permanent surface the user must later rule away, and the flip day is a
  full-corpus churn instead of 281 small ones.
- **C2 derive the demand set from the expectations.** Cost: `engine.pl` and
  the compiler disagree about where queries come from, which is a second
  source of truth for the one thing this arc is trying to make single.
  Not recommended.
- **C3 add explicit query lines to all 281.** Cost: 281 edits, each of which
  is a semantic assertion someone must get right; `ticks(N)` (69 sites) can
  move under any of them.
- **C4 F-fix-A: expectations are implicit demand roots (harness-side).**
  The conformance harness treats every `final/2` and `deltas/2` ref as a
  standing query from t=0. Zero fixture edits; a fixture keeps meaning
  exactly what it means; the demand plane is exercised by NEW fixtures that
  demand less than they compute. Cost: the harness gains a roots parameter,
  and "a fixture asserts what it demands" becomes a stated law. It carries
  the same objection already leveled at C2, in harness form: the harness
  would be a second place queries come from, one that never governs
  production. When the default flips, C4 (as the harness refinement) rides
  with C3 (as the program surface) exactly the way the fable lane composed
  them: C1 as the program default during migration, C4 as the harness
  refinement when the default flips.
- Recommendation: **C1**, with C3 applied per-fixture only where a fixture
  is deliberately promoted into the demand-graded family.

**`ticks(N)` is the sharp edge under any fork.** A drain tick is minted when
any nextFrontier or departure frontier holds a row
(`1_incremental.ts:1070-1080`); pruning a cone removes edge writes and can
therefore remove drain ticks. 69 sites are exposed.

### 1.7 The Prolog oracle's evaluate-everything loop

`run_program/5` discards QueryPlans at the first line
(`engine.pl:503-504`, the `_` in `prepare_program(SugaredProg, HostProg, _, _, _)`),
computes `level_closure` over EVERY plain level rule and every aggregate
rule (`engine.pl:526-529`), and `run_ticks/7` recomputes each tick
(`engine.pl:558-570`).

**Under demand:** the referee must apply the same transform or it stops
being a referee. Two shapes:
- **O1 the same cone, computed by the same predicate.** The cone lives in
  the shared `2_demand_cone.pl` module (section 4.3), which `engine.pl` can
  import directly. It does import `1_expansion.pl` and `1_host_expand.pl`,
  which are the shared pre-passes (`engine.pl:504-508`), so a third shared
  module is consistent with the existing door split (that precedent is
  `1_host_expand.pl` itself, shared between compiler and oracle,
  `compile.pl:88-98`).
- **O2 the magic-set rewrite as a program transform, applied before
  `check_program/1`.** Reference implementation exists and is executable
  (`books/v6/algos/magic_sets.pl`, 33 lines, three checks). This is the
  shape ARCH already names (`ARCH.pl:164` species `rewrite`).
  Buys: one transform serves both doors AND handles bound query arguments,
  because it is a rewrite over rules rather than a filter over a list.
  Costs: a rewrite changes rule identity, which changes stratum numbers,
  which changes `sql_rule_order` output, which is exactly the thing the two
  doors must agree on byte for byte.
- Recommendation: **O1 for the static cone, O2 only when bound query
  arguments arrive** (section 4's two-layer split). Doing O2 first pays the
  rewrite's cost before anything needs it.

### 1.8 TEXT_DOOR byte-identity

The gate compares the TERM door's emitted TypeScript against the TEXT
door's, byte for byte, for every fixture the term door compiles
(`v6/prolog/compile/scripts/text_door_receipt.pl:1-12`, expect 196/196/0
per `v6/justfile:45-47`).

Both doors run the same compile passes, so a cone computed inside
`program_plan/2` (`compile.pl:124-175`) changes both sides identically.
**Byte-identity survives** as long as both doors see the same query set.
They do today: the printer already emits `?` lines for a `program/3` term
(`v6/prolog/print_dl.pl:62`, `:356`), and the term door's own fixtures carry
queries in five cases (section 1.6), so the round trip has a query path
already.

**One hazard, named:** `print_dl_program_with_edb_types/7`
(`print_dl.pl:193`) synthesizes `col_type/3` decls for EDB refs with a
literal witness. If cone pruning removes a rel, its synthesized decl must be
removed on both sides or the two doors print different decl sets. This is a
receipt to write, not a defect to predict.

### 1.9 golden-flex's self-gating coverage

`golden_coverage.pl` requires every `surface/5` and `expression/5` row to
appear in `golden-flex.dl6`'s parsed TERM or its comment-stripped SOURCE
(`v6/prolog/compile/scripts/golden_coverage.pl:111-120`, `:151-155`).
`query/1` is a live row (`registry.pl:192`), which is why the golden carries
two query lines (`v6/dl/fixtures/golden-flex.dl6:467-468`) against 40 rules.

**The finding, and it is the one that would go silent:** the coverage gate
reads the SOURCE, not what runs. Under demand, golden-flex's other rules
fall outside the two query cones and stop being exercised by the twelve byte
comparisons in step 3 of `golden-flex.sh` (`v6/tsv2/scripts/golden-flex.sh:10-17`),
while the coverage gate keeps passing. The composition receipt would narrow
without failing.

**Migration:** the golden gains a query per construct family, or a second
gate asserts `cone(golden-flex) == every rel the golden declares`. The
second is cheaper and states the intent directly: THIS program is
deliberately fully demanded.

### 1.10 The store's materialization

`v6/sprefa-store/js/src/lower/` already carries a `materialization: "lazy"`
concept: a cold/lazy derived rel lowers to a cold Observable that re-runs per
subscribe (`lower/ast.ts:200-202`), and a recursive stratum is DEFERRED
unless every member is lazy IDB (`lower/lower.ts:154-170`, the test at
`:162`).

**Scope check, and it matters:** that lowering is not on the tsv2 emitted
path. The tsv2 runtime's imports from the store package are surface-level:
`runtime/scratchStore.ts:14-15` imports two bindings, `open_db` and
`SqlRunner`, and `runtime/types.ts:4` imports five types
(`ISqlRunner`, `QueryResult`, `SqliteDb`, `SqlStatement`,
`TraceStatement`). The store's own engine header says v6/dl's tick loop does
not reach its cascade (`sprefa-store/js/src/engine/engine.ts:35-38`). The
scope conclusion stands: store lowering is off the tsv2 emitted path.

**A consequence of the pull `rows(rel)` API, worth naming.** `rows(rel)`
(`v6/tsv2/serve/3_engine.ts:127-134`) reads `finalSelect[rel]` and returns
live rows for an unqueried rel today, because DDL is total (section 1.5) and
every rel gets a `finalSelect` regardless of demand. Under pruning that
meaning changes: an out-of-cone rel's table is empty, so `rows(unqueriedRel)`
returns an empty set where it once returned live rows. That is a contract
change to pin, not a defect: either keep DDL whole (only prune statements
and host demand), or extend `rows(rel)` to lazily materialize on first ask
and declare unqueried rels "not materialized" by contract. UNKNOWN which,
until the DDL-prune fork is ruled.

So: prior art worth reading, zero migration owed, and one warning. The
store's laziness is per-subscribe recompute (a cold Observable), which is
the OPPOSITE of the never-cold pole of the reset fork (section 2.3).
Copying its shape would re-cold every cone on every reader.

### 1.11 serve's binds spin regardless of demand

`runProgram$` constructs four runners unconditionally on every accepted
program (`v6/tsv2/serve/4_http.ts:152-190`, the merge at `:163-187`).
`IntervalBindRunner` starts one rxjs `interval` per LITERAL read out of the
program text (`v6/tsv2/serve/2_binds.ts:147-175`); `WatchBindRunner` starts
one OS watch plus one `git ls-files` subprocess per literal
(`2_binds.ts:386-447`, `:287-295`). A bind with no literal is honestly
silent (`2_binds.ts:174` returns `EMPTY`), which is the only demand-shaped
behavior on this seam and it is compile-time, not runtime.

**This is the user's "we are literally subscribing to everything", exactly
located.** A declared cadence spins forever whether or not any cone reads it.

**Under demand:** the bind should subscribe per demand ROW, not per literal.
That is `switchMap` over the demand rel's current rows rather than
`merge(...timers)` over `plan.literals`. It is also what makes the user's
canonical composition expressible (section 3).
**Price:** the literal path is a compile-time constant list and the demand
path is a runtime-varying set, so `IntervalBindRunner`'s constructor stops
being a constructor and becomes a function of a rows Observable. That is a
real rewrite of `2_binds.ts:139-176` and `:373-448`, roughly 80 lines, with
`extraction-live`, `files`, `watch-scale` and `memory-soak` as its gates.

### 1.12 The arrival boundary accepts anything declared

`ArrivalTargets` is every ref that is not a rule head
(`compile.pl:152-153`), and the HTTP boundary accepts a POST for any of them
with full per-column type checking (`4_http.ts:283`, `:296-302`). So the
generic external ingress ALREADY EXISTS and is already typed at the
boundary. What it lacks is a clock identity (section 3).

### 1.13 `__pre_` snapshots are full-table copies

Every `pre`-read rel gets `DELETE FROM "__pre_<rel>"` plus a full
`INSERT ... SELECT` from the base table, per tick
(`v6/prolog/emit_ts.pl:1206-1215`).

**Under demand this is a semantic hazard, not just a cost.** `pre(rel)`
means "the previous boundary". If the rel was outside the cone last tick and
inside it this tick, `__pre_rel` holds the empty set, and `pre` silently
reads absence instead of the previous value. The clock checker grades `pre`
at offset -1 (`registry.pl:213`, `3_clock_check.pl:80-85`), so the offset
math is right while the data is missing.
**Migration:** a rel that any surviving rule reads through `pre` is IN the
cone unconditionally, or the cone must be computed over the `pre` edge too.
The second is correct and is one extra edge kind in the walk.

### 1.14 The precedent worth copying: `now/1`

`program_uses_tick/2` answers one question, and the `__tick` table plus its
per-tick advance are emitted only when the answer is yes
(`v6/prolog/analyze.pl:173-185`, the comment at `:173-176`: "every other
emitted module stays byte-identical to what it was before now/1 was
lowered").

That is the shape the whole ladder should take: a compile-time question, a
conditional emission, and a stated byte-identity claim for every program the
question does not touch.

## 2. The edge-plane tension, resolved as priced options

INGRESS IS NOT EVALUATION. Applying a signed EDB arrival to its table is a
plain write; derivation starts where rules read it. The three options below
differ only in what happens to that write before first demand.

### 2.1 Why `<+` is the hard one and `<-` is not

The two planes store their inputs differently, and that difference is the
whole problem.

| plane | input | lifetime | receipt |
|---|---|---|---|
| level `<-` (B ring) | base tables | durable | `lower.pl:687-736` `rel_ddl/5` |
| edge `<+` (N ring) | this tick's delta / frontier rows | **one tick** | `1_incremental.ts:619-627` clears delta and nextFrontier at tick start; `:1082-1093` clears and refills frontier at tick end |

`TICK-MODEL.md:39-41` states the rings; `:50-65` states that the edge plane
is the DERIVATIVE of the level plane and a bare trigger atom is
`subscribe (dS)+`.

So a level rel is safely lazy by construction: its inputs are still on disk,
and the from-scratch `recomputeSql` already exists in every emitted level
statement and is already used by boot (`lower.pl:3447-3451`). Wake it up
whenever; the answer is the same.

An edge rel is not: its input window is destroyed every tick. An occurrence
that arrives while the consumer is cold is gone, permanently, with nothing
to recompute from. Under rx that is exactly a `Subject` with no subscriber,
and it is the correct semantics for a Subject. Whether it is the correct
semantics for this language is the fork.

### 2.2 Before first demand: three options

**A. Drop.**
Edge arms outside the cone never fire; pre-demand occurrences are lost.

```
# nothing new is written; the cone simply does not include this rule
commit_seen(sha) <+ pre_commit(_repo, sha, _at_ms).
```
rx lowering: `preCommit$` is a bare `Subject`; with no subscriber, `next()`
drops on the floor.

- Price: zero. No new storage, no new decl, no new gate.
- Conformance: every `deltas(Ref, PerTick)` expectation (163 sites) reads as
  "deltas since first demand". Under the compat rule (C1) that is
  vacuously unchanged; under a flipped default it is a corpus-wide
  re-grading.
- Pre-commit example: a commit landing before the first query is invisible.
  For a git hook that is usually wrong, because the hook fires exactly once
  and the user cares about that firing.

**B. Buffer at ingress.**
Queue arrivals per EDB rel until first demand, then replay.

```
# no dl spelling exists; this option INVENTS one
rel pre_commit(repo: text, sha: text, at_ms: int) buffer(until_demand).
```
rx lowering: `new ReplaySubject(Infinity)` in place of `Subject`.

- Price: unbounded memory unless bounded; a new decl word; a new runtime
  structure above the SQL seam, which is the one place this repo does not
  keep state.
- Tick math breaks: a replayed occurrence arrives at a later tick than it
  happened, so `now(Tick)` (`analyze.pl:163-171`) and every tick-derived
  value shift. The clock checker's offsets stay right while the tick
  NUMBERS lie.
- **Build-vs-buy verdict: this is buying a second retention mechanism when
  the language already has one.** See C.

**C. Persist through the store, and let `keep(...)` be the buffer.**
Arrivals land in base tables whether or not anything is demanded (they
already do: `applyArrivals` writes to the base table via
`arrivalAddSql`/`arrivalDelSql`, `1_incremental.ts:629-669`). On first
demand, the cone's `recomputeSql` builds level state from whatever
accumulated.

For a `log`-kind rel this preserves the occurrences, because a log rel's
TABLE is its occurrence history, appended, bounded by its own `keep(...)`
(`analyze.pl:56-57` `decl_keep/3`; `lower.pl:3399` `retention_statements`;
ruling `q10_retention = keep_clause_required_on_log` at `rulings.pl:60-65`).
`TICK-MODEL.md:136-155` states the distinction precisely: the occurrence is
in N and can never be minus; the STORED WINDOW is in Z at the boundary and
`keep(...)` alone reclaims it.

```
rel pre_commit(repo: text, sha: text, at_ms: int) log keep(count(64)).
commit_seen(sha) <+ pre_commit(_repo, sha, _at_ms).
```
rx lowering: `new ReplaySubject(64)`, where the 64 is the program's own
declared bound rather than a runtime policy.

- Price: the replay-on-wake path must read the log table's retained rows and
  stage them as this tick's frontier, which is one new emitted statement per
  waking log rel. No new decl word. No new storage. No new memory.
- For a `set`-kind rel only the latest state survives, which is what `set`
  means, so C degrades to A for set rels and that degradation is honest.
- Conformance: `deltas` for a log rel replayed on wake are the SAME
  occurrences at LATER ticks, which is B's tick-shift problem in a bounded
  form. The bound is program-declared, so the shift is stated rather than
  emergent.

**Recommendation: C, with A as the stated behavior for set-kind rels and B
rejected.** B is a bespoke queue for a problem the language already solved
with `keep(...)`; the build-vs-buy law applies to the language's own
mechanisms as much as to libraries.

### 2.3 After first demand: the reset fork, and the defect it names

The user ruled share-with-no-reset ONLY as the shape of the worked pre-commit
example. It was never a global default for demanded sources in general, and
this document must not read it as one. Reset behavior for demanded sources
is an OPEN FORK, unruled:

- **never-reset** (warm forever): a demanded rel stays subscribed once warm,
  the read of section 3's example.
- **rx-default reset-on-refcount-zero** (cold on last reader): the rxjs
  default, re-cold every cone on every reader.
- **per-rel declaration**: the author states liveness per rel.

No recommendation here. When a design needs to pin it, the mechanism is a
field already in sight: `snapshot(current)` today has one value
(`emit_ts.pl:419-422`); a `snapshot(standing)` alternative would be the
field the choice travels in, a one-field widening of `IQueryPlanData`
(`emit_ts.pl:288`).

**What the fork is NOT deciding: the `share()` defect, which stands on its
own merits.** `LiveEngine.ticks$` uses bare `share()` (`v6/tsv2/serve/3_engine.ts:112`).
Under rxjs 7 (`v6/tsv2/package.json:26`, `"rxjs": "^7.8.2"`) bare `share()`
defaults to `resetOnRefCountZero: true`. When the last subscriber leaves,
the `tap` finalize at `3_engine.ts:108-110` flips `running` false, and the
next `submit` errors with "tsv2 engine is not running"
(`3_engine.ts:116-124`). That is a reset-on-refcount-zero sitting in the
exact place the worked example's never-cold shape must not have one. Today
it is masked because `runProgram$` holds a permanent inner subscription
(`4_http.ts:164`); under a query-as-subscribe design it stops being masked.
The comment at `3_engine.ts:180-193` records a prior measured outage through
this state (the whole served process went ECONNREFUSED). The fix direction
is one line (`share({ resetOnRefCountZero: false })`) plus a test that drops
every reader and submits again, but whether that direction is the settled
answer is the ladder step 5 fork, not a foregone conclusion (section 6).

### 2.4 What each option does to the pre-commit example

| option | commit before first query | commit after first query | second commit |
|---|---|---|---|
| A drop | invisible | fires | fires |
| B buffer | fires late, at a shifted tick | fires | fires |
| C log + keep | fires late, bounded by `keep` | fires | fires |

The "second commit" column is the same in all three, and it is NOT free:
section 3.3 shows why it is a separate hazard that lives in the composition,
not in the ingress.

## 3. Generic event ingress as a language construct

### 3.1 What exists, precisely

| seam | who may declare | receipt |
|---|---|---|
| `sh` host | any program, any name, typed inputs and outputs | `registry.pl:189`, `1_host_expand.pl:174-192` |
| `bind` | **only `interval` and `watch`** | `registry.pl:275-276`; `1_host_expand.pl:388-391` throws `bind_mismatch` for any other name |
| `POST /arrivals` | any non-derived ref, typed at the boundary | `compile.pl:152-153`, `4_http.ts:283`, `:296-302` |

So the ingress transport is already generic and already typed. Two things
are missing, and only two:

1. **A declared clock identity.** `clock_origin/3` picks ANY node with no
   incoming causal dependency (`3_clock_check.pl:192-195`). An EDB rel is an
   origin by accident. Nothing in the program says "this rel is fed by the
   git pre-commit hook and that is its clock".
2. **A declared liveness.** `snapshot(current)` is a field with exactly one
   possible value (`emit_ts.pl:419-422`, `1_host_expand.pl:404-410`). There
   is no way to say standing.

### 3.2 Three forks for the decl surface

**Fork A: one generic `event` bind row.**

```prolog
% registry.pl, beside interval and watch
bind_definition(event, [col(topic, text), col(seq, int), col(payload, json)]).
bind_executor(event, live_event).
```
```
bind event(topic: text, seq: int, payload: json).

rel pre_commit(repo: text, sha: text, at_ms: int).
pre_commit(repo, sha, at_ms) <-
    event("git.pre_commit", _seq, payload),
    decode(payload, {repo: repo: text, sha: sha: text, at: at_ms: int}).
```
rx lowering:
```
const event$ = pushTopic("git.pre_commit");            // one route, one Subject
const preCommit$ = event$.pipe(map(decodePayload));    // decode/2 IS the map
```
- Price: two registry rows, one runner beside `IntervalBindRunner`, one HTTP
  route, one `bindPlansFor` executor name (`2_binds.ts:456`). Zero compiler
  work: `decode/2` is already live and already lowers to a dictionary JOIN
  (`registry.pl:83`).
- Consistent with `clock_residency` (it is a bind).
- Buys: generic ingress with in-language typing.
- Costs: typing is per-use-site, not per-source. A shape mistake is a decode
  miss at runtime, not a load-time refusal. The clock identity is still
  accidental, because the origin is `event/3` and every topic shares it.
  That last point is the one that matters: **fork A does not solve the
  problem the user named.**

**Fork B: a typed source declaration, `sh`-shaped, no template.**

```
source pre_commit(repo: text, sha: text, at_ms: int) @ clock(git_hook).

rel commit_seen(sha: text) log keep(count(64)).
commit_seen(sha) <+ pre_commit(_repo, sha, _at_ms).
```
rx lowering:
```
const preCommit$ = declaredSource("pre_commit");       // typed at the boundary
const commitSeen$ = preCommit$.pipe(map(row => [row.sha]));   // <+ = subscribe (dS)+
```
- Price: one `surface/5` row (`source_decl/3` or similar), parser work in
  `parse_dl.pl` beside `bind` (`parse_dl.pl:872`), a `golden_coverage.pl`
  row plus a golden-flex use (`golden_coverage.pl:111-120`), SYNTAX.md
  regeneration, a named refusal for a malformed push, and a `clock_role/4`
  row if the source carries a grade of its own.
  Estimate: 60-100 lines across four files plus receipts.
- Buys: load-time typing, a named refusal, and a DECLARED clock origin, which
  is the thing forks A and C cannot give.
- Costs: a new construct word. That is in tension with `clock_residency`'s
  "never a new language construct" for CADENCE. The tension is arguable
  either way: a pre-commit hook is not a cadence, it is a typed external
  event, and the ruling's stated target is wall-clock time. **Do not resolve
  this here.** It is a user call, and it is the single most consequential
  question in this document.

**Fork B': B as pure sugar over A.**
`source` is a term-expansion producing the `bind event(...)` decl plus the
decode rule plus a `clock_origin` decl fact. Nothing new reaches the engine
core, which is exactly the module-catalog stance 10 extension surface
(`2026-08-03-module-catalog-ruling.md:100-106` names block-under-rel as that
surface, and this is the same move one level out).
- Price: the parser work from B minus the engine work; the desugar is one
  clause in the expansion phase table (`v6/prolog/1_expansion.pl`).
- Buys: B's authoring surface at A's engine cost.
- Costs: the clock origin is then a synthesized decl fact rather than a
  first-class node, so the checker reads it from `Decls` instead of from the
  dependency graph. That is how `keyed/2` and `kind/2` already work
  (`3_clock_check.pl:306-315`), so it is in-idiom.

**Fork C: declare nothing.**
Keep using bare EDB rels and `POST /arrivals` as `v5-git-diags.dl6` does
today (`v6/dl/fixtures/v5-git-diags.dl6:129-130`, `want_at` and `base_at`
are exactly this).
- Price: zero.
- Buys: nothing. The clock identity gap stays open, which is the gap the
  user's words describe.

**Recommendation: B', with the B-vs-B' choice deferred to whoever owns the
`clock_residency` boundary.** B' is the only fork that gives a declared
clock origin without putting a second event mechanism into the engine.

One further point against reaching for a new construct word at all: the
language already shed this exact surface once. `LANG.md:15-16` records that
`external` and `register` died, and `bind` is the unbundled survivor. Any
Fork B-style `source` functor that does not desugar back to a bind row is
reintroducing a construct the language already had grounds to unbundle; the
four-keyword surface (enum, struct, rel, bind) is a ruling-backed boundary.
This does not decide fork B versus B' (that stays the `clock_residency`
call), but it tilts the weight of the new-keyword cost.

### 3.3 The worked composition: three spellings, and what the checker does to each

The shape the user wrote: first `pre_commit`, then switch to a merge of
`interval(1000)` and the pre-commit edge again, shared with no reset. Three
spellings exist, and they do not grade alike. `.audit-scratch/adjudicate.pl`
and `.audit-scratch/sample.pl` re-run all three through the real checker
(`check_clock_program/1`).

**Spelling A: level-plane accumulate (this analysis lane's program).**
The merge is two `<-` rules on one head; `scan_due` is a level rel whose
inputs are on disk, so the direct pre-commit leg and the interval leg are
both durable reads.

```
# the gate opens on the first pre-commit and never closes (set kind, so a
# second identical write is silent -- ruling r_equal_row_write = noop)
rel scan_gate(repo: text).
scan_gate(repo) <+ pre_commit(repo, _sha, _at_ms).

# after the gate, BOTH legs of the merge reach scan_due
rel scan_due(repo: text, bucket: int).
scan_due(repo, bucket) <- scan_gate(repo), interval(1, bucket).
scan_due(repo, bucket) <- scan_gate(repo), pre_commit(repo, _sha, at_ms),
                          bucket := at_ms / 1000.

? scan_due(_Repo, _Bucket).
```
rx lowering:
```
const scanDue$ = preCommit$.pipe(
  take(1),                                              // "first pre_commit"
  switchMap(() => merge(interval(1000), preCommit$)),    // the merge after it
  share(),                                               // liveness: the reset fork
);
```
**Verdict: compiles clean.** `clock_origin/3` returns two origins
(`pre_commit/3` and `interval/2`, `3_clock_check.pl:192-195`); from origin
`pre_commit`, `scan_due` is reachable at offset 0 by two paths, so
`clock_path_conflict` (`3_clock_check.pl:328-339`) does not fire. This is
the good news and it is worth stating: the composition is already
expressible in the level plane. Cost: `scan_due` is a growing maintained
level view, and the pulse is its delta stream; the set-kind gate rel
(`scan_gate`) swallows a second identical write by design
(`rulings.pl:69` `r_equal_row_write = noop`).

**Spelling B: edge arms, bare atoms (the fable lane's program).**

```
bind pre_commit(repo: text, head_digest: text).
bind interval(period: int, bucket: int).

rel armed(repo: text).
armed(repo) <+ pre_commit(repo, _head_digest).

rel gate_fire(repo: text, bucket: int).
gate_fire(repo, bucket) <+ interval(1, bucket), armed(repo).
gate_fire(repo, bucket) <+ pre_commit(repo, _head_digest), armed(repo), interval(1, bucket).

? gate_fire(repo, bucket).
```
**Verdict: REFUSED.** The checker throws
`clock_path_conflict(pre_commit, gate_fire, 0, 1)`: two offsets into one
head from one origin. The mechanism, from `3_clock_check.pl:129-138`: in a
finalize-free edge arm every bare atom is a trigger whose grade is 1 iff the
source rel is edge-headed; the BARE READ of the edge-headed latch (`armed`)
IS the +1, while the direct `pre_commit` read is the offset-0 path. Two
offsets into one relation from one origin is exactly the existing
`clock_path_conflict` refusal. This is the spelling the kill list warns
about, and only this spelling: the refusal is specific to the bare-atom
edge-arm shape, not to the composition in general.

**Spelling C: edge arms, the latch read as a `latest/1` sample.**

```
bind pre_commit(repo: text, head_digest: text).
bind interval(period: int, bucket: int).

rel armed(repo: text).
armed(repo) <+ pre_commit(repo, _head_digest).

rel gate_fire(repo: text, bucket: int).
gate_fire(repo, bucket) <+ interval(1, bucket), latest(armed(repo)).
gate_fire(repo, bucket) <+ pre_commit(repo, _head_digest), latest(armed(repo)), interval(1, bucket).

? gate_fire(repo, bucket).
```
**Verdict: compiles clean.** Reading the latch as `latest/1` makes the
sample an `edge_sample` read of current state instead of a bare trigger, so
it no longer carries the +1; the two paths to the head grade alike. `sample.pl`
in the audit scratch demonstrates this exact spelling passing.

**Two distinct hazards, on separate fork rows.**

- **(a) The silent latch (this lane's D-fork).** In spelling A, delete the
  direct `pre_commit` leg of the merge (or spell the gate as a set that only
  ever arms once), and the shape silently becomes "first one only, forever":
  the set-kind latch swallows a second pre-commit, the program compiles, and
  nothing complains. Fork, priced on this row:
  - **D1 a label** `not_provable(second_event_refires(Head, Origin))`, in
    the idiom of the two existing labels (`3_clock_check.pl:349-364`
    `multi_trigger_batch_invariance`, `:386-393`
    `arm_absence_batch_invariance`). Price: ~20 lines in `3_clock_check.pl`,
    one row in the replay gate (`compile/test/3_clock_history.pl`, which the
    TICK-MODEL calls the completeness evidence, `TICK-MODEL.md:26-30`), one
    plunit test. Buys: the hazard is named and queryable and never silent.
    Costs: it does not stop the program.
  - **D2 a refusal.** Price: the same code, plus the risk of rejecting a
    program that WANTS first-only semantics (`exhaustMap` is a legitimate
    shape and the ruling vocabulary has no other spelling for it). Both
    existing batch-invariance facts were labels for exactly this reason
    (`TICK-MODEL.md:20-24`: both shapes appear in ruled programs).
  - Recommendation: **D1 now, D2 only after a corpus measurement shows zero
    legitimate first-only programs.** That measurement is one grep over the
    31 `.dl6` files plus 281 fixtures.
- **(b) Two offsets into one head (fable's C-fork, spelling B).** Refused
  today by the checker. The checker already emits
  `not_provable(multi_trigger_batch_invariance(...))` on that arm, so the
  refusal is not silent. This is the fork the fable spelling opens: whether
  the arm-with-sample shape deserves a label (like the existing batch
  invariance facts) rather than a refusal, and which offset the ARMING
  occurrence fires on. UNKNOWN which; it needs the user's word.

**Liveness of the shared inner.** "No reset" in the worked example is a
consequence of the reset fork above, and the compiler CAN pin whichever pole
is chosen in the plan: `snapshot(current)` today has one value
(`emit_ts.pl:419-422`); a `snapshot(standing)` alternative would be the
field that carries the choice into the emitted module, where the runtime can
honor it. That is the same one-field widening of `IQueryPlanData`
(`emit_ts.pl:288`) named in section 2.3.

## 4. Demand closure mechanics

### 4.1 Two layers, and they must not be confused

| layer | binds at | prunes | mechanism | ruling that constrains it |
|---|---|---|---|---|
| static cone | compile | statements, boot, bind literals, host decls | reverse reachability over the rule graph | `2026-08-03-module-catalog-ruling.md:115` F1: catalog at compile, dyn-DDL never |
| demand keys | runtime | ROWS inside a surviving rel | magic-set rewrite / demand rows | same ruling M4 at `:127-128`: no mid-tick table creation |

The static layer creates tables; the dynamic layer only ever adds rows to
tables that already exist. That is not a preference, it is what F1 and M4
already say, and it is also exactly what `__host_demand_*` already does:
a static table (`gen_emitted/native_ts_query_term.ts:134`) whose ROWS are
the runtime demand.

### 4.2 What prunes statically, what does not

| statically prunable | needs runtime demand |
|---|---|
| a rel outside every query's reverse cone | a rel inside the cone whose useful rows are one key slice |
| a `sh` decl no cone reaches (its subprocess never spawns) | a host whose demand rows depend on a bound query argument |
| a bind literal no cone reaches | a bind whose cadence should stop when the last demand key leaves |
| boot recompute for a pruned rel | none |
| `CREATE TABLE` | **never pruned, by F1/M4** |

### 4.3 Where the cone computation lives

The contract asks whether `analyze.pl` is the natural host. Half-right, and
the correction is worth making because the recon's third bullet cites
"`analyze.pl:124-175` (`program_plan` rel/rule collection)" and
`program_plan/2` is in `compile.pl`, not `analyze.pl` (`compile.pl:18`
exports it, `compile.pl:124-175` is the clause; `analyze.pl` mentions it
only in comments at `:197` and `:1059`). The line numbers are right, the
file is wrong.

The resolution is a new SHARED module, not a placement inside `analyze.pl`.
The reason is section 1.7: the oracle must run the identical cone or
byte-identity dies on the first pruned program, and the oracle imports
`analyze.pl` only indirectly. The precedent is exact: `1_host_expand.pl` is
one file both compiler doors and the oracle consume
(`compile.pl:88-98`). So the cone lives in `v6/prolog/2_demand_cone.pl`
(name per the phase-numbered convention), exporting `demand_cone/4` and
`prune_to_cone/3`, and both `compile.pl:program_plan/2` and
`engine.pl:run_program/5` call it.

Inside it, the reachability primitives are already bought, and they sit in
`analyze.pl`: `program_refs/2` at `:231`, `derived_refs/2` at `:80`,
`body_ref_uses/2` at `:104-107` (positive, negated, sampled, and `pre` reads
all count -- a negation or a `pre` read is still a read), plus
`edge_headed_refs/2` and `level_headed_refs/2` at `:8-13`. The graph walk
itself is `0_graph.pl`: `graph_from_edges/3` (`0_graph.pl:41-44`) and a
reachability walk (`collect_reachable/6` at `0_graph.pl:170-181`). Build the
edge list REVERSED (body ref -> head ref) and the cone is one
`collect_reachable` seeded with the query rels. `3_clock_check.pl:247-254`
already builds an edge list this way; copy that shape. One extra edge kind
the plain body-ref walk cannot see: the host pairing
`__host_response_N` implies `__host_demand_N` (section 1.3), so the
response-to-demand edge must be added explicitly.

- **The call site is `compile.pl:program_plan/2`**, one line after the refs
  union at `compile.pl:148-151` and before `RelPlans` is built at
  `compile.pl:162-168`. Narrow `AllRefs` there and everything downstream
  narrows for free, because DDL, level statements, edge statements and boot
  all read `RelPlans` / `RuleOrder` out of the same `plan/6`
  (`lower.pl:3359-3418`).
- **The oracle call site is `engine.pl:run_program/5`**, before `split_rules`
  (`engine.pl:526-527`). QueryPlans stop being discarded at both sites
  (`compile.pl:106`, `engine.pl:504`).

Sketch, signatures first per the planning protocol:

```prolog
%% demand_cone(+Decls, +Rules, +QueryPlans, -ConeRefs) is det.
%  Every ref that can contribute a row to some ref in a query plan.
%  Body: edges are BodyRef-HeadRef reversed to HeadRef-BodyRef, seeded with
%  the query refs, closed by 0_graph:collect_reachable/6. Edge kinds included:
%  positive uses, negated uses, latest/pre/finalize wrapper refs (a `pre`
%  read is a cone edge -- see section 1.13), every probe's generated demand
%  and response ref, and the __host_response_N => __host_demand_N pairing.
demand_cone(Decls, Rules, QueryPlans, ConeRefs).
```
Instance lifetime: none, it is a pure function of the expanded program,
computed once per compile inside `program_plan/2` and once per run inside
`engine.pl:run_program/5`.
Storage layout: none at runtime; the cone is a compile-time list that
becomes a shorter emitted statement array.
Uniqueness: `sort/2` on the result, like every other ref list in
`compile.pl:151`.

Estimated size: ~15 lines in `2_demand_cone.pl`, ~4 in `compile.pl`, one call
in `engine.pl`.

### 4.4 Imports on the same plane

The module-catalog ruling already puts imports here: "Import = demand for a
module instance" and "referencing anything in a module IS the demand"
(`2026-08-03-module-catalog-ruling.md:13-14`, stance 4 at `:44-45`), with
"unreferenced = never lowered" (M4 at `:126-128`) and module args as demand
keys (M1 at `:117-120`).

Under the cone computation that is automatic and needs no second mechanism:
a module is a rel/0 with children (stance 7 at `:48-51`), a reference is an
ordinary body ref, and the cone walk reaches it exactly as it reaches any
other rel. Eagerness is then a standing `?` line inside the module and
nothing else.

This is a design consequence, not a receipt: the catalog does not exist in
this tree (`grep __catalog_` over `v6/prolog` and `v6/tsv2` returns
nothing). Stated so nobody later reads it as measured.

## 5. What does not change

Each claim with its receipt, and the two claims that are conditional are
marked as such. The byte-identity invariant is per surviving STATEMENT, not
per module: pruning removes whole statement entries from a generated list,
never edits one, so each statement that survives is byte-identical while the
emitted module as a whole necessarily shrinks.

| survives | why | receipt |
|---|---|---|
| the two arrows' semantics inside a demanded cone | every statement is built per rule and pruning removes whole entries, never edits one | `lower.pl:1472-1511` (one edgestmt group per rule), `lower.pl:3398` (`level_statement_groups` maps over `RuleOrder`) |
| the store schema for a surviving rel | `rel_ddl/5` is a pure function of one relplan | `lower.pl:687-736` |
| `keep(...)` retention for a surviving rel | retention statements are per-relplan | `lower.pl:3399`, `1_incremental.ts:545-570` |
| the arrival boundary's typing | validates against `relColumns` / `relColumnTypes`, which survive for any rel with a table | `4_http.ts:265-307` |
| the tick log LINE FORMAT | the emitter is per-delta and format-only | `v6/tsv2/runtime/ticklog.ts`, `3_engine.ts:163` |
| refCount arithmetic | the five-statement reconcile is per level statement | `1_incremental.ts:500-531` |
| every `throws(...)` fixture (60 sites) | refusals fire in `check_supported_subset_expanded/1` and `check_clock_program/1`, both BEFORE any cone would be applied | `compile.pl:134-135` |
| TEXT_DOOR byte-identity | both doors run the same passes; the cone is inside `program_plan/2` which both reach | `text_door_receipt.pl:1-12`, `compile.pl:124-175` |

**Conditional claim 1: "the non-query pipeline is left as-is".** The recon's
closing line says the smallest change set "changes only the query path". That
is TRUE only under the compat rule (C1 in section 1.6): no query lines means
everything is demanded. Under any other default it is false for all 281
fixtures and 18 of 31 `.dl6` files. State the rule when you state the claim.

**Conditional claim 2: "tick logs stay byte-identical".** False for any
program that actually prunes. The tick log reports every rel with a delta
(`1_incremental.ts:993` reads the boundary for every relation passed
in), so a narrower relation list is a narrower log. golden-flex step 3 does
twelve byte comparisons of tick logs (`golden-flex.sh:10-17`), so this is
the gate that will catch it first. The FORMAT contract holds; the bytes do
not.

**Not surviving, named:** `ticks(N)` (69 fixture sites) is exposed under any
pruning, because a drain tick is minted from frontier occupancy
(`1_incremental.ts:1070-1080`) and pruning removes edge writes.

## 6. Migration ladder

Each step is landable alone, each names its gate, each is expected to leave
`just green-all` (31 legs, `v6/justfile:375-376`) green.

**Step 0. Carry the query's columns through emission.** No behavior change.
`compile_query/2` already builds `columns(Args)`
(`1_host_expand.pl:404-410`); `world_plan_lines/2` drops them
(`emit_ts.pl:310-314`). Widen `IQueryPlanData` (`emit_ts.pl:288`) with
`columns` and a `bound` list saying which positions are ground, and widen
`query_plan_json/2` (`emit_ts.pl:419-422`).
Gate: `text-door` 196/196/0; `sweep` replay identical.
Price, stated honestly: the five query-bearing fixtures and the 13
query-bearing `.dl6` files re-emit with a wider `queryPlans` literal, so
their checked-in `gen_emitted/*.ts` change by one line each. `gen_emitted`
is regenerated by the sweep and graded behaviorally, not byte-diffed against
HEAD (`v6/tsv2/scripts/sweep.sh:59-77`), so this is git churn, not a red
gate.

**Step 1. Compute the cone; consume nothing.** `2_demand_cone.pl:demand_cone/4`
plus the calls in `compile.pl:program_plan/2` and `engine.pl:run_program/5`,
emitting a `demandedRels` constant. Under the compat rule the constant equals
every rel when the program has no query.
Gate: a new plunit test asserting (a) `cone == all rels` for a zero-query
program, (b) the hand-computed cone for `golden-flex.dl6`, (c) the cone
includes every `pre`-read rel (section 1.13). Battery otherwise untouched,
because nothing reads the constant yet.

**Step 2. Prune statements behind a flag.** Filter
`INCREMENTAL_LEVEL_STATEMENTS`, `INCREMENTAL_EDGE_STATEMENTS`,
`INCREMENTAL_RELATIONS` and `boot` to the cone under an env switch, in the
same shape as the existing emitter-mode split
(`gen_emitted/native_ts_query_term.ts:372`).
Gate: `golden-flex` in both emitter modes plus both demand modes; `sweep`
run twice.
Price: a second axis on an already two-mode emitter is four combinations to
keep green. That cost is real and recurring; the alternative (flip without a
flag) has no bisection path when a leg reddens.

**Step 3. Oracle parity.** The same cone in `engine.pl:run_program/5`
(`engine.pl:503-531`), so the referee agrees. The conformance harness passes
every expectation ref as a demand root (section 1.6, C4), so the 281
fixtures keep their meaning verbatim.
Gate: `conformance` 281/0 unchanged under the compat rule, plus a new
fixture family asserting an undemanded rel is EMPTY and a demanded one is
not. Promote 2-3 fixtures, do not convert the corpus.

**Step 4. `prepareTick` and `promoteFrontiers` relation lists.** These
iterate relations, not statements (`1_incremental.ts:619-627`,
`:1082-1093`), so they need the cone-filtered relation list explicitly or
the per-tick DELETE storm survives step 2 entirely.
Gate: `memory-soak` statements/tick, which is the receipt that would
otherwise show no improvement and make step 2 look like it did nothing.

**Step 5. The `share()` running state.** The bare `share()` at
`3_engine.ts:112` plus the upstream finalize at `:104-111` flips `running`
false when the last subscriber leaves, so `submit()` (`:116-124`) errors
until re-subscribe; masked today by the permanent subscription at
`4_http.ts:164`; the comment at `3_engine.ts:180-193` records a prior
measured outage through this state. This step does NOT settle the reset
fork. It presents the narrow question behind the defect: **should `running`
depend on the `ticks$` subscription at all?** If it should not, the fix is
decoupling `running` from refcount; if it should, the reset pole is
implicitly chosen. Write the test (drop every `ticks$` subscriber, then
submit) either way.
Gate: `tsv2-test`, `serve-leak-soak`. One line of code, independent of every
other step; it can land first.

**Step 6. Binds subscribe per demand row.** Rewrite `IntervalBindRunner`
(`2_binds.ts:139-176`) and `WatchBindRunner` (`2_binds.ts:373-448`) to take
a demand-rows Observable instead of `plan.literals`.
Gate: `extraction-live`, `files`, `watch-scale`, `memory-soak`.
Price: ~80 lines and the seam most likely to leak an OS handle. It is also
the step that makes the user's composition real, so it cannot be skipped,
only sequenced late.

**Step 7. Generic event ingress** (section 3, fork B' recommended).
Gate: `golden-flex` coverage (a new registry row fails it by name until the
golden flexes it, `golden_coverage.pl:151-180`), `text-door`, plus a
pre-commit rail modelled on `precommit-changed` (`v6/justfile:294`), which
already drives a real four-commit repository through the served engine.

**Step 8. The second-event hazard labels** (section 3.3, the D-fork label).
Gate: `plunit`, plus a row in `compile/test/3_clock_history.pl` with the
FIXED twin that must not carry the label (`TICK-MODEL.md:26-30`).

**Sequencing overlay.** Steps 0-1 are individually trivial to revert: they
carry columns and compute a cone nobody consumes yet. Steps 2-3 are the
semantic commit: pruning activates and the oracle locks the new meaning.
Steps 4-7 are where the machine quiets: the relation lists stop the DELETE
storm, the `share()` running state is nailed, binds and hosts start only on
demand, and the ingress lands. Step 8 is where the checker closes the last
hole.

### Which fixtures need a standing query added

Under the compat rule (recommended): **none**. That is the rule's entire
purpose.

Under a flipped default, the exact bill:

| corpus | needs a query | already has one |
|---|---|---|
| conformance fixtures | 276 | 5 (`2_hosts_wiring.pl:66,138,179,234`; `4_struct_values.pl:421`) |
| `v6/dl/fixtures/*.dl6` | 18 | 13 |
| `v6/tsv2/goldens/**/*.dl6` | 0 measured | 2 |

The 18 `.dl6` files with zero `?` lines: `0_extraction-clock-golden`,
`1_rtkq-extraction-golden`, `clock-swr-demo`, `comment-suppress-rail`,
`conformance`, `crawl_org`, `devlog`, `diag-rail`, `door-handwritten`,
`extraction-live`, `files-hosts`, `norm-route`, `scip-families`,
`served-endurance`, `served-host-clock`, `served-json-projection`,
`served-watch-rail`, `sg-rail`.

Several of these are RAILS whose whole output is a side effect or a served
read, so the query to add is not obvious from the file: `served-watch-rail`
and `extraction-live` exist to prove a seam turns, not to answer a question.
Those are the ones to look at first if the default is ever flipped, because
they are where "what does this program demand" has no honest answer yet.

## 7. Open questions this analysis did not resolve

1. **Does a typed external event source count as a "new language construct"
   under `clock_residency`?** Section 3, fork B vs B'. User call.
2. **Compat rule or flipped default?** Section 1.6, C1 vs C3. User call, and
   it sets the price of every other step.
3. **Second-event hazard: label or refusal?** Section 3.3, D1 vs D2.
   Recommended D1, but the measurement that would justify D2 is one grep and
   has not been run.
4. **Does `reconcileEveryTick` flipping under pruning change any observable
   behavior, or only statement counts?** Section 1.2. Needs a fixture, not
   an argument.
5. **Log-rel wake replay: which tick do replayed occurrences carry?** Section
   2.2 option C. The bound is program-declared; the tick number is not.
6. **Reset behavior for demanded sources in general** (never-reset vs
   rx-default reset-on-refcount-zero vs per-rel declaration). Section 2.3.
   Open fork, unruled, no recommendation.
7. **The arm-with-sample shape (spelling B) and which offset the arming
   occurrence fires on.** Section 3.3, hazard (b). UNKNOWN; needs the user's
   word.
