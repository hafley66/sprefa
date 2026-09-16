# IMPACT: demand-driven laziness against the engine as built

Branch lab/impact-lazy at feb14d8d. Receipts are `file:line` into this worktree
(`v6/` paths relative to root). Every claim below is grounded in a receipt; where
reality does not support a contract term, that term is marked UNKNOWN rather than
guessed.

## 0. Merge preflight

`git merge --ff-only feb14d8d` returned `Already up to date` (exit 0). The branch
head is feb14d8d itself. No DEVIATION.md written.

## 1. Every eager assumption in the codebase

The recon establishes the system is 100% eager and that a query line lowers to
inert metadata whose `queryPlans` is never read by the runtime. This section
walks each eager surface the contract names and, for each, answers: does a
demand-driven evaluation break it, change its meaning, or leave it alone, and
what is the migration.

### 1.1 The tick cascade (every rule, every tick)

- `v6/tsv2/gen_emitted/native_ts_query_term.ts:376-387` `runIncrementalTick`
  applies `INCREMENTAL_LEVEL_STATEMENTS` (one entry per rule, every rule in the
  program) then `INCREMENTAL_EDGE_STATEMENTS` then
  `recomputeLevelsAfterEdges` on every tick.
- `v6/tsv2/gen_emitted/native_ts_query_term.ts:390-396` `runTick` is the only
  entry; the naive path is taken only when
  `EMITTER_MODE === "naive" || !INCREMENTAL_PROGRAM_SAFE`.
- `v6/tsv2/serve/3_engine.ts:188-208` every pushed batch runs
  `this.program.tick(this.seam, arrivals)` over all levels; `3_engine.ts:191`.
- `v6/prolog/conformance/engine.pl:529` `run_program` calls
  `level_closure(PlainLevel, AggRules, ...)` over every rule, then `run_ticks`
  recomputes every tick (`engine.pl:558-575`).

Meaning under demand: a demand-driven engine computes only the rels in each
query cone, so the cascade must be pruned from "every rule" to "rules in the
union of query cones." This **changes meaning**: the emitted program's tick is
smaller and its emitted modules are different (fewer statements), which breaks
byte-identity goldens (1.5) unless the fixtures that depend on them add standing
queries that keep their cones covering the whole program.

Migration: prune `INCREMENTAL_LEVEL_STATEMENTS` and `INCREMENTAL_EDGE_STATEMENTS`
to the demand closure (section 4). The generated list is assembled in
`v6/prolog/emit_ts.pl` (the recon's smallest-change set, second bullet, names
this generator).

### 1.2 sql_rule_order strata (fixed rule ordering)

- `v6/prolog/strat.pl:79` header `sql_rule_order/2 : topological sub-order
  within each stratum group`; `strat.pl:81` `sql_rule_order(Rules, Ordered)`.
- `v6/prolog/lower.pl:21` notes lower.pl receives rules already in
  `strat.pl:sql_rule_order/2` order.

Meaning under demand: the stratum order is a static total order of level rules.
Demand is an orthogonal axis: the order does not change with queries (recon's
founding claim), and a demand-driven engine does not re-topo-sort per query. It
**leaves the ordering alone** for rules inside a cone. The only change is *which*
rules are present, not their relative order. Migration: none for ordering
itself; the pruned generator keeps `sql_rule_order` order within the cone.

### 1.3 Host-demand rows generated unconditionally

- `v6/tsv2/gen_emitted/native_ts_query_term.ts:322` the
  `__host_demand_tree_sitter` level emits identity/witness digests for
  every `file_digest x query_value` pair on every tick, with no dependency on
  `queryPlans`. The recon's verdict (recon `RECON-QUERY.md:146-149`) confirms the
  codebase vocabulary "demand" means *external-host input demand*, not query
  demand, and that these rows are generated unconditionally.

Meaning under demand: the host whose answers no query cone needs should never
probe; generating its demand rows is wasted work and (worse) a side-effecting
`sh` host would run queries nobody reads. A demand-driven engine must **change
its meaning**: host demand generators are included only for hosts inside a
demand cone. The host surface itself (`sh_decl`/`probe` are the built-in host
constructs, `v6/prolog/compile/registry.pl:189-190`) is unchanged.

Migration: key the host-demand statement emission (and the host executor wiring)
off the demand closure, exactly the recon's second bullet
(`RECON-QUERY.md:228-233`).

### 1.4 Boot statements

- `v6/prolog/lower.pl:3431` `boot_statements/5` builds a separate list of
  `bootstmt(Sql, Params)`.
- `v6/tsv2/serve/3_engine.ts:220-233` `bootServedProgram` runs
  `[...program.ddl, ...WitnessCache.ddl()]` then
  `BootRunner.run(seam, program.boot)` unconditionally at boot.

Meaning under demand: boot runs every declared boot statement regardless of any
query. A rel that no query cone needs still gets boot-seeded. Demand-driven
boolean: boot statements are **left alone for in-cone rels**; for out-of-cone
rels the questions is whether "eagerness = a standing query only"
(`2026-08-03-module-catalog-ruling.md:7,12`) implies boot seeds for unqueried
rels are dropped. That is a ruling fork, priced in section 2. Migration:
filter the boot list to the demand closure, or keep it whole (see 1.4 price).

### 1.5 Conformance fixture semantics (final/1 under demand)

The contract says "`final/1` asserts the union of ALL rels." Reality is more
precise, so the contract term is only partly supported:

- `v6/prolog/conformance/engine.pl:558-560` the oracle's final answer is the
  sorted union of all stores:
  `store_rows(Store, Rows), append(Rows, Level, FinalAll0), msort(...)`.
- `v6/prolog/conformance/engine.pl:578-579` the fixture expectation grammar is
  `final(Ref, SortedRows) | deltas(Ref, PerTick) | ticks(N) | throws(Term)`.
- `v6/prolog/conformance/engine.pl:602-604` `expectation_holds(final(Ref,...),
  FinalAll, _)` calls `rel_rows(Ref, FinalAll, ...)` which filters the union to
  one named rel.

So a fixture's `final/1` names a specific rel and selects that rel's rows out of
a union that is always computed in full. The parenthetical "union of ALL rels"
is UNKNOWN as a literal description of a single assertion; what is true is that
the oracle always *computes* every rel (eagerly) and each assertion then filters
to the rel it names.

Meaning under demand: a fixture that asserts a rel that no query line names
would have that rel never computed, so the assertion becomes vacuously empty or
stale. The fixture's *meaning* is unchanged if the rel happens to sit in a demand
cone; otherwise the fixture **breaks** unless a standing query is added. This is
precisely the migration-ladder point (section 6): every fixture whose asserted
rels are not covered by a query must add a standing query to preserve meaning.

### 1.6 The Prolog oracle's evaluate-everything loop

- `v6/prolog/conformance/engine.pl:503-531` `run_program/5` prepares, expands,
  and runs `level_closure` + `run_ticks` over every rule, seeding the store from
  initial rows (this receipt also covers 1.1/1.5's oracle half).
- The oracle parses/expects `final/1`, `deltas/2`, `ticks/1`,
  `throws/1` (`engine.pl:578`), keyed on whichever rels the fixture asserts,
  independent of queries (recon `RECON-QUERY.md:157-170`).

Meaning under demand: the oracle is the grading reference. If the emitted engine
becomes lazy, the oracle must either (a) gain the same demand closure so it
grades the pruned behavior, or (b) keep evaluating everything and treat the
pruned emitted program as a *different* program for grading purposes. This is a
**meaning change** to what the oracle is for (from "evaluate all, grade named"
to "evaluate the demand closure"). Migration: add a query/demand parameter to the
oracle's `run_program/5` that prunes `level_closure` and `run_ticks` to the cone,
keeping the no-query path byte-identical for the existing door.

### 1.7 TEXT_DOOR byte-identity goldens

- `v6/prolog/ARCH.pl:631` `TEXT_DOOR  compiled=102  byte_identical=102  failures=0`
  (a representative green row from the battery).
- `v6/prolog/compile/scripts/text_door_receipt.pl:1-12` the gate: every fixture
  that compiles through the term door must also compile through the text door
  (`.dl6` surface) and produce **byte-identical emitted TypeScript**, zero
  failures.
- `v6/prolog/compile/scripts/text_door_receipt.sh` runs the receipt under a
  time budget.

Meaning under demand: the byte-identity door compares emitted TypeScript between
two ways of feeding the *same* program. Both feeds produce the same demand
closure (the same queries), so if the demand closure is computed in the shared
compiler (`compile.pl:128-135`) before emission, both doors still emit identical
bytes. The door therefore **survives** as long as the pruning happens once, in
the shared path, and both doors feed the same queries. What a demand-driven
engine *does* change is the *content* of the emitted module versus today's full
program: the emitted module shrinks to the cone. Existing fixtures that rely on
the full-program emit (and on the oracle full evaluation) need a standing query
to keep their bytes (migration, section 6). UNKNOWN: whether the byte-identity
*counter* (e.g. `compiled=102`) is asserted against a pinned count elsewhere; the
receipt header explicitly rejects frozen counts
(`text_door_receipt.pl:9-12`), so the count is not a hardcoded assertion.

### 1.8 golden-flex's self-gating coverage

- `v6/prolog/compile/scripts/golden_coverage.pl:1-40` the gate reads the
  registry's construct inventory and golden-flex's own parsed term, and **fails,
  naming the construct**, when a live registry row is not exercised by the
  golden.
- `v6/dl/fixtures/golden-flex.dl6:1-25` the composition program header: "ONE
  program that exercises every live surface construct," graded at three
  cardinalities, oracle vs BOTH emitter modes, tick log AND final state, then
  end to end through the served HTTP engine.
- `v6/tsv2/tests/goldenFlexServed.test.ts` the served leg.

Meaning under demand: golden-flex grades *every* construct, so its whole cone
must be live. If the engine becomes lazy and only the demanded cone runs,
golden-flex is exactly the program that must keep a wide standing query set so
its every-construct grading stays meaningful. The self-gating property (a newly
live registry row fails the gate until the golden exercises it) is **left
alone**; it constrains the language surface, not the execution model. Migration:
ensure golden-flex carries standing queries covering all its asserted rels; the
gate's registry-vs-golden check needs no change.

### 1.9 The store's materialization

- `v6/tsv2/serve/3_engine.ts:220-233` materializes `program.ddl` plus
  `WitnessCache.ddl()` into the SQLite seam at boot.
- `v6/tsv2/gen_emitted/native_ts_query_term.ts:133-193` the `ddl` array `CREATE
  TABLE`s every rel plus every delta/frontier/support working table, including
  rels no query names (`__host_*`, `interval`, `query_source`, `query_value`).
- `v6/tsv2/gen_emitted/native_ts_query_term.ts:262` `finalSelect` holds one
  `SELECT` per rel, used only by the pull `rows(rel)` API
  (`3_engine.ts:127-133`), never by the tick (recon `RECON-QUERY.md:123-126`).

Meaning under demand: tables are created unconditionally now. A demand-driven
engine can drop DDL for rels outside every cone (saving the materialization)
or keep all tables but only populate in-cone ones. The price fork is in section
2 (edge-plane persistence). The pull `rows(rel)` API makes an out-of-cone rel
produce a valid-but-empty table: that is a **meaning change** (a caller asking
`rows(unqueriedRel)` currently gets live rows; under a pruned DDL it gets an
empty or unknown rel). Migration: either keep DDL whole (only prune statements
and host demand), or extend `rows(rel)` to lazily materialize on first ask and
declare unqueried rels as "not materialized" by contract.

### 1.10 serve's binds (interval/watch spin regardless of demand)

- `v6/tsv2/serve/2_binds.ts:4-5` live sources each submit on their own cadence:
  `interval(periodMs) -> map(toBucketRow) -> mergeMap(submit)` and
  `watchSource(root) -> bufferTime(...) -> map(diffAgainstLast) -> submit`; they
  spin whether or not any query needs them.
- `v6/tsv2/serve/3_engine.ts:102-113` `this.ticks$ = this.arrivals.pipe(concatMap(...))`
  turns only while subscribed; each batch runs the full tick.
- `v6/tsv2/serve/1_hosts.ts:690` and `v6/tsv2/serve/4_http.ts:322,426` host and
  `POST /arrivals` re-enter through `engine.submit`.

Meaning under demand: the serve layer's push model matches laziness *up to the
source*: `ticks$` only turns while subscribed, which is already lazy at the
rxjs level (`RECON-QUERY.md:128-130`). But the *sources themselves*
(`interval`/`watch` binds, hosts) are wired to submit unconditionally, not gated
on demand. Under a strict demand engine the binds for rels outside every cone
should not subscribe/poll at all (the contract's "we are literally subscribing
to everything" defect, `CONTRACT.md:26-27`). Meaning change: binds become demand-
scoped. Migration: build the bind executor set from the demand closure, same
principle as 1.3.

## 2. The edge-plane tension: `<+` rels under laziness

The `<+` arrow is the edge-rule operator, with its own semantics distinct from
the level arrow `<-`:

- `v6/prolog/0_coalesce_expand.pl:60` `:- op(1150, xfx, <+).`
- `v6/prolog/0_coalesce_expand.pl:104` `build_rule(edge, Head, Body, (Head <+ Body))`.
- `v6/prolog/0_program_check.pl:613` `rule_is_edge((_ <+ _)).`
- `v6/prolog/3_clock_check.pl:207-213` clock roles: `edge_trigger` reads the
  occurrence ring (`z`, `positive`, `source_delay`), `edge_sample` reads state,
  `edge_pre` reads previous. An edge body "reads OCCURRENCES" (the trigger
  itself), not steady state (`0_coalesce_expand.pl:107-110`,
  `present_goal(edge, Atom, latest(Atom))`).

The tension: `<+` rels fire on *occurrence* (a trigger arriving), so they are the
plane on which external events (the pre-commit, the interval tick) land. Under a
demand engine there is a window before the first demand for a given `<+` rel in
which its triggering events still arrive but nothing consumes them. Three
options, priced against conformance semantics and the pre-commit example:

### Option A: drop before first demand

Arrivals for an undemanded `<+` rel are discarded at ingress. Cheapest; zero
storage. Cost: any event that arrives before first demand is permanently lost.
For the pre-commit example this is fatal if the first `pre_commit` trigger is
what the composition keyed on and it arrived before the query subscribed.
Conformance: a fixture asserting a `<+` rel that is dropped pre-demand reports
an empty/partial result => **breaks** the fixture unless demand is established
before the relevant event (the `POST /arrivals` ordering test
`v6/tsv2/serve/4_http.ts:322,426` must be re-timed or the program boot-subbed).

### Option B: buffer at ingress

Hold undemanded `<+` arrivals in a bounded buffer keyed by rel; on first demand
replay the buffer, then stream live. Preserves all events up to the buffer
cap. Cost: memory; a cap policy is needed (unbounded pre-demand buffering is a
rogue one, contradicts laziness). Pre-commit: the first trigger is replayed on
demand, so the composition's first-then-merge still fires. Conformance: the
oracle must replay the same buffer, so the oracle's `run_program/5`
(`engine.pl:503-531`) needs a matching buffering model for undemanded `<+` rels
before their demand tick.

### Option C: persist via store materialization (module-catalog stance 1)

Rely on the store materialization (section 1.9): all `<+` rels keep their
tables, and undemanded rels are materialized-but-unread (or persisted on
arrival like any table write) so nothing is lost and the pull `rows(rel)`
(`3_engine.ts:127-133`) still returns true history. Cost: keeps the full DDL
(`native_ts_query_term.ts:133-193`), so the storage saving of laziness is
foregone; the saving moves to compute (no cascade, no host demand). Pre-commit:
safest, events never lost, but the "lazy" contract is weakened to
"lazy-compute, eager-store." This is the position `2026-08-03-module-catalog-
ruling.md:32-36` (catalog materialized into the store) is most consistent with.

### After first demand: share-no-reset

Per the user ruling (`CONTRACT.md:21-25`), after first demand an edge plane must
share with NO reset, i.e. never re-cold and never re-fire history it already
fired. The clock system must express and check this shape: a *new* pre-commit
after the switch must still fire even though the original trigger already fired.
This is the second-event-must-still-fire hazard (section 3.4). Under all three
options, share-no-reset governs the post-first-demand behavior and is orthogonal
to the pre-demand choice; the pre-demand choice only decides what happens to
arrivals before first demand.

Recommendation: Option C for correctness-preserving migration (no fixture
re-timing, no oracle buffer model, consistent with the module-catalog stance),
with Option B as the storage-conscious follow-up after the demand closure and
its tests land. Do not adopt Option A while any fixture or the pre-commit
composition depends on first-trigger semantics.

## 3. Generic event ingress as a language construct

Today's external sources are narrow built-ins: `interval` and `watch` are
defined as binds (`v6/prolog/compile/registry.pl:275-279`,
`bind_executor(interval, live_interval)`), and `sh` hosts are
`sh_decl`/`probe` (`registry.pl:189-190`). The generic transport today is
`POST /arrivals` (`4_http.ts:426,322`) and host/bind submit
(`1_hosts.ts:690`, `2_binds.ts:4-5`). The user ruling asks for a generic way to
receive external events, typed in the clock world, entering as a typed EDB row
(`CONTRACT.md:16-20`).

### 3.1 Decl surface

A typed external event source is declared like a bind but lowered as an EDB
arrival rel in the clock world. Design, consistent with registry.pl's construct
table (`registry.pl:189-190,275-279`) and with the two arrows (`<-` level,
`<+` edge):

```
event_source(pre_commit).   % courier fires on the git pre-commit hook
```

That is a new registry row family `event_source/1` (`world`, `no_refs`,
`decl(clock_receipt)`), mirroring how `bind_definition(interval, [...])`
carries columns (`registry.pl:262,275-276`). The git pre-commit enters as a
typed EDB row in clock world; a consumer reads it via the edge arrow because an
external event is an *occurrence*, matching `edge_trigger`'s
`z, positive, source_delay` clock role (`3_clock_check.pl:209`).

### 3.2 Pure-rxjs lowering

```
event_source(pre_commit) -> new Observable(emit on hook)  // one row per hook run
```

Compared to the binds it replaces, the convention is the same shape as
`2_binds.ts:4-5` (`source -> map(toRow) -> submit`) except the source is
user-declared rather than a built-in executor, and the row lands in the typed
EDB rel `pre_commit/0` in clock world instead of an untyped bind bucket.

### 3.3 The user's composition, in dl and rx

Given the ruling `CONTRACT.md:21-25` (first `pre_commit`, then `switchMap` to a
merge of `interval(1000)` and the `pre_commit` edge again, `share` no reset):

```
% dl
pre_commit_hook        <+ pre_commit.                       % edge on the trigger
begin_hook             <+ pre_commit_hook, first_time.      % fires once
after_hook(1)          <-  pre_commit_hook.
after_hook(Next)       <-  after_hook(At), interval(1000, At), Next := At + 1,
                          not(pre_commit_hook).              % re-fire guard
```

```
% rx (spanning the two arrows; edge = merge of occurrences, level = recompute)
const preCommit$ = eventSource("pre_commit");
const first$ = preCommit$.pipe(first());                       // first pre_commit
const roll$ = first$.pipe(
  switchMap(() => merge(interval(1000), preCommit$)),          // after the switch
  share()                                                       // NO reset
);
```

Second-event-hazard note: the merge after the switch re-subscribes to
`preCommit$`; a *second* pre_commit must produce a fresh occurrence. With plain
`share` (no `shareReplay`/`replay`), re-subscription gets a fresh stream, so a
post-switch pre_commit still fires; `shareReplay(1)` would NOT (it would replay
the first trigger and never see a genuinely new one). The clock checker must
therefore distinguish "share with no reset" from "share with replay."

### 3.4 What the clock checker must prove

The clock checker today computes dependency rings/signs/grades and throws on a
violation (`v6/prolog/3_clock_check.pl:559-563` `check_clock_program`,
`clock_violation/2`), over the expanded program only; it does not model a
subscribe/refCount plane. To check the first-then-merge shape it must add a
proof obligation:

- **Second-event-must-still-fire (S-rule).** After the switch, a *new*
  occurrence of the merged source must still be accepted as a fresh event. The
  checker proves this by verifying the post-switch subscription re-arms (a real
  observable, not a replay/`shareReplay`), so a second `pre_commit` is a new
  `edge_trigger` (`z, positive, source_delay`,
  `3_clock_check.pl:209`), not a suppressed duplicate. A program whose join
  keys off `pre(pre_commit)` (`edge_pre`, `3_clock_check.pl:212`) such that a
  repeated event is silently treated as the same must refuse
  (`clock_violation/2`), because that is exactly the "new pre_commit must still
  fire after the switch" case the user ruled out (`CONTRACT.md:24-25`).

This extends `3_clock_check.pl` from a static ring/sign/grade analyzer to one
that also checks the demand/subscribe topology; the natural home is the same
module (add a `clock_violation` clause) since it already reads
`edge_headed_refs/2` and the clock roles.

## 4. Demand closure mechanics

### 4.1 Queries as the only subscribe roots

- The single `?` surface: `v6/prolog/compile/parse_dl.pl:977-989`
  `query_stmt(query(Atom), ...)`, folded into `program(Decls, Rules, Queries)`
  at `parse_dl.pl:122,136` and `:348`.
- Registered as readonly decl lowering to `query_plan`:
  `v6/prolog/compile/registry.pl:192`
  `surface(query/1, read, no_refs, decl(query_plan), live)`.
- Today the query plan is inert: arguments dropped at emission
  (`emit_ts.pl:310-314` keeps only `Name/Arity`; `emit_ts.pl:419-422` emits
  `{ rel, arity, snapshot }`; shape at `emit_ts.pl:288`), and `queryPlans` is
  never read by the runtime (declared only at
  `v6/tsv2/runtime/types.ts:438`; test literal at
  `v6/tsv2/tests/6_host-extraction-batching.test.ts:119`; no other refs per the
  recon's grep at `RECON-QUERY.md:141-144`).

Under the ruling `2026-08-03-module-catalog-ruling.md:7,12` ("eagerness = a
standing query only"), a query line is both the projection root and the demand
root. First step must be to stop dropping the query arguments: keep
`columns(Args)` from `v6/prolog/1_host_expand.pl:404-410`
(`compile_query(query(Atom), query_plan(Name/Arity, columns(Args),
snapshot(current)))`) through `emit_ts.pl` (recon first bullet,
`RECON-QUERY.md:222-227`).

### 4.2 Imports ride the same demand plane

Per the module-catalog ruling: import = demand for a module instance, module args
= demand keys, eagerness = a standing query only (`2026-08-03-module-catalog-
ruling.md:9-14`). So an import is not a separate mechanism; it is a demand root
compiled to a standing query on the imported module's root rel. No new runtime
subscribe path is introduced; import lowers through the same `?` query surface.
Instance identity (module args as demand keys) is the same key the clock checker
keys its occurrence checks on (`3_clock_check.pl:209`).

### 4.3 Static pruning vs dynamic demand keys

- **Static (compile-time):** rels outside every query cone can be pruned from
  level/edge statements, host demand, and DDL. The boundary is a fixed-point
  reachability from the query roots over rule bodies. The rel-use walker
  already exists: `v6/prolog/analyze.pl:100-124` (`body_ref_uses/2`,
  `atom_ref_args/2`) enumerates every rel a body reaches, and
  `v6/prolog/compile.pl:124` (`program_plan/2`) already unions
  `program_refs/declared_refs/seeded_refs`; the cone is that walk seeded from
  query roots instead of "every ref."
- **Dynamic (runtime demand keys):** which *module instance* is demanded can
  depend on data (module args as demand keys, `ruling.md:126-131` M4 laziness:
  "unreferenced = never lowered"). Instance identity is a digest; the runtime
  decides at first instantiation (M4: two-phase checking,
  `ruling.md:126-128`). This must stay runtime; it cannot be a compile-time
  constant.

### 4.4 Where the cone computation lives

The cone is the body-rel reachability walk (`analyze.pl:100-124`) seeded from
query roots, with the full-ref basis already assembled by `program_plan/2`
(`compile.pl:124`). The natural host is `analyze.pl`: it already owns the
body-ref walker that defines reachability, and `program_plan/2` in `compile.pl`
provides the roots. Recommendation: put the static cone closure in `analyze.pl`
alongside the walker, and expose it to `lower.pl` (statement pruning,
`lower.pl:3431`) and `emit_ts.pl` (DDL and `INCREMENTAL_*` assembly) via
`program_plan/2`'s `Plan`, matching the recon's second bullet
(`RECON-QUERY.md:228-233`). Keep the dynamic demand-key (instance) decision in
the served runtime, keyed off `queryPlans` once it is actually read there
(`3_engine.ts`; recon third bullet, `RECON-QUERY.md:233-237`).

## 5. What does not change

Named invariants that survive demand-driven evaluation, each with a receipt:

- **The arrows' semantics inside a demanded cone.** A level rule still recomputes
  from state (`<-`, `level_read b positive`), an edge still fires on occurrence
  (`<+`, `edge_trigger z positive source_delay`):
  `v6/prolog/3_clock_check.pl:207-213`; the `<-`/`<+` operator definitions and
  coalesce expansion are unchanged: `v6/prolog/0_coalesce_expand.pl:60,104`.
  Demand only decides *which* rules exist in the run, not how an in-cone one
  behaves.
- **The fixed stratum order.** `sql_rule_order/2` topo-sorts within a group and
  does not depend on queries: `v6/prolog/strat.pl:79,81`. In-cone rules keep
  that order; pruning removes rules, never reorders them.
- **The store schema for materialized rels.** Every rel that is materialized
  keeps its table and its delta/frontier/support working tables:
  `v6/tsv2/gen_emitted/native_ts_query_term.ts:133-193`; the served
  materialization at boot is unchanged for in-cone rels:
  `v6/tsv2/serve/3_engine.ts:220-233`. UNKNOWN: whether a demand-pruned engine
  keeps the *entire* schema (Option C in section 2) or drops out-of-cone
  tables; that is a design fork, not an existing invariant.
- **Byte-identity of the non-query pipeline within a cone.** The
  `runIncrementalTick` sequencing (prepare/apply/edges/recompute/read/promote)
  at `v6/tsv2/gen_emitted/native_ts_query_term.ts:376-387` is unchanged for the
  statement set that survives pruning; only the *membership* of
  `INCREMENTAL_LEVEL_STATEMENTS`/`INCREMENTAL_EDGE_STATEMENTS` changes. The
  byte-identity door (`text_door_receipt.pl:1-12`) survives because both term
  and text door feed the same queries through the same compiler
  (`compile.pl:128-135`).
- **The self-gating golden surface check.** `golden_coverage.pl`'s
  registry-vs-golden fail mechanism is about the language surface, not
  execution: `v6/prolog/compile/scripts/golden_coverage.pl:1-9`. Demand does not
  touch it.
- **The push/serve seam for arrivals.** `POST /arrivals`
  (`4_http.ts:426,322`), host re-entry (`1_hosts.ts:690`), and the submit-based
  binds (`2_binds.ts:4-5`) all stay the transport; demand changes *which* binds
  subscribe, not how a submitted batch enters the engine (`3_engine.ts:102-113,
  191`).

Precision note: the contract asks for an explicit "what does not change" and the
recon claims byte-identity of the non-query pipeline is preserved. That holds
only for the statement subset that remains in a cone; the *emitted module as a
whole* necessarily differs when statements are pruned. The invariant is per-
statement, not per-module.

## 6. Migration ladder

Smallest landable steps, each with a gate, each leaving the battery green.
Battery references: conformance fixtures (`engine.pl` final/deltas), plunit,
TEXT_DOOR, sweep both modes, golden-flex, tsv2 tests.

1. **Keep query columns.** Stop dropping `columns(Args)` in
   `v6/prolog/1_host_expand.pl:404-410` and `v6/prolog/emit_ts.pl:310-314,419-422`;
   widen `IQueryPlanData` (`emit_ts.pl:288`) and the declared runtime type
   (`runtime/types.ts:438`). Gate: all existing fixtures compile and emit
   unchanged bytes (TEXT_DOOR `text_door_receipt.pl:1-12` still green), because
   this step adds metadata only and does not yet prune.
2. **Compute the static cone, but do not prune yet.** In
   `v6/prolog/analyze.pl` (near the walker at `100-124`), compute the query-root
   reachability and expose it through `program_plan/2` (`compile.pl:124`). Gate:
   a plunit on the new cone predicate; emitted bytes still unchanged.
3. **Read `queryPlans` in the served runtime; expose per-query standing
   streams** that re-emit the query rel's `finalSelect` rows on each tick's
   deltas (`3_engine.ts`; endpoint in `4_http.ts`; the recon's third bullet,
   `RECON-QUERY.md:233-237`). Gate: a new served test asserting a query rel
   re-emits on arrival; existing tsv2 tests untouched.
4. **Prune level/edge statements and host demand to the cone.** Key off
   `queryPlans` in the generator that assembles
   `INCREMENTAL_LEVEL_STATEMENTS`/`INCREMENTAL_EDGE_STATEMENTS` and the
   `__host_*` demand statements in `emit_ts.pl` (recon second bullet,
   `RECON-QUERY.md:228-233`). Gate: fixtures whose asserted rels sit in a cone
   stay byte-identical; fixtures whose asserted rels are out-of-cone break --
   this is the explicit fixture migration below.
5. **Prune DDL to the cone (Option C of section 2 first).** Keep tables for
   in-cone rels and the store materialization (`3_engine.ts:220-233`,
   `native_ts_query_term.ts:133-193`). Gate: served engine boots with the pruned
   DDL and the query streams (from 3) resolve.
6. **Make the oracle demand-aware.** Add a query/demand parameter to
   `engine.pl:503-531` `run_program/5` so grading matches the pruned program;
   keep the no-demand path byte-identical (`engine.pl:558-560`). Gate:
   golden-flex and conformance both grade the same behavior on both sides of
   the door diverged.

Which existing fixtures need a standing query added to keep their meaning
(section 1.5): any fixture whose `final(Ref,...)`/`deltas(Ref,...)` assertion
(`engine.pl:578,602-604`) names a rel that steps 4-5 would prune. Golden-flex
(`dl/fixtures/golden-flex.dl6:1-25`) grades every construct and therefore needs
standing queries covering all its asserted rels (or the pruned cone must be
treated as its cone). `native_ts_query_term`'s own `? captured(...)` query
(line 56 emit) already names one rel; its `__host_*`/`interval` rels are asserted
nowhere by its query, so under pruning they are the first candidates to need
either a standing query or an explicit out-of-cone contract. The byte-identity
and sweep fixtures that compile whole programs through both doors
(`text_door_receipt.pl:1-12`, `golden_oracle.pl`) must either add standing
queries or pin the no-demand full-program emit as a separate golden
(`ARCH.pl:631` TEXT_DOOR row).

## 7. UNKNOWNs

- The literal reading of "`final/1` asserts the union of ALL rels" (section 1.5):
  reality is per-rel assertion over an always-computed union. The contract's
  phrase is only partly supported; the always-score-everything half is
  grounded, the per-assertion scope is narrower.
- Whether a pruned engine keeps the full DDL (Option C, section 2) or drops
  out-of-cone tables; the existing code materializes everything
  (`native_ts_query_term.ts:133-193`), so the invariant is "all," and a change
  to that invariant is a ruling, not a receipt.
- A concrete `pre_commit` example already present in-tree: `labs/rel_as_stream`
  and the `ARCH.pl:57` switch_map scope note reference the shape, but no
  checked-in fixture instantiates the git pre-commit composition verbatim;
  future fixtures would be new content, not extracted from the current battery.
