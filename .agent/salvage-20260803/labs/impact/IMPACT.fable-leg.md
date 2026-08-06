# IMPACT: demand-driven laziness vs the engine as built

Analysis lane fable, branch lab/impact-lazy-fable at feb14d8d. Ground truth:
RECON-QUERY.md (the system is 100% eager; queries lower to dead metadata).
Every recon receipt this document builds on was re-read in this worktree and
line numbers re-verified. Paths are repo-relative.

Binding rulings (2026-08-03): the language must be lazy the way rxjs is lazy;
external events are typed clock-world EDB rows; a generic event ingress is
required, host is one transport among many; the canonical composition is
first-pre-commit then switchMap to merge(interval(1s), pre-commit-edge),
share with no reset; "we are literally subscribing to everything" is the
defect. Module-catalog stances consumed: import = demand, module args =
demand keys, eagerness = a standing query only, catalog materialized into the
store, static table set, keep-forever teardown with digest guards, no
mid-tick table creation (2026-08-03-module-catalog-ruling.md, decisions 1, 4,
M1, M4, F1).

---

## 1. The eager inventory

Ten sites where the engine assumes "evaluate everything, always". For each:
the mechanism, whether demand-driven evaluation BREAKS it / CHANGES its
meaning / LEAVES it alone, and the migration.

| # | site | verdict under demand | migration size |
|---|------|----------------------|----------------|
| 1 | tick cascade (every rule, every tick) | changes: statement list filtered to demanded cone | medium (plan-time prune) |
| 2 | sql_rule_order strata | leaves alone: same algorithm over the pruned rule set | small |
| 3 | host-demand rows generated unconditionally | changes: demand rule prunes with its consumer; the big win | none beyond #1 |
| 4 | boot statements | changes: level recompute restricted to cone; seeds stay | small |
| 5 | conformance fixture semantics (final/deltas) | meaning preserved only if expectations become demand roots | harness change, zero fixture edits |
| 6 | Prolog oracle evaluate-everything loop | breaks byte-identity unless the oracle prunes identically | medium (shared cone module) |
| 7 | TEXT_DOOR byte-identity goldens | leaves alone (both doors share the compiler) | golden regen only |
| 8 | golden-flex self-gating coverage | changes: graded rels need standing queries, or zero-query = eager default | fixture edit or default rule |
| 9 | store materialization / DDL | leaves alone: tables stay static and eager, rows become lazy | none |
| 10 | serve binds (interval/watch spinning) | changes: sources start on first demand, never stop (share no-reset) | medium (runner gating) |

### 1.1 The tick cascade

Every emitted level statement runs on every tick.
`runIncrementalTick` applies `INCREMENTAL_LEVEL_STATEMENTS` -- one entry per
level rule, the whole program -- unconditionally
(v6/tsv2/gen_emitted/native_ts_query_term.ts:375-387; the array at :311).
The served loop feeds every pushed batch into `program.tick`
(v6/tsv2/serve/3_engine.ts:188-208), and every ingress converges on that one
seam: HTTP (v6/tsv2/serve/4_http.ts:322), binds (v6/tsv2/serve/2_binds.ts:162,396),
hosts (v6/tsv2/serve/1_hosts.ts:690). `queryPlans` is read by nothing
(v6/tsv2/runtime/types.ts:438 is the only non-generated, non-test reference).

Under demand: the mechanism inside a demanded cone is untouched -- the change
is WHICH statements are in the array. The prune happens at plan time
(section 4), so `1_incremental.ts` itself does not change. Verdict: changes,
mechanically shallow.

### 1.2 sql_rule_order strata

`stratum_groups/2` and `sql_rule_order/2` order ALL level rules
(v6/prolog/strat.pl:18-41, 81-102); `program_plan/2` calls it over the full
rule list (v6/prolog/compile.pl:173) and `lower_program` consumes the order
list positionally (v6/prolog/lower.pl:3359-3398). Both operate on whatever
rule list they are handed. Restricting Rules to the cone before
`sql_rule_order` preserves relative order of surviving rules: stratum
relaxation and Kahn's order only consult edges among the rules present, and
removing a rule outside the cone removes no edge between two rules inside it
(a body ref of an in-cone rule is by definition in-cone). Verdict: leaves
alone; the call site moves after cone computation.

### 1.3 Host-demand rows generated unconditionally

A probe expands into a DEMAND LEVEL RULE plus a joined read
(v6/prolog/1_host_expand.pl:501-517: `DemandRule = (DemandAtom <- DemandBody)`).
The emitted program then derives demand rows on every tick from whatever the
body joins -- e.g. `INSERT OR IGNORE INTO "__host_demand_tree_sitter" ... FROM
"__frontier_file_digest" d0, "query_value" b0`
(v6/tsv2/gen_emitted/native_ts_query_term.ts:322) -- and the served host
runner executes a subprocess per unanswered witness, boot replay plus live
+deltas (v6/tsv2/serve/1_hosts.ts:534-571). Nothing consults queries; the
word "demand" here means world-facing demand only (RECON-QUERY.md runtime
section, re-verified).

Under query demand: the demand rule is an ordinary level rule, so it prunes
with the rest of the cone -- `__host_demand_N` is in the cone iff
`__host_response_N` is read by an in-cone rule (the compiler must add that
response→demand pairing edge to reachability; the response rel is EDB, so
plain body-ref reachability would stop there). This is where laziness stops
real work: subprocesses. Verdict: changes; one extra edge kind in the cone
computation, no host-runner change.

### 1.4 Boot statements

`boot_statements/5` = seed statements for Initial rows plus a full t=0 level
recompute over EVERY level statement (v6/prolog/lower.pl:3431-3457);
`bootServedProgram` runs all DDL then all boot statements
(v6/tsv2/serve/3_engine.ts:220-233). Under demand: the level-recompute half
is the same list as #1 and prunes with it. The seed half stays whole -- seeds
are EDB writes, and the edge-plane resolution (section 2) keeps EDB ingress
eager. Verdict: changes, by inheritance from #1 only.

### 1.5 Conformance fixture semantics

The oracle's `run_program/5` returns `FinalAll` = the sorted union of every
store and every level rel (v6/prolog/conformance/engine.pl:558-560);
`final(Ref, Rows)` filters that union per rel and `deltas(Ref, PerTick)`
filters the tick deltas (engine.pl:576-617). A fixture never names a demand
root; `query(` appears in exactly 2 of the fixture files
(4_struct_values.pl, 2_hosts_wiring.pl) and is discarded at engine.pl:504
(`prepare_program(SugaredProg, HostProg, _, _, _)`).

What a fixture MEANS under demand: `final(some_rel, Rows)` currently means
"after evaluating everything, some_rel holds Rows". Under a demand-driven
engine, a rel outside every query cone holds nothing, so most of the 281
fixtures would fail as written. Fork:

- **F-fix-A (recommended): expectations are implicit demand roots.** The
  harness treats every `final/2` and `deltas/2` ref as a standing query from
  t=0. Zero fixture edits; a fixture keeps meaning exactly what it means; the
  demand plane is exercised by NEW fixtures that demand less than they
  compute. Price: the harness gains a roots parameter; "a fixture asserts
  what it demands" becomes a stated law.
- **F-fix-B: zero queries = demand everything** (program-level default, see
  section 4). Also preserves all 281, and needs no harness change at all --
  but then no existing fixture ever exercises pruning, and a fixture WITH a
  query line would suddenly prune, silently changing the 2 fixtures that
  have one. Price: the 2 query-bearing fixtures need audit; the default is a
  program property rather than a harness property, so it also governs
  production.

These compose: B as the program default, A as the harness refinement when
the default flips. Verdict: meaning preserved under either; decide before
step 5 of the ladder.

### 1.6 The Prolog oracle

`run_program/5` seeds, closes over all plain level rules
(`level_closure(PlainLevel, AggRules, BaseRows, 0, Level0)`, engine.pl:529),
then `run_ticks` recomputes every tick (engine.pl:558-570). Queries never
consulted (engine.pl:504). golden-flex byte-diffs the oracle's tick log and
final state against both emitter modes
(v6/tsv2/scripts/golden-flex.sh:97-110), and the sweep does the same across
the corpus. A pruned runtime emits NO deltas for undemanded rels; an eager
oracle emits them; byte identity dies on the first pruned program.

Verdict: breaks unless the oracle prunes identically. Migration: the cone
computation must live in a module both doors and the oracle share (the
precedent is exact: 1_host_expand.pl is already shared between compiler and
oracle, compile.pl:88-98 comment). The oracle filters Rules by the cone
before `split_rules`; everything downstream is untouched.

### 1.7 TEXT_DOOR byte-identity

The gate compares term-door output vs text-door output of the SAME compiler
build, dynamically over every fixture that compiles
(v6/prolog/compile/scripts/text_door_receipt.pl:1-12; 196/196/0). Both doors
run `program_plan/2` → `lower_program` → `emit_ts`, so a cone prune added
inside `program_plan` changes both doors identically. Verdict: leaves alone.
The separate cost is golden churn: `gen_emitted/*.ts` are byte-pinned, and
step 1 of the ladder (query columns in `IQueryPlanData`) changes one line of
every query-bearing golden -- a regen commit, gated by sweep.

### 1.8 golden-flex self-gating coverage

`golden_coverage.pl` fails BY NAME when a live registry row is not exercised
by golden-flex.dl6 (v6/prolog/compile/scripts/golden_coverage.pl:1-31). Two
consequences:

- Any new ingress construct (section 3) must get a registry row AND a
  golden-flex usage, or the gate is red. The gate is a forcing function in
  the migration's favor.
- golden-flex.dl6 carries exactly two query lines, `? display(...)` and
  `? pick_stats(...)` (v6/dl/fixtures/golden-flex.dl6:467-468), while its
  grading is whole-program byte-diff (golden-flex.sh:97-110). Under
  pruning-by-default the golden's cone would exclude most of its own graded
  rels. Migration: add `?` lines for every graded rel (self-documenting: the
  golden then declares its own observation surface), or ride the zero-query
  default -- but golden-flex is NOT zero-query, so it is the first named
  program that needs standing queries added. Verdict: changes; one fixture
  edit, and the coverage gate itself is an ally.

### 1.9 Store materialization and DDL

Every rel -- queried or not -- gets CREATE TABLE plus delta/frontier/refCount
working tables at boot (v6/prolog/lower.pl:3416 assembles the DDL;
v6/tsv2/gen_emitted/native_ts_query_term.ts:133-193;
v6/tsv2/serve/3_engine.ts:224 runs it plus `WitnessCache.ddl()`). The
module-catalog ruling already fixes this plane: static table set (F1),
catalog materialized (decision 1), no mid-tick table creation (M4).

Verdict: leaves alone, deliberately. Tables stay eager and static; ROWS
become lazy. An undemanded rel is an empty table, which is the language's
existing reading of an unused declaration ("an undemanded declaration is an
EMPTY relation, not an absent one", v6/prolog/1_host_expand.pl:106-111).
Price: DDL for rels that may never fill -- a table-count cost, zero compute;
the store's own storage-diet items cover it independently.

### 1.10 Serve binds

`IntervalBindRunner` subscribes an rxjs `interval` per literal at program
load (v6/tsv2/serve/2_binds.ts:139-176); `WatchBindRunner` boots a watcher,
runs `git ls-files`, hashes files, and reconciles per glob at load
(2_binds.ts:373-448); each firing submits a batch that runs a full tick.
This spinning happens whether or not anything reads the results -- the exact
"we are literally subscribing to everything" defect.

Under demand: a bind's rel is in some query cone or not. Statically out:
don't start the source at all (the emitted plan's `literals` list is already
per-program, emit_ts.pl:392-402; the prune shrinks it). Statically in: start
on FIRST demand, and per the share-no-reset ruling never stop --
`share({ resetOnRefCountZero: false })` is the lowering; teardown stays
keep-forever (catalog M4). With queries as boot-standing subscriptions
(section 4), "first demand" for today's programs is boot, so behavior is
unchanged for every existing golden. Verdict: changes; runner gating plus
plan filtering.

---

## 2. The edge-plane tension: `<+` under laziness

Edge rules fire on body-atom ARRIVALS this tick and append; consequences
never retract (v6/prolog/LANG.md:31-36). The log plane stores stamped
occurrences (`lrow(st(0, Position), Row)`, engine.pl:549-556). The tension:
laziness means nothing runs until asked, but an occurrence that happens
before anything asks either vanishes, waits, or lands in storage.

The split that dissolves most of it: INGRESS IS NOT EVALUATION. Applying a
signed EDB arrival to its table is a plain write; derivation starts where
rules read it. The three
options differ in what happens to that write before first demand.

### Option E1 -- dropped (pure rx cold-observable semantics)

Arrivals to rels in no demanded cone are discarded at the seam.

- rx reading: exact. A cold observable's history does not exist; subscribe
  late, see nothing.
- Conformance: any fixture whose expectations become demand roots at t=0
  (F-fix-A) never observes a drop, so the battery survives; but a NEW
  late-demand fixture asserting pre-demand occurrences is inexpressible.
- Pre-commit example: safe ONLY because the composed global is demanded from
  boot (the query is standing). An undeployed query = silently lost commits.
- Price: cheapest to build; a durable engine that forgets world events it
  was told about; contradicts stance 1 (the store materializes) and the
  self-diagnosis law (the trail has holes).

### Option E2 -- buffered at ingress

Arrivals for undemanded cones queue in memory; first demand replays them.

- rx reading: `ReplaySubject` per rel. But the replay re-enters the tick
  loop LATER than the occurrence, so occurrence tick stamps move -- the tick
  log stops being the record of when things happened, which the Z-ring
  derivative semantics (compile/TICK-MODEL.md section 1-2) is built on.
- Unbounded memory on exactly the programs that ignore the most input;
  collides with "nothing seizes the machine".
- Price: worst of both; listed for completeness only.

### Option E3 -- persisted via store materialization (recommended)

Every EDB arrival is applied to its table always -- the ARRIVAL_STATEMENTS
path stays eager for all arrival targets -- and only DERIVED evaluation is
demand-gated. This is stance 1 extended to the whole EDB plane: the store
materializes; ingress is a write to the base layer.

- Level reads over a log rel work retroactively: first demand runs the cone's
  recompute over the full stored history -- the B-plane answer is complete.
- Edge ARMS fire on deltas, and there are no deltas in the past: occurrences
  that landed before first demand do not retro-fire edge arms. The language
  already names this exact behavior "late-subscriber backlog replay"
  (LANG.md:35-36) as the known consequence of the edge join shape; E3 makes
  the backlog READABLE (it is in the table) without re-firing arms over it.
  A program that wants catch-up spells a level rule over the log; a program
  that wants edge-only sees events from demand onward. Both are honest.
- Conformance: with F-fix-A, no existing fixture ever observes the
  distinction. New fixtures can pin both behaviors (level-over-log sees
  history; edge arm does not).
- Pre-commit example: a pre_commit row that arrives before the query stands
  is durably in the table; the level half of the composition (`armed`, below)
  derives true at first demand, so the interval starts; the edge half fires
  from demand onward. No lost commits, no replayed ticks.
- Price: undemanded EDB tables grow (rows, disk -- bounded by `rel(N)` /
  `keep` retention where declared); arrival statements stay in the emitted
  program for every EDB rel, so ingress cost is unchanged from today (it is
  today's behavior, kept).

### After first demand: share with no reset

Ruled: once a cone is demanded it never goes cold. Lowering:
`share({ resetOnRefCountZero: false })` at every derived rel the cone
materializes; in engine terms, demanded-cone membership is monotone per
program load (a set that only grows), so the statement list per tick only
grows, and the COUNT-test discipline (statements/tick) still has a stable
ceiling to pin. This matches catalog M4 (keep-forever, digest guards) and
avoids the recompute-guard hazard: no cold restarts means no from-scratch
re-derives to guard.

---

## 3. Generic event ingress as a language construct

### 3.1 What exists

- `bind` is the world-push construct; its name set is CLOSED: `interval` and
  `watch` only (registry.pl:275-279 `bind_definition/2`; anything else
  throws `bind_mismatch`, v6/prolog/1_host_expand.pl:388-390).
- `sh` hosts are pull (demand/response, witness-cached) -- a transport for
  answers; unsolicited events have no place in that contract.
- POST /arrivals is the generic transport, and it already validates every
  field of every row against the program's own declarations
  (v6/tsv2/serve/4_http.ts:300-330, `handleArrivals$` → `engine.submit` at
  :322). What is missing is not a wire, it is a DECLARATION: a program
  cannot say "this rel is fed by an outside event source of this shape" and
  have the clock checker treat it as such.

### 3.2 The decl surface (fork, with recommendation)

- **Option I1 (recommended): open the bind table with a third executor.**
  A bind whose name is not a built-in is an external event source; its
  columns are its typed row shape; its executor is `live_event`, which
  starts NO process -- ingress is POST /arrivals (or a per-source route
  `POST /events/:bind`) hitting the existing validation. Surface:

  ```
  bind pre_commit(repo: text, head_digest: text).
  ```

  Registry: one new `bind_executor(<open>, live_event)` defaulting rule
  beside the existing two (registry.pl:278-279), and the `bind_definition/2`
  closed-set check relaxes to built-ins-only. `bindPlansFor`'s known-set
  gains `live_event` (2_binds.ts:452-463) with a runner that only registers
  the rel name for ingress. Price: smallest possible; no new keyword
  (LANG.md:15-16 records that `external`/`register` already died -- bind IS
  the unbundled survivor); the golden-flex coverage gate forces one usage.
  The configuration-column convention (column 1 = config literal,
  registry.pl:260-266) does not apply to live_event -- its rows are wholly
  world-authored -- which must be stated in the registry row to keep
  `bind_read_literals` honest (an event bind contributes no literals and
  starts no source; it authorizes ingress).

- **Option I2: a new decl functor (`event pre_commit(...)`).** A registry
  row `event_decl/2, world, decl(event_plan)`. Price: new keyword against
  the four-keyword law; parser + printer + roundtrip + tmLanguage churn;
  buys only a different word for what I1 says with `bind`.

- **Option I3: hosts with streaming responses.** Rejected on shape: a host
  is demand-keyed and witness-cached; events have no witness and no demand
  key. Restating LANG.md:26 ("an effect is a lazy rel whose oracle is the
  world") does not make an unsolicited event an answer.

Typed clock-world entry: an I1 event bind rel is EDB; the clock checker
already classifies outside arrivals as edge triggers with `source_delay`
resolved from the graph (registry.pl:207-213 `clock_role/4`;
v6/prolog/3_clock_check.pl). No second vocabulary is added -- exactly the
ruling: "typed in clock world even tho its host event or event matcher from
edb".

### 3.3 The worked composition (pre-commit, then every second)

dl (descriptive names; `armed` is the "has the first pre-commit happened"
level; the edge plane fires the pulses):

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

The second `gate_fire` arm is the "pre-commit edge again" leg of the merge:
a LATER pre_commit fires it (pre_commit is the arriving atom, `armed` and
`interval` are sampled current sets). The first arm is the interval leg.
The `?` line is the demand root; under section 4 it is the one subscribe.

rx lowering (the ruling's shape, verbatim intent):

```ts
const preCommit$ = ingress("pre_commit");            // POST /events/pre_commit
const armed$ = preCommit$.pipe(take(1));
const gateFire$ = armed$.pipe(
  switchMap(() => merge(interval(1000), preCommit$)),
  share({ resetOnRefCountZero: false }),             // lazy init, one global, never re-cold
);
```

### 3.4 What the clock checker must prove (the second-event hazard)

The hazard in rx: after `take(1)` the first-event stream completes; the
merged `preCommit$` inside the switch must be the SAME live source, or the
second pre-commit is silently dropped. In dl the hazard cannot occur by
construction -- a rel is one shared arrival stream and an edge arm fires on
every arrival forever -- but the checker must still PROVE the composed shape,
because the same rel (`gate_fire`) is written from one origin
(`pre_commit`) along two paths with different offsets:

- direct: `pre_commit` → arm 2, offset 0 (outside arrival fires the arm
  this tick);
- via `armed`: `pre_commit` → `armed` (edge write) → arm 2's sample, offset
  +1 (`edge_departure`/edge-written occurrences carry to the next tick;
  `clock_role/4`, registry.pl:207-213 and the source_delay note).

Unequal offsets into one relation from one origin is EXACTLY the existing
`clock_path_conflict` refusal (v6/prolog/3_clock_check.pl:325-341,
`clock_refusal_reason` at :395). So the canonical ruled composition, written
naively, is REFUSED by the checker as built. This is the concrete checker
work item, as a fork:

- **C1 (recommended): a named label instead of a refusal for the
  arm-with-sample shape.** When the conflicting shorter path's arm ALSO samples the rel the
  longer path writes (`armed` in arm 2), the first-occurrence race is
  confined to the arming tick and the steady state is well-defined; the
  checker emits a `clock_boundary/2` label (the precedent is exactly
  `multi_trigger_batch_invariance` and `arm_absence_batch_invariance`,
  TICK-MODEL.md checker-status section -- both are labels because both shapes
  appear in ruled programs). The label states the one observable choice: does
  the ARMING pre_commit itself produce a gate_fire that tick (offset-0 path)
  or not (offset-1 sample sees `armed` empty). Fixture pins whichever
  reading the user rules; `compile/test/3_clock_history.pl` gets the pair.
- **C2: keep the refusal, require the program to split rels** (a
  `gate_fire_on_commit` and `gate_fire_on_pulse`, unioned by a level rel).
  Always expressible, never ambiguous; price: the canonical composition
  cannot be written in its natural shape, and the ruling's "the clock system
  must be able to express and check this shape" reads as unmet.

Second half of the proof obligation, both options: `armed` is edge-headed
and nothing finalizes it, so it is monotone -- once armed, never disarmed --
which is `share` with no reset stated as a clock fact. The checker can
discharge it syntactically (no `finalize(armed)`, no level head on `armed`,
kind not keyed), and SHOULD emit it as a queryable `clock_fact/5` so the
first-then-merge shape is certified rather than assumed.

---

## 4. Demand closure mechanics

### 4.1 Queries as the only subscribe roots

The single `?` surface parses to `query(Atom)`
(v6/prolog/compile/parse_dl.pl:977-989, many per file via :544), is carried
as `query_plan(Name/Arity, columns(Args), snapshot(current))`
(v6/prolog/1_host_expand.pl:404-410), and then the columns are dropped and
the plan goes inert (v6/prolog/emit_ts.pl:310-314, 419-422; discarded by the
compiler at compile.pl:106; never read at runtime). The recon's smallest
change set stands, spot-verified: carry columns through emission; compute the
cone; consume `queryPlans` in serve as standing per-query streams over
`finalSelect` deltas.

Under the module-catalog ruling the same plane serves imports: referencing a
module member IS demand (decision 4), module args are demand keys (M1), and
an unreferenced member is never lowered (M4). One demand plane, two
customers -- `?` lines and cross-module references -- which is why the cone
belongs in the compiler's shared analysis layer rather than in serve.

### 4.2 Static vs dynamic

**Static (compile-time prune).** Rel-level reachability from the query rels:

- through level and edge rule bodies (`body_ref_uses/2`,
  v6/prolog/analyze.pl:104-131 -- positive, negated, sampled, and `pre` reads
  all count: a negation or a `pre` read is still a read);
- through the host pairing `__host_response_N` ⇒ `__host_demand_N`'s rule
  (section 1.3 -- the one edge plain body-refs cannot see);
- through bind rels to bind literals (a cone containing `interval/2` keeps
  the `interval(1, _)` literal alive; one outside it does not).

Everything outside the closure: rules not lowered, level statements not
emitted, host plans and bind literals dropped from the world plan. Tables
still created (section 1.9, F1).

**Dynamic (runtime demand keys).** Which KEYS of an in-cone rel are needed --
`? dep_pin("cli", module, version)` versus all repos -- is data. The catalog
ruling already names the mechanism: args become leading columns, magic-set
shape (M1 "scalar data-driven -> args become leading columns (magic set)").
That is a second arc: it changes lowering (per-rule filters seeded from a
demand-keys table) and leaves the tick loop alone. The honest v1 boundary:
static cone prune ships first; a query's argument constants are metadata the
runtime may use to filter its OUTPUT stream, while the derivation itself
stays whole-key until the magic-set arc.

### 4.3 Where the cone lives

`analyze.pl` is the natural host, and slightly better: a new shared module
beside it. Receipts for the placement: analyze.pl already exports the
reachability primitives (`body_ref_uses/2`, `program_refs/2`,
`declared_refs/2`) and is imported by strat, lower, and compile; but the
ORACLE must run the identical cone (section 1.6), and the oracle imports
analyze only indirectly. The pattern to copy is 1_host_expand.pl -- one file,
both doors and the oracle consume it (compile.pl:88-98). So:
`v6/prolog/2_demand_cone.pl` (name per the phase-numbered convention),
exporting `demand_cone(Decls, Rules, QueryPlans, ConeRefs)` and
`prune_to_cone(Rules, ConeRefs, ConeRules)`; called from
`compile.pl:program_plan/2` between `prepare_program` and `sql_rule_order`
(compile.pl:124-175), and from `engine.pl:run_program/5` before
`split_rules` (engine.pl:526-527). QueryPlans stop being discarded at both
sites (compile.pl:106, engine.pl:504).

### 4.4 The zero-query default (fork)

- **D1 (recommended for migration): zero queries = everything demanded.**
  The module-catalog ruling's own spelling -- "eagerness = a standing query
  only" -- read as: a program with no `?` line has an implicit standing query
  over every declared rel. Every existing program, golden, and fixture is
  zero-query or nearly so, and compiles to byte-identical output. Price: the
  lazy behavior is opt-in by writing a query, which inverts "lazy by
  default" until the default flips (ladder step 7).
- **D2: zero queries = nothing demanded.** Honest laziness, and every
  zero-query program becomes a no-op. Price: the entire battery breaks in
  one step; only viable behind D1-first migration.

---

## 5. What does not change

Precise invariants, each with its reason:

1. **The tick transaction model and drain rule.** One batch = one tick +
   bounded drains; the fold and cap (v6/tsv2/serve/3_engine.ts:136-186,
   DRAIN_CAP at :67; engine.pl:558-570, drain_cap). Demand changes which
   statements run inside a tick, never the tick protocol.
2. **The arrows' semantics inside a demanded cone.** Level IVM
   (frontier/refCount statements), edge arms, aggregate strata gaps
   (strat.pl:43-54), retention, keys: all statement GENERATION is untouched;
   the cone only selects which rules reach the generator
   (lower.pl:3359-3418 consumes the plan it is handed).
3. **Byte identity of the non-query pipeline under D1.** A full cone yields
   today's exact statement list; the only emitted-byte change is the
   `queryPlans` const gaining columns (emit_ts.pl:327-331 renders whatever
   the shape holds). The recon's claim ("that set changes only the query
   path") holds with that one stated exception, gated by sweep + TEXT_DOOR.
4. **The store schema and DDL posture.** Static table set, dictionaries
   first, witness cache, WITHOUT ROWID shapes (lower.pl:3413-3417;
   3_engine.ts:224). Rows lazy, tables eager (F1, section 1.9).
5. **The ingress seam and arrival validation.** One submit seam, typed
   row checking at the boundary (4_http.ts:300-330; 2_binds.ts; 1_hosts.ts:690)
   -- under E3 it stays eager for every EDB rel, event binds included.
6. **The tick log line format and Z-ring reading.** `TickLogEmitter.line`
   with column types (3_engine.ts:163); the log remains the signed multiset
   record for whatever ran. Under a pruned cone the log is the derivative of
   the DEMANDED state, which is the honest statement of what evaluated.
7. **The oracle/runtime agreement CONTRACT.** Byte-diff stays the gate; both
   sides prune from one shared module (1.6), so the contract survives the
   semantics change rather than being weakened to subset comparison.

---

## 6. Migration ladder

Each step lands alone, battery green after each.

1. **Carry query columns.** `compile_query` already builds `columns(Args)`
   (1_host_expand.pl:404-410); stop dropping them at emit_ts.pl:310-314 and
   :419-422; widen `IQueryPlanData` (emit_ts.pl:288). Regen gen_emitted.
   Gate: sweep, TEXT_DOOR, tsv2-test, golden-flex -- all unchanged verdicts,
   goldens re-pinned in the same commit.
2. **Serve consumes queryPlans.** Standing per-query streams: on each tick
   whose deltas intersect the query rel, re-emit `finalSelect` rows
   (3_engine.ts owns the stream; 4_http.ts grows GET /query/:rel SSE beside
   GET /ticks; registry.pl `http_route/3` row added). No engine change.
   Gate: new tsv2 tests; one-subscribe ratchet untouched (the stream rides
   `ticks$`, no new `.subscribe()`).
3. **Cone as analysis only.** New shared `2_demand_cone.pl`; a diag receipt
   prints per-program cone size vs total (rels pruned, statements pruned).
   No behavior change anywhere. Gate: new plunit tests; conformance 281
   untouched.
4. **Prune behind a flag, D1 default.** `program_plan/2` and the emitter
   prune when the program has queries AND the flag is on; zero-query = full
   cone always. New fixtures: a query-bearing program with COUNT tests
   pinning statements/tick and EXPLAIN SEARCH-not-SCAN on the cone path,
   plus a pruned-host fixture proving no subprocess spawns for an undemanded
   probe. Gate: sweep with flag off is byte-identical; flagged fixtures
   green.
5. **Oracle prunes; expectations become roots.** engine.pl takes the cone;
   conformance harness passes every expectation ref as a demand root
   (F-fix-A) so all 281 fixtures keep their meaning verbatim. Gate:
   conformance 281/0 with the flag ON for the oracle.
6. **World plane gated.** Bind literals and host plans filtered to the cone
   at emit; `live_event` ingress (I1) lands here with its registry row and
   golden-flex usage; bind start moves to first-demand with share-no-reset.
   Gate: golden-flex (edit per 1.8: add `?` lines for graded rels),
   extraction-live, served legs, watch-scale.
7. **Flip the default.** Pruning on for query-bearing programs everywhere;
   zero-query stays eager (D1). Fixtures needing standing queries added, the
   complete list as of this audit: `v6/dl/fixtures/golden-flex.dl6` (two `?`
   lines today, graded rels exceed them, section 1.8);
   `v6/tsv2/goldens/multirepo_crawl/0_multirepo_crawl.dl6` already queries
   its four graded rels (:113-116) -- audit confirms cone covers grading;
   the 2 term fixtures with `query(` (4_struct_values.pl, 2_hosts_wiring.pl)
   ride the harness roots from step 5; every other .dl6 golden is zero-query
   and rides D1. Gate: green-all, 31 legs.
8. **Clock-checker first-then-merge facts (C1 or C2 per ruling) + the
   pre-commit worked example as a served golden.** Separate arc; depends on
   6. Gate: conformance + clock-history replay pair + a new served golden.

Steps 1-3 are individually trivial to revert; 4-5 are the semantic commit;
6-8 are where the machine stops spinning.

---

## 7. Open forks requiring a ruling

| fork | options | recommendation | priced in |
|------|---------|----------------|-----------|
| pre-demand edge arrivals | E1 drop / E2 buffer / E3 persist | E3 | section 2 |
| fixture meaning | F-fix-A roots / F-fix-B default | A (with B during migration) | 1.5 |
| ingress surface | I1 open bind / I2 new keyword / I3 host stream | I1 | 3.2 |
| checker on first-then-merge | C1 label / C2 refusal+split | C1 | 3.4 |
| zero-query default | D1 eager / D2 empty | D1 | 4.4 |
| arming-tick fire | offset-0 fires / offset-1 waits | none -- needs the ruling C1 exists to record | 3.4 |

Nothing above is decided here; E3/F-A/I1/C1/D1 are recommendations with
their prices stated, awaiting the user's word.
