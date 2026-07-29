# TICK MODEL: the semiring/grading semantics behind the clock checker

Status: formal record (2026-07-29, user-directed "write this math proof
down near the clock checker code"). The clock/cardinality checker
(golden plan phase 5) implements THIS document; until it exists, the
load-time refusals listed in "Theorems" are its hand-proven instances.
The checker's home is analyze.pl's supported-subset gate (see the
pointer comment there) plus engine.pl check_program/1.

## 1. Objects: three semirings

Every rel denotes a function from tick to an annotated relation. The
annotation ring is what the two rule arrows select:

| ring | plane | rel type | consequences (fixture witnesses) |
|---|---|---|---|
| B (boolean, idempotent) | level / set | `S : Tick -> B-Rel` | identical write = zero delta (key_identical_write_is_silent); count never 0 on empty group (level_eval agg_rule_rows Bag \== []); keyed = at most 1 per key at tick boundaries (key_last_write_wins) |
| N (counting) | occurrences / log / refCount | `O : Tick -> N-Rel` | log duplicates fire again (identical_increments_stack_as_log_deltas); refCount retraction = subtraction in N (P3 emitter); cycles never cancel in N (retraction lab: counts never reach zero on cycles, reseed required) |
| Z (signed) | the delta stream / tick log | `dS : Tick -> Z-Rel` | boundary diff = discrete derivative of the B plane (engine.pl boundary_deltas); the tick log is the signed multiset record (ticklog.ts) |

The tick loop is one Kleisli step iterated: sqlite state in a State
monad, tick log in a Writer monad over Z-annotations. The within-tick
level closure is a least fixpoint taken in B, which is why it
terminates and why level rules are timeless.

## 2. The derivative reading (lifecycle = sign decomposition)

The edge plane is the derivative of the level plane:

```
S  : Tick -> B-Rel          level state
dS : Tick -> Z-Rel          its per-tick derivative (the tick log)

bare trigger atom  == subscribe (dS)+     the positive part
finalize(atom)     == subscribe (dS)-     the negative part
update arm         == (dS)- at t  JOIN  S at t     (update-arm lab U1: pairwise)
complete           == the scope rel's own (dS)-
```

The four lifecycle arm kinds are the sign decomposition of one
derivative; that is why the update arm needed zero constructs
(plans/2026-07-29-update-arm-verdict.md) and why finalize in a level
rule is necessarily a refusal: a level body reads S, and S has no
sign to bind.

Log rels are the special case where state is the integral of the
delta stream (no retraction => monotone); set rels are the general
case where the delta stream is the derivative of state.

## 3. Grading: what tick a thing is on

Rule-graph edges carry a tick delay in the monoid (N, +); a path's
offset is the sum of its edge grades (a graded monad):

| rule-graph edge | grade | witness |
|---|---:|---|
| level rule reading anything | +0 | inside the fixpoint |
| edge rule triggered by an arrival-side delta | +0 | edge-carry seam receipt: arrival-side derived rows fire same tick |
| edge rule triggered by an edge-WRITTEN rel | +1 | seam receipt: stage_one writes t1, stage_two fires t2 (edge_chain_hops_tick_per_stage) |
| finalize | +1 | departure is a next-tick occurrence (update-arm U1) |
| pre | -1 | reads the previous boundary |
| @next carry | +1 | by construction |
| clock/world bind | injects at its world tick | 1_binds.ts |

pipe_stage_costs_one_tick and edge_chain_hops_tick_per_stage are
fixtures OF this table.

## 4. Coercions between rings (the A2 analysis)

Inside one edge body, the trigger atom is read in N (bag, one firing
per occurrence) while store-side atoms are re-solved in B (Visible is
sort/2-deduplicated). N JOIN B with no stated coercion is why join
cardinality is a function of arrival batching (design review A2:
three batchings of the same rows give 3, 1, 2 result rows). The
coercion operators are:

| operator | coercion | status |
|---|---|---|
| latest(Atom) | N -> (0 or 1) per tick (sample) | edge-body lowering owed (review B1); in level rules it is the identity on B and therefore REFUSED (latest_in_level_rule) |
| not(Atom) | the 0-test | lowered (NOT EXISTS) |
| keyed decl | per-key B at boundaries | landed incl world-fed arrivals (keyed-divergence fix) |
| retention keep(count) | bounds the N accumulation | lowered (match lane) |

## 5. Theorems already shipped (each = a ring error made a refusal)

Every silent-wrong class found by the 2026-07-29 design review is a
construct reading the wrong object; each is now a load-time refusal
in BOTH engine.pl check_program/1 and analyze.pl, with fail-first
fixtures:

| refusal | ring error |
|---|---|
| finalize_in_level_rule | reading (dS)- from a body that sees only S |
| latest_in_level_rule | an N->B coercion applied where everything is already B (identity, so the word lies) |
| pre_in_level_rule | grade -1 read inside the grade-0 fixpoint (level ctx has no previous state) |
| log_on_level_headed_rel | declaring N-accumulation on a rel whose rows are computed in B and never stored: no delta channel exists |
| keyed_level_head | a per-key B boundary invariant on a plane that has no write events to replace at |

No semantic defect has yet been found WITHIN a plane; all five were
cross-plane placements. That is the empirical case that this line is
the language's spine.

## 6. What the checker does (phase 5 spec)

1. registry.pl gains two columns per construct: ring signature (what
   it reads/writes in {B, N, Z}) and tick grade.
2. Per rule body: every atom's ring must compose; an N-B junction
   requires an explicit coercion operator or is refused with the
   junction named (this generalizes the five theorems and would have
   caught A2 at design time).
3. Per rule-graph path: grades sum; a program's tick-offset table is
   derivable output (the answer to "what tick is this row on"), and
   cross-checks the oracle's observed tick placement in fixtures.
4. The refusal discipline is the enforcement: named term, both
   implementations, fail-first fixture. No warnings-only mode.
