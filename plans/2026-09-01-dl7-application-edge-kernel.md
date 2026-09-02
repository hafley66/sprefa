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
`v7/src/2_comptime/0_lowerer.pl:181-233`,
`v7/src/2_comptime/2_compiler.pl:220,330,820-829`,
`v7/prelude/3_derived_rules.dl7:173-275`, and
`v7/src/2_comptime/1a_generated_program_assembler.pl:148-288`.

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
8. Curry carrier removal requires the application graph to be complete at the
   start of a compiler round. Reading live `:/4` edges creates an aggregate
   dependency cycle through `curry_rank`; reading `edge_snapshot/4` while the
   source lowerer still emits call edges exhausted the 16-round source-refreeze
   limit. The next migration must move call-edge production across that phase
   boundary before replacing the carriers.
9. Ground call-owned `:/4` fact heads are promoted into the frozen edge set at
   the start of the same compiler round. Generated type edges continue through
   the ordinary end-of-round freeze. `curry_specialization/2` and
   `curry_bound/4` are now DL7-derived views over `Apply/2`, frozen call edges,
   callable signature edges, and `Literal/3`.

### Value-node checkpoint

The additive value-node signatures are:

```text
Literal(Node, PrimitiveType, RawValue)
Variable(Node, Scope, Name)
```

Their instance lifetimes and identities are:

- A literal node is content-interned by `(PrimitiveType, RawValue)` and may be
  shared by every equal occurrence in the compiler closure.
- A generated variable node is interned by `(Scope, Position, Name)`. Its scope
  is the generated callable relation, so equal names in different callables do
  not capture one another.
- A relation or type reference continues to use its existing node directly.

Storage and flow remain separate. `Literal/3` and `Variable/3` are ordinary
prelude rows in the compiler graph. Application `:/4` edges point to those
nodes. The legacy `const`, `var`, `curry_bound`, `head_arg`, and `body_arg`
terms remain beside them until parity is proved. Runtime emitters eventually
unbox literals and erase compiler-only variable metadata.

The first implementation changes only escaping-partial call edges and
generated Curry variables. It adds no evaluator primitive and no macro syntax.

### Round-boundary checkpoint

The source lowerer now emits one application occurrence graph:

```text
Apply(Call, Callable)
:(Call, supplied label, ValueNode, source position)
:(Call, return label, Result, return position)
```

`source_application_edges/2` recognizes the ground call-owned edge fact heads
and includes them in the next evaluation's initial `edge_snapshot/4` input.
This supplies the complete source application graph without exposing generated
type edges early.

The DL7 prelude derives:

- `curry_call/4` by binding the return label from the declared third edge of
  `Curry` and joining it to a call occurrence;
- `curry_specialization/2` from that call;
- `curry_bound/4` from supplied call edges, using `Literal/3` to recover raw
  constants and direct node targets for references;
- the existing generated callable shape and forwarding rule from those views.

Direct source lowering of the Curry fixture now emits three initial `Apply`
rows and zero `curry_specialization` or `curry_bound` rows. The latter two
relations remain compatibility views for `body_arg/5` until the generated-rule
application migration removes that carrier.

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

The value-node checkpoint passes 38 of 38 SWI tests and the one tree-sitter
corpus parse. Application edges now target direct relation references or
boxed literals, and generated Curry variables have scoped interned nodes.

The round-boundary checkpoint passes 39 of 39 SWI tests and the one tree-sitter
corpus parse. The final full-suite Curry test consumed 16.766 seconds versus
11.829 seconds at the value-node checkpoint, a local one-run increase of 4.937
seconds. No evaluator primitive or CI configuration changed.

The generated-rule checkpoint represents generated heads and goals with
`HeadCall`, `BodyCall`, `Apply`, and ordinary `:` edges. `head_arg/4` and
`body_arg/5` have been removed. The complete V7 SWI suite passed 39 of 39 tests
at that checkpoint.

The compiler trace and cache checkpoint adds phase and fixpoint-step metrics,
content-keyed prelude and complete-compile caches, and relation-indexed
temporary evaluator facts. The complete V7 SWI suite passes 41 of 41 tests in
59.5 seconds; the Tree-sitter corpus passes 1 of 1 tests. A cold Partial fixture
compile measured 12.882 seconds, with 10.363 seconds in comptime. The repeated
content-cache compile measured 45 milliseconds inside the cache step before
removing an unrelated explicit post-compile garbage collection.

## Staffing

- Implementation: Codex directly, high reasoning.
- Worktree: `/private/tmp/sprefa-v7-value-nodes`.
- Branch: `feature/v7-generated-application-nodes`.
- Base SHA: `28c889b0cc88678aa3383ef7f7cf7be71718d373`.
- Suite budget: focused curry tests per checkpoint; complete V7 suite before
  push.
