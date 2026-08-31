# Deferred DL7 expression-bind review

Date: 2026-08-30

Scope: `d8d6e4792` on `feature/dl7-count-aggregate`. Production code and tests
were unchanged.

## Findings

| Question | Finding |
| --- | --- |
| Ground compiler IR | Yes, provided every fresh expression value is encoded as a reified term such as `var(expression_return(NodeId))` or `var(derived_lookup(NodeId))`. A native Prolog variable would violate the existing contract. |
| Duplicate labels and dense indices | Yes for declared slots. A compiler-only declaration marker can share the current pending-declaration list with `pending_edge/4`; one accessor can project both shapes to `(Owner, Name, Index)`. The checker emits only static markers as canonical `:/4` edges. |
| Chained binds | Yes. `B` lowers its reference to `A` before the `Option/2` goal. The two bind rules form positive recursion on `:/4`, which the tabled evaluator closes independent of source or clause order. Refreeze is not needed for this direct lookup. |
| Functional keys and rounds | One result is stable. Two or more results for one bind violate both `:/4` keys and are rejected at stable closure. Zero results produce no key conflict and expose a separate materialized-density gap. Snapshot-dependent RHS relations retain the existing possibility of reaching the 16-round limit. |
| First-order erasure | Yes. The checked forms remain `rule/2`, `checked_goal/2`, `call(ref(Relation), Arguments)`, and reified `var/1`. Derived operator position remains unavailable through milestones 1 through 5. |
| Smallest invariant repair | Keep the proposed rule representation, tag its head variable with the bind node identity, and validate exactly one matching colon row at stable closure. If lookup is required to occur strictly after refreeze, emit the existing `edge_snapshot/4` call instead of `:/4`; this adds one compiler round per dependency layer. |

## 1. Groundness

`lower_datalog/4` requires a ground reader unit and documents ground compiler
data at `v7/src/2_comptime/0_lowerer.pl:8-16`. Existing authored variables are
already encoded as `var(Identity)` by `lower_argument/4` at :345-363.
Generated-program variables use the same representation in
`generated_argument_result/5` at
`v7/src/2_comptime/1a_generated_program_assembler.pl:308-320`.

The derived bind should therefore lower to ground data of this shape:

```prolog
rule(
  call(name(Owner, ':'),
       [ ref(Owner), const(Name),
         var(derived_bind(BindNodeId)), const(Index)
       ]),
  Goals)
```

A derived atom lookup similarly uses
`var(derived_lookup(AtomNodeId))`. Reader node identities make these variables
fresh, ground, and deterministic across consecutive compiler runs. The
canonical-run assertions at `v7/test/1_entrypoints.test.pl:43-57` and :59-107
would detect a process-global `gensym/2` identity leak.

Groundness is checked again for generated relations and rules by
`check_resolved_rules/5` at `v7/src/2_comptime/1_checker.pl:49-73`, and for
evaluation inputs by `evaluate/4` at
`v7/src/1_libtime/0_evaluator.pl:24-35`. Native SWI variables appear only
during proof execution in `instantiate_rule/3` and `instantiate_call/4` at
`0_evaluator.pl:500-518`.

## 2. Declaration validation without another edge relation

The current validation path consumes only `pending_edge/4`:

```text
check_datalog/4
  -> bind_diagnostics/3
     -> duplicate_bind_diagnostics/4
     -> duplicate_index_diagnostics/4
     -> dense_index_diagnostics/4
        -> count_owner_edges/3
```

These predicates are at `v7/src/2_comptime/1_checker.pl:18-34` and :198-249.
They can consume mixed static and derived declarations through one compiler
accessor:

```prolog
declaration_slot(pending_edge(Owner, Name, _, Index),
                 Owner, Name, Index).
declaration_slot(derived_declaration(Owner, Name, _, Index),
                 Owner, Name, Index).
```

`resolve_edges/6` at `1_checker.pl:251-261` should resolve
`pending_edge/4` and skip `derived_declaration/4`. `finish_checked/9` at
:136-153 then preserves its existing output invariant: `root_graph/2`
contains concrete canonical edges only. `graph_seeds/2` and `edge_seed/2` at
`v7/src/2_comptime/2_compiler.pl:294-310` require no marker case.

`constructor_relations/4` and `edge_owned_by/2` at
`v7/src/2_comptime/0_lowerer.pl:141-146` must count declaration slots rather
than static edges, because a derived field still contributes to product
arity. Static parent traversal remains on concrete constructor bindings in
`parent_owner/3` at `1_checker.pl:287-288`.

This checks duplicate names, duplicate ordinals, and declared density across
mixed static and derived binds. There are currently zero entrypoint assertions
for `duplicate_bind/2`, `duplicate_bind_index/2`, or `non_dense_index/2`.
`checked_edge_indices_expose_adjacent_and_strict_order` at
`v7/test/1_entrypoints.test.pl:336-371` covers concrete predecessor rows.

Declared density does not by itself prove materialized density. If a derived
RHS returns zero rows, index `I` has a declaration marker and no `:/4` row.
`predecessor_seeds/2` at `1_checker.pl:319-328` and
`frozen_predecessor_rows/2` at `2_compiler.pl:261-271` currently derive
adjacency from concrete edge indices. A later concrete index can therefore
create a predecessor pair across the missing derived edge.

The smallest preservation check is exact-one validation at stable closure.
The head identity `var(derived_bind(BindNodeId))` lets the compiler recover
every expected `(Owner, Name, Index)` directly from authored rules, without a
new runtime relation or a checked-program shape change. Zero matches produce a
positioned missing-derived-bind diagnostic; multiple matches are already a
functional-key failure.

## 3. Chained bind execution and refreeze

For:

```text
(: A (Partial User))
(: B (Option A))
```

the required checked rule bodies are:

```text
:(Owner, A, AResult, AIndex) <- Partial(User, AResult).

:(Owner, B, BResult, BIndex) <-
    :(Owner, A, AValue, AIndex),
    Option(AValue, BResult).
```

`lower_goals/7` currently retains authored order at
`v7/src/2_comptime/0_lowerer.pl:240-255`. Generated expression goals must use
the same inner-first order. `check_goal_sequence_failures/7` at
`v7/src/2_comptime/1_checker.pl:466-504` treats ordinary positive calls as
binding all of their variables through `check_goal/4` at :545-548.

Both bind heads target `ref(kernel(':'))`, so the second body lookup is a
zero-gap positive self-dependency. `stratify_rules/3` and
`dependency_gap/4` admit that recursion at
`v7/src/1_libtime/0_evaluator.pl:242-292`. `proves/2` and `proves_body/2` at
:427-456 run it under tabled evaluation. Rule order is not a semantic
dependency.

The current refreeze path has a narrower role:

```text
RoundClosure
  -> colon_rows/2
  -> FrozenEdges
  -> snapshot_edge/2
  -> edge_snapshot/4 seeds in the next round
```

The predicates are `continue_compiler_rounds/15` at
`v7/src/2_comptime/2_compiler.pl:171-190`, `compiler_round_seeds/4` and
`snapshot_edge/2` at :244-256, and `colon_rows/2` at :288-292. Frozen colon
rows are not seeded back as `:/4`; each derived bind rule rederives them.
Consequently, direct `:/4` lookups can close a bind chain in the same evaluator
round. Refreeze exposes those rows to userland `edge_snapshot/4` consumers on
the following compiler round.

The existing prelude exercises both sides. `Partial/2` and `Option/2` are
declared with `return` at `v7/prelude/0_types.dl7:2-8`, and their executable
rules are at :103-123. `Partial/2` still has the temporary
`partial_request/1` gate at :105; its removal remains milestone 6. The current
compiler-round oracle is
`userland_type_operators_chain_across_compiler_rounds` at
`v7/test/1_entrypoints.test.pl:59-108`, with result inspection in
`type_operator_snapshot/2` at :606-657.

## 4. Functional-key and stability cases

`kernel_relation_keys/2` declares `:/4` keys `[0,1]` and `[0,3]` at
`v7/src/2_comptime/1_checker.pl:302-303`. A derived bind fixes owner, name,
and index while leaving only target variable.

| Stable RHS result count | Compiler result |
| ---: | --- |
| 0 | No colon row and no functional-key diagnostic. Exact-one validation is required to preserve concrete edge density. |
| 1 | One row satisfies both keys. Repeated derivation is deduplicated by `sort/2`. |
| 2 or more distinct targets | Rows collide on both keys. `validate_functional_rows/3` reports through `functional_key_conflict/4`. |

Functional validation is implemented at
`v7/src/1_libtime/0_evaluator.pl:188-240`. Compiler rounds call it after the
frozen edge, intern, and generated-program sets become equal in
`continue_after_assembly/19` at `v7/src/2_comptime/2_compiler.pl:194-227`,
then the final closure is checked again by `finish_evaluation/11` at :94-105.
The direct key-conflict receipt is
`final_closure_rejects_declared_functional_key_conflicts` at
`v7/test/1_entrypoints.test.pl:110-127`.

`colon_rows/2` sorts each round's edge set before the equality comparison at
`2_compiler.pl:177-205`, so duplicate proofs do not make a round unstable. A
relation whose result changes in response to the preceding
`edge_snapshot/4` can alternate edge sets. That behavior already exists for
compiler rules over snapshots and ends at `compiler_round_limit(16)` through
`:213-225`. Milestone 8's expression-mode key proof prevents structurally
underdetermined calls; stable-closure key validation remains the check against
incorrect userland key claims.

## 5. Exact edit map for milestones 1 through 5

| Milestone | Predicates requiring edits or additions |
| --- | --- |
| 1. Expression carrier | In `0_lowerer.pl`, add `lower_expression/7` and result combinators; route the atom, literal, variable, and rejected-form seams now split between `lower_target/4` (:104-123) and `lower_argument/4` (:345-363). Fresh values use deterministic reified identities. |
| 2. Declared return | Widen `reservation/4`, constructed by `finish_bind/6` (:84-94), to retain `Index`, or pass declaration markers into executable lowering. Update the two reservation matches in `lower_call_mode/6` (:284-313). Add callable resolution and exactly-one-return predicates over the complete reservation table. Add kernel return metadata beside `kernel_relation/2` (:381-396), whose declaration edges currently live later in `kernel_graph/2` at `1_checker.pl:330-394`. Leave the full-arity `lower_call_mode/6` path unchanged. |
| 3. RHS call bind | In `0_lowerer.pl`, split `lower_bind/5` and `finish_bind/6` (:76-94) into static-edge and derived-marker results; update `continue_declarations/6`, `lower_bind_list/5`, `continue_bind_list/6`, `constructor_relations/4`, and `edge_owned_by/2` (:61-74, :141-167) for the widened declaration shape. Change `lower_executables/5` and its worker (:169-190) to emit a derived bind rule instead of skipping every bind; reuse `continue_rule/8` and `prepend_rule/4` (:192-202) so rule indices and origins include it. In `1_checker.pl`, generalize `bind_diagnostics/3` and its four walkers (:198-249), and make `resolve_edges/6` (:251-261) skip derived markers. `resolve_call/5` (:602-623), `resolve_argument/4` (:625-650), and head safety (:652-679) already accept the resulting ordinary colon rule. |
| 4. Nested applications | In `0_lowerer.pl`, add recursive expression-argument lowering that concatenates each argument's prerequisite goals before the enclosing relation goal. The atom arm checks `derived_reservation(Owner, Name, BindNodeId, Index)` first and emits the positive colon lookup; static atoms retain `name(Owner, Name)`. The call arm reuses reservation and relation arity data currently read by `lower_call_mode/6` (:284-313). No checker or evaluator predicate needs a new IR case. |
| 5. Uniform nested positions | Change `lower_head_call/5`, `lower_call/5`, `lower_call_mode/6`, `finish_call_arguments/6`, `lower_arguments/4`, `lower_argument/4`, and `aggregate_argument_result/2` at `0_lowerer.pl:278-367` to return prerequisite goals and origins with the call/value. Change `lower_rule/8` (:226-238) to prepend head prerequisites to the body. Change `lower_goal/5` and `lower_goals/7` (:240-276) to splice prerequisite goals before each positive or negative enclosing call and assign consecutive goal-origin indices after expansion. The `count` path must hoist its expression goals while retaining `aggregate(count, Value)` in the head. `head_safety_diagnostics/5` at `1_checker.pl:652-679` and aggregate proof evaluation at `0_evaluator.pl:114-186` then consume ordinary checked goals unchanged. |

Milestones 1 through 5 require no new evaluator, generated-program assembler,
or compiler-round IR case. `2_compiler.pl` needs only the stable exact-one
validation described above. The milestone-5 entrypoint seam is
`count_groups_completed_lower_proofs_and_rejects_bad_placement` and
`nested_head_receipt/1` at `v7/test/1_entrypoints.test.pl:405-494`.

## 6. Smallest alternative

The proposed representation fits the ground and first-order invariants after
adding declared-slot handling and stable exact-one validation. Its direct
colon lookup has same-round recursive semantics.

If the required semantic boundary is specifically post-refreeze lookup, use:

```text
edge_snapshot(Owner, Name, Value, Index)
```

for a bare derived atom. `edge_snapshot/4` already has the same two keys at
`1_checker.pl:303`, and `snapshot_edge/2` already populates it. This changes no
runtime apply surface and introduces no new relation. A dependency chain then
advances one bind per compiler round and remains subject to the current
16-round limit. Exact-one validation remains necessary because snapshot
lookup also cannot distinguish a missing bind from a relation with zero
answers.
