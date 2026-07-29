# Clock Checker Proof-Payoff Lab

## Context

DL6 currently has a formal tick model, executable refusal checks, a reference
engine, an SQL emitter, and byte-for-byte temporal fixtures. It does not yet
have the general clock/cardinality checker described by the formal model.
`v6/prolog/compile/TICK-MODEL.md:3-8` says the general checker is pending.
`v6/prolog/ARCH.pl:680-681` records `tick_model` as done and `clock_check` as
unbuilt.

The existing language has three annotation domains:

```text
S  : Tick -> B-Rel       current set/level membership
O  : Tick -> N-Rel       occurrence/log multiplicity
dS : Tick -> Z-Rel       signed boundary change
```

The selected rule form and declaration metadata determine the domain.
Within-tick level evaluation is a least fixpoint over `B`; edge rules consume
positive or negative parts of `dS`; logs preserve occurrence multiplicity in
`N`. Rule-graph edges also have integer tick grades. These definitions are in
`v6/prolog/compile/TICK-MODEL.md:10-82`.

The audit compares that implemented world with three proof traditions:

- Rust references couple referent lifetime with aliasing restrictions. The
  Rust Reference states the operative principles for live shared and mutable
  references, while also stating that the exact aliasing rules are not fully
  determined:
  <https://doc.rust-lang.org/stable/reference/behavior-considered-undefined.html#undefined-alias>.
- Lustre assigns streams clocks, requires operands of data operators to share
  a clock, uses `pre` for prior values, and rejects cyclic data dependencies
  without delay. Source: Halbwachs, Caspi, Raymond, and Pilaud, *The
  synchronous data flow programming language LUSTRE*, 1991:
  <https://hal.science/hal-05028988v1/file/lustre.ieee91.pdf>.
- Esterel constructive semantics requires each instant's signal presence and
  absence to have constructive justification, including cyclic circuits.
  Source: Berry, *The Constructive Semantics of Pure Esterel*:
  <https://www.college-de-france.fr/media/gerard-berry/UPL1145408506344059076_Berry_EsterelConstructiveBook.pdf>.

These systems answer different questions. Rust checks access to memory places.
Lustre checks synchronous stream compatibility, initialization, and causal
schedule. Esterel checks constructive reaction inside an instant. DL6 checks
relational planes, signed row lifecycles, stratified absence, and tick
placement. A DL6 relation reference is an integer graph edge in SQLite, not a
borrowed memory address.

## Existing invariants

| Invariant | Static location | Executable evidence | Current strength |
|---|---|---|---|
| Level rules read current `B` state | `TICK-MODEL.md:15-24` | level fixtures and SQL/oracle final-state grades | Runtime semantics, described formally |
| Edge triggers consume signed boundary occurrences | `TICK-MODEL.md:26-48` | arrival, finalize, update, completion fixtures | Runtime semantics, fixture graded |
| Negated level dependencies are strictly lower strata | `compile/strat.pl:42-100` | stratifier tests and compiler refusals | Statically checked |
| Aggregates are strictly above every input | `compile/strat.pl:67-75` | aggregate fixtures | Statically checked |
| Current SQL lowering rejects positive recursive strata | `compile/strat.pl:103-138` | `recursive_stratum(Refs)` refusal | Statically checked capability boundary |
| `finalize`, `latest`, and `pre` cannot read from a level rule | `compile/analyze.pl:1020-1065` and shared program checks | paired oracle/compiler fail-first fixtures | Statically checked named cases |
| Log retention and keyed declarations have plane restrictions | shared program checks plus `analyze.pl:1027-1066` | conformance refusal fixtures | Statically checked named cases |
| Keyed positive rows replace by key; negative rows retract the exact row | `chat_log/20260729.4.rel-edge-clock-fixpoint.pl` | `stale_keyed_retraction_keeps_replacement` | Runtime and emitted-SQL graded |
| Edge stages written by edge rules advance one tick | `TICK-MODEL.md:50-66` | `edge_chain_hops_tick_per_stage`, `pipe_stage_costs_one_tick` | Runtime and emitted-SQL graded |
| Host/bind rows enter on an observed world tick | registry bind rows and served runtime | bind and host fixtures | Runtime graded for registered executors |

## What is statically provable now

1. A level program has no negative dependency cycle accepted by the Prolog
   stratifier.
2. An aggregate cannot observe a relation in its own or a later stratum.
3. The current emitted SQL subset has an acyclic positive rule order within
   each stratum. Positive recursion is refused by the emitter.
4. Five cross-plane placements are rejected by named checks:
   `finalize_in_level_rule`, `latest_in_level_rule`,
   `pre_in_level_rule`, `log_on_level_headed_rel`, and
   `keyed_level_head`.
5. Key positions are valid, unique column positions after the relation-edge
   fixpoint work.
6. Declared relation and expression shapes satisfy the current supported
   subset gate before SQL emission.

The checker cannot currently derive a clock expression or tick offset for
every relation. `registry.pl` has surface axis and lowering roles, without the
ring signature and grade columns specified in `TICK-MODEL.md:103-115`.

## What is checked by execution or referee

1. Exact tick placement for edge chains, finalize, update, and carried rows.
2. Glitch behavior at the arrival/level seam.
3. Last-write behavior for several keyed writes in one boundary.
4. Equality between the Prolog oracle tick log and emitted SQLite/TypeScript
   tick log.
5. Final relational state, where fixtures include a final-state grade.
6. Host response and bind cadence for the concrete registered executors.

These receipts prove the listed programs. They do not constitute a
program-wide theorem for new rule graphs.

## Counterexamples and proof gaps

| Program shape | Current result | Missing proof |
|---|---|---|
| Join an arrival occurrence with current set membership | Accepted in supported shapes | No inferred `N × B -> N` cardinality displayed to the user |
| Two paths reach one relation with different tick grades | Can be fixture graded | No static clock-offset conflict diagnostic |
| Zero-grade positive recursive level SCC | Oracle can compute a `B` least fixpoint; SQL emitter refuses it | No shared capability theorem selecting one implementation boundary |
| Zero-grade cycle containing absence or an occurrence-sensitive operator | Some instances refused by stratification or named gates | No general labelled-SCC causality proof |
| Read prior state on the first boundary | `pre` is refused in current emitted subset | No initialization calculus exists if `pre` later becomes live |
| Host demand whose provider never responds | Program remains waiting or quiescent according to runtime behavior | No static completion or liveness proof |
| Concurrent host answers for the same key | SQLite transaction and keyed arrival behavior decide the result | No schedule-independence proof |
| Parent relation stores a target row ID that later disappears | Relation-edge lab is still defining the lifecycle | No static borrow-style lifetime or boundary referential-integrity proof |
| Multiple keyed writes in one batch | Ordered replacement semantics are executable | No confluence theorem across alternate batch orderings |

### Concrete Rust comparison

Rust's borrow checker prevents a reference from outliving its referent and
prevents aliasing of a live mutable reference. DL6 rows are persistent values
selected by relations. Multiple readers of one row require no alias control.
Writes occur as signed rows at a serialized tick boundary. The useful
borrow-checker analogue is therefore:

```text
parent endpoint is live at boundary t
    implies target membership is live at boundary t
```

That property is an antijoin invariant over ordinary relations. It can be
checked at each transaction boundary and maintained incrementally. It does
not require ownership or borrow syntax. A stronger cascade, ownership, or
exclusive-reference rule would change current relation semantics and lacks a
current-world proof case.

### Concrete Lustre comparison

DL6's rule-graph grade is the closest existing object to Lustre's clock
calculus. Lustre clock equality corresponds to requiring all inputs of one
instantaneous operation to denote compatible tick expressions. `pre` and
`@next` correspond to delayed dependencies. DL6 additionally carries
relational annotation domains `B`, `N`, and `Z`, so compatibility requires
both clock-grade unification and ring composition.

The current named refusals are manually encoded instances of that combined
calculus. General inference is absent.

### Concrete Esterel comparison

DL6's within-tick `B` least fixpoint provides constructive evidence for
monotone positive facts. Stratified negation ensures absence is read only
after a lower stratum settles. This supplies a restricted constructive
reaction model.

Esterel-style causality becomes relevant for a same-tick SCC containing
absence, lifecycle sign inspection, sampling, an effect result, or another
nonmonotone dependency. Current checks cover named placements and negative
stratification. They do not label every dependency and prove each SCC
constructive.

## Minimal checker extensions ranked by proof payoff

| Rank | Internal extension | Signature and body sketch | Proof payoff | Surface cost |
|---:|---|---|---|---|
| 1 | Label each rule dependency with ring, sign, and grade | `edge_fact(Rule, From, To, ReadRing, WriteRing, Sign, Grade)`; project facts from existing AST and registry rows | Generalizes the five manual cross-plane refusals; makes every join and delay inspectable | None |
| 2 | Infer relation clock offsets and reject incompatible paths | `infer_clock(Edges, Rel, ClockExpr)`; unify path sums, report both paths on conflict | Static tick placement for arbitrary acyclic graphs; checks the semantics now proved fixture by fixture | None |
| 3 | Labelled SCC causality check | `check_scc(Scc, Edges)`; accept monotone `B` zero-grade closure, require positive delay for recurrence, reject zero-grade negative or occurrence-sensitive cycles | Restricted Lustre/Esterel causality theorem over the current language | None |
| 4 | Boundary referential-integrity invariant | `dangling(ParentId, TargetId) <- parent(...), not target_id(TargetId)`; execute as an incremental antijoin before commit | Rust-lifetime analogue for relation edges at durable boundaries | None |
| 5 | Export proof facts and compare them with runtime receipts | `clock_fact(Rel, Ring, Offset, Scc, Constructive)`; fixtures compare inferred offsets with tick logs | Turns hidden checker output into queryable evidence and catches checker/runtime drift | None |

Rank 1 requires extending internal registry metadata. It can be derived from
existing declaration and rule forms. No new keyword, annotation, arrow, or
clock literal is required.

## Type signatures, timelines, and storage

Proposed internal signatures:

```prolog
relation_plane(+Program, +RelRef, -Plane).
rule_dependency(+Program, -RuleId, -FromRef, -ToRef,
                -ReadRing, -WriteRing, -Sign, -Grade).
infer_clock_offsets(+Dependencies, -Offsets).
check_constructive_sccs(+Dependencies, -Diagnostics).
boundary_reference_violations(+Program, -InvariantPlans).
```

Body sketches:

```prolog
relation_plane(Program, Ref, Plane) :-
    % Derive from existing rel declaration, log/retention metadata,
    % and whether Ref is a level-only head.

rule_dependency(Program, RuleId, From, To, RR, WR, Sign, Grade) :-
    % Walk the existing expanded rule body once.
    % Classify the head plane and each use from existing syntax/registry facts.

infer_clock_offsets(Dependencies, Offsets) :-
    % Collapse zero-grade SCCs, then propagate integer path sums.
    % Two unequal sums reaching the same clock variable produce one named
    % diagnostic carrying both paths.

check_constructive_sccs(Dependencies, Diagnostics) :-
    % Zero-grade SCC: accept only monotone B dependencies.
    % Delayed SCC: accept when every cycle has positive total grade.
```

Instance timelines:

1. Parser and expansion produce the current AST.
2. Existing declaration and supported-subset checks run.
3. The checker projects labelled dependency facts.
4. Zero-grade SCCs are classified before SQL generation.
5. Clock offsets are inferred over the SCC condensation graph.
6. The emitter consumes the checked plan.
7. At each runtime tick, outside signed rows are absorbed, level state reaches
   its supported closure, edge occurrences execute, and boundary deltas are
   recorded.
8. Optional proof receipts compare inferred offsets with observed tick logs.

Storage:

- Checker facts live for one compilation and can remain Prolog terms.
- Emitted proof facts, if requested by a test or diagnostic tool, are a small
  table keyed by `(program_id, relation_ref)`.
- Runtime relation data remains in the existing SQLite tables.
- Boundary reference checks compile to antijoins and execute once per changed
  target or parent batch, not once per parent row.
- Uniqueness remains relation `key(...)` plus row identity. Clock inference
  adds no durable identity.

## Decisions

### D1: first checker increment

1. **Label dependencies, then infer clocks.** Adds internal metadata and proof
   output with no surface syntax. This is the plan's selected sequence.
2. Add more placement-specific refusals. Lower implementation cost per case;
   proof coverage continues to depend on enumerating constructs.
3. Infer clocks without rings. Tick conflicts become visible; occurrence/set
   composition remains unchecked.
4. Infer rings without clocks. Cardinality mismatches become visible; temporal
   path conflicts remain fixture-only.
5. Retain runtime receipts only. Language behavior stays unchanged; new
   programs receive no general static theorem.

<!-- todo(decision): D1 requires user selection among labelled dependency inference, more named refusals, clocks only, rings only, or runtime receipts only -->

### D2: positive zero-grade recursion

1. **Accept in the calculus as monotone `B`; keep SQL lowering refusal until a
   fixpoint emitter lands.** Checker and backend capability diagnostics remain
   separate. This is the plan's selected hypothesis.
2. Reject in the language. Oracle behavior and relational expressiveness
   narrow to the current SQL backend.
3. Lower every positive SCC through a SQLite fixpoint. Backend work expands
   before the checker theorem is independently graded.
4. Permit only self recursion. Mutual recursion remains refused despite having
   the same monotone proof shape.

<!-- todo(decision): D2 requires user selection for monotone positive zero-grade recursion and the current SQL backend boundary -->

### D3: relation-edge lifetime enforcement

1. **Boundary antijoin refusal.** A transaction with a live parent and absent
   target fails by a named invariant; no surface syntax. This is the plan's
   selected hypothesis pending the relation-edge lifecycle lab.
2. Allow dangling endpoints. Reads use ordinary joins and omit missing
   targets; integrity becomes query policy.
3. Retract parents automatically with targets. This selects cascade ownership
   semantics for every relation edge.
4. Retain target rows while referenced. This adds reference-counted lifetime
   semantics to relation membership.
5. Declare policy per edge. This adds language surface before an existing
   program proves more than one policy is required.

<!-- todo(decision): D3 remains gated on the active relation-edge lifecycle lab and requires user selection only after its missing-target and retraction receipts -->

### D4: host liveness

1. **Exclude completion from the static theorem.** The checker proves clock and
   causality for rows that arrive; provider liveness stays an operational
   metric and timeout policy.
2. Require every provider to declare finite or streaming completion. Adds
   internal provider metadata and checks composition with lifecycle arms.
3. Require timeouts for finite providers. Adds policy to every effect request.
4. Model provider completion as ordinary response relations. Adds relation
   rows but cannot prove an external process will emit them.

<!-- todo(decision): D4 requires user selection for the boundary of the clock theorem around external provider liveness -->

## Verification

The checker increment is accepted only with these executable receipts:

1. Registry inventory test asserts every live temporal or join construct has
   one ring/grade classification.
2. Existing five cross-plane refusals emerge from the general checker with the
   same diagnostic terms.
3. `edge_chain_hops_tick_per_stage` and `pipe_stage_costs_one_tick` inferred
   offsets equal their observed tick logs.
4. A diamond with equal path grades passes; a diamond with unequal path grades
   refuses and reports both paths.
5. A monotone positive `B` SCC is classified constructive; a zero-grade SCC
   containing negation, finalize, latest, or occurrence-sensitive input
   refuses.
6. A delayed recurrence is classified productive only when every cycle has
   positive total grade.
7. Parent/target boundary antijoin receipts cover parent-first, target-first,
   same-batch, target retraction, parent retraction, and keyed replacement.
8. Oracle/compiler diagnostic parity remains exact.
9. Conformance and emitted-SQL sweep remain at or above the branch baseline:
   compiler units 147 PASS, conformance 164 PASS, sweep 164 total with 103
   compiled, 61 named unsupported, and zero compiler crashes.

## Staffing

- Research and plan: one Sol agent, shared worktree, base `902d53b7`.
- Implementation of ranks 1 and 2: one Codex agent after D1, same branch or a
  dedicated worktree, with ownership of `registry.pl`, a new checker module,
  analyzer integration, and focused fixtures.
- Relation-edge boundary invariant: remains in the active
  `rel_edge_clock_fixpoint` lane to avoid overlapping compiler edits.
- Suite budget: focused Prolog units after each internal step; full
  conformance and both emitter sweep modes before merge.
- No behavior rewrite, host rewrite, or surface migration belongs to this
  plan.
