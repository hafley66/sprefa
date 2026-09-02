# DL7 Application Edge Kernel

## Context

V7 now generates callable partial specializations, refreezes dependent source,
applies relation-valued returns, and exposes the closed compiler graph to DL7
and Prolog emitters. The implementation represents the same application data
several times:

- `curry_specialization/2` records the callable and specialization.
- `curry_bound/4` records an argument position, kind string, and value.
- `head_arg/4` and `body_arg/5` repeat argument kind and value.
- `application(Constructor, Arguments)` embeds the same constructor and
  argument graph inside the semantic identity.

The relevant producers and consumers are
`v7/src/2_comptime/0_lowerer.pl:190-240`,
`v7/prelude/3_derived_rules.dl7:171-233`, and
`v7/src/2_comptime/1a_generated_program_assembler.pl:270-335`.

The application graph should use the existing edge relation. A call occurrence
is a node, `Apply/2` connects it to its callable, and its outgoing `:/4` edges
carry supplied and derived tuple values at callable positions.

## Decisions

1. The normalized application graph is:

   ```text
   Apply(Call, Callable)
   :(Call, Label, ValueNode, Position)
   ```

   A functional result is the ordinary edge whose label is `return`.
2. A source call node has occurrence identity while arguments are incomplete.
   A generated type or callable result retains its existing content-interned
   semantic identity.
3. Signature edges belong to the callable node. Supplied tuple edges belong to
   the call node. Their shared labels and positions permit direct joins.
4. Compiler variables and primitive literals eventually become boxed value
   nodes. References already target nodes. Boxing removes the remaining
   `Kind + Value` duplication without confusing a variable name with an equal
   text literal.
5. Rule heads and body goals eventually become application nodes. `Head/2` and
   `Body/4` retain rule structure while `head_arg` and `body_arg` disappear.
6. Migration is additive before it is subtractive: emit the application graph,
   prove parity, switch one consumer, then remove its legacy carrier.
7. Helper relations such as `curry_unbound`, `curry_rank`, and `curry_edge` may
   remain derived views. They do not own application state.

Rejected alternatives:

- Deleting `Kind` immediately loses the distinction between generated
  variables and equal primitive literals.
- Reusing the specialization result as the call node mixes supplied edges with
  its generated callable signature and creates competing ordinal spaces.
- Adding `supplied_slot/4` repeats the existing `:/4` tuple exactly.

## Migration sequence

```text
source call
    │
    ▼
Apply(Call, Callable)
    │
    ├── :(Call, input label, supplied value, source position)
    └── :(Call, return, result value, return position)
              │
              ▼
       userland completion rules
              │
              ▼
generated callable signature and forwarding rule
              │
              ▼
fixed-arity Datalog
```

<!-- todo(feature): Emit canonical Apply/2 call nodes and ordinary argument edges beside the current Curry carriers, with exact parity coverage. -->
<!-- todo(refactor): Derive Curry specialization and bound-slot views from application nodes and edges, then remove lowerer-authored curry_specialization and curry_bound rows. -->
<!-- todo(feature): Box compiler variables and primitive literals as value nodes so generated argument carriers no longer store a separate kind string. -->
<!-- todo(refactor): Represent generated rule heads and body goals as application nodes and remove head_arg and body_arg from the assembler protocol. -->
<!-- todo(refactor): Replace structural application(Constructor, Arguments) identities with opaque interned node identities plus Apply and argument edges. -->

## Verification

- Every escaping partial emits one call node, one `Apply/2` row, one edge per
  supplied input, and one return edge.
- Joining a call edge to its callable signature by label and source position
  recovers the existing `curry_bound` rows.
- Two equivalent pure applications produce equal semantic result identities;
  separate source occurrences retain separate call identities.
- Generated forwarding rules remain byte-for-term equal through each additive
  migration checkpoint.
- Compiler rows and runtime IR remain deterministic across two compilations.
- Runtime IR contains no application graph transport after monomorphic
  emission.
- The complete V7 SWI suite and tree-sitter corpus pass before merge.

## Staffing

- Implementation: Codex directly, high reasoning.
- Worktree: `/private/tmp/sprefa-v7-application-edge-unification`.
- Branch: `feature/v7-application-edge-unification`.
- Base SHA: `efa7930cc71c09b0a40bed03801b6b75876bca0b`.
- Suite budget: focused curry tests per checkpoint; complete V7 suite before
  push.
