# Type Semantics Index

## Contents

1. Protocol position
2. Identity and graph kernel
3. Shared and language-native semantics
4. Example extraction
5. Queries, projections, and losses
6. Connection to the compiler clocks
7. Open decisions

## 1. Protocol position

The **Type Semantics Index**, abbreviated **TSI**, sits beside SCIP's source
identity graph. SCIP-style rows
identify symbols, definitions, references, occurrences, source ranges, and
declaration relationships. Semantic type facts identify the types attached to
those symbols and preserve the operations and proofs supplied by each source
language's checker.

```text
source bytes
    |
    +--> syntax extractor ----------> CST, spans, written declarations
    |
    +--> native compiler/checker ---> resolved symbols and native type meaning
                                           |
                         +-----------------+-----------------+
                         |                                   |
                         v                                   v
              SCIP-like symbol facts                   TSI facts
                         |                                   |
                         +-----------------+-----------------+
                                           |
                                           v
                                  DL7 comptime closure
                                           |
                         +-----------------+-----------------+
                         |                                   |
                         v                                   v
                cross-language queries             target projections
```

OpenAPI, JSON Schema, GraphQL metadata, and similar schema formats enter and
leave through adapters over this graph. They describe a projection of the
graph's wire or service surface. The graph retains language semantics that
those formats have no vocabulary for.

## 2. Identity and graph kernel

The common protocol is an open set of relational facts. Its small graph kernel
uses stable IDs and ordered labeled edges.

```text
tsi.type(TypeId)
tsi.denotes(SymbolId, TypeId)
tsi.has_type(OccurrenceId, TypeId)
tsi.origin(TypeId, Language, SourceRange)

tsi.product(TypeId)
tsi.sum(TypeId)
tsi.callable(TypeId)
tsi.primitive(TypeId, PrimitiveClass)

tsi.edge(EdgeId, OwnerTypeId, Label, TargetTypeId, Position)

tsi.parameter(ParameterTypeId, ConstructorTypeId, Position, Variance)
tsi.called(ResultTypeId, CalleeTypeId, ArgumentListId)
tsi.argument(ArgumentListId, Position, ArgumentTypeId)

tsi.input(CallableTypeId, Position, InputTypeId)
tsi.output(CallableTypeId, Position, OutputTypeId)
```

`tsi.denotes/2` connects a declaration symbol to the type that symbol names.
`tsi.has_type/2` connects a source occurrence to the semantic type assigned by
the native checker. The occurrence remains keyed by its SCIP document and
range identity, so TSI does not duplicate the source index.

These rows describe graph topology. Optionality, mutability, visibility,
ownership, lifetimes, effects, defaults, key roles, serialization choices, and
other edge semantics are additional facts keyed by `EdgeId` or `TypeId`.

Identity follows four rules:

1. A nominal source type derives its identity from the resolved source symbol.
2. An anonymous structural type derives its identity from its closed ordered
   edge graph.
3. A type call result derives its identity from its callee and ordered
   argument IDs.
4. A generic parameter derives its identity from its declaration symbol and
   ordinal.

The protocol can carry source-local IDs before closure. The comptime interner
produces canonical IDs once the identity inputs are ground.

## 3. Shared and language-native semantics

Common facts record semantics shared across language families. Namespaced fact
families preserve native rules that do not have one common interpretation.

```text
Common graph facts
  tsi.product, tsi.sum, tsi.edge, tsi.parameter, tsi.called,
  tsi.argument, tsi.input, tsi.output

Common semantic relations
  tsi.subtype(Source, Target, Witness)
  tsi.assignable(Source, Target, Witness)
  tsi.conforms(Source, Contract, Witness)
  tsi.equivalent(Left, Right, Witness)

Language-native extensions
  ts.conditional(Result, Check, Extends, Then, Else)
  ts.mapped(Result, Parameter, SourceKeys, ValueOperator)
  ts.readonly(EdgeId)
  ts.optional(EdgeId)

  rust.trait(Contract)
  rust.impl(Type, Contract, ImplSymbol)
  rust.lifetime(ParameterId, LifetimeId)
  rust.ownership(EdgeId, Ownership)

  go.interface(Contract)
  go.type_set(Contract, TypeSetId)
  go.embedding(Owner, EmbeddedType)
```

`Witness` is an ID for the source declaration, compiler proof, or derived rule
that established a semantic relation. This keeps explicit Rust `impl`, Go or
TypeScript structural satisfaction, and DL7 userland conformance distinguishable
while allowing a shared `conforms/3` query.

The intersection across type systems is computed by rules over facts. Each
language retains its own open set of kind and operator relations.
For example, a portable-record query can require `product/1`, closed edge
names, supported primitive widths, and a chosen optionality policy. A Rust
projection can additionally consume ownership and explicit implementation
facts. A TypeScript projection can additionally consume mapped and conditional
operator facts.

## 4. Example extraction

TypeScript source:

```ts
interface User<T> {
  readonly id: T;
  name?: string;
}
```

Conceptual emitted facts:

```text
tsi.type(user_type)
tsi.denotes(user_symbol, user_type)
tsi.origin(user_type, typescript, user_range)
tsi.product(user_type)
ts.interface(user_type)

tsi.parameter(t_parameter, user_type, 0, invariant)

tsi.edge(user_id_edge, user_type, id, t_parameter, 0)
ts.readonly(user_id_edge)

tsi.edge(user_name_edge, user_type, name, text_type, 1)
ts.optional(user_name_edge)
```

For `User<number>`, the checker or comptime closure adds:

```text
tsi.called(user_number_type, user_type, user_number_args)
tsi.argument(user_number_args, 0, number_type)
```

The result node represents one canonical closed type. Rules may project
its substituted edges without erasing the constructor, argument, source, or
TypeScript-specific facts.

## 5. Queries, projections, and losses

DL7 rules consume the protocol as ordinary relations.

```text
serializable_edge(?Edge) <-
    tsi.edge(?Edge, ?Owner, ?Label, ?Target, ?Position),
    serializable_type(?Target).

portable_contract(?Type) <-
    product(?Type),
    every_edge_serializable(?Type),
    every_edge_has_portable_presence(?Type).
```

An emitter declares which fact families it consumes. Projection produces both
an artifact model and loss rows.

```text
projected_type(Target, SourceType, ProjectedType)
projection_change(Target, Subject, Change, Reason, SourceRange)
```

`Change` has values such as `widened`, `erased`, `encoded`, and `rejected`.
Examples of reportable changes include:

- an i64 projected into a TypeScript `number`;
- a nominal type projected as structural JSON Schema;
- TypeScript absence and null projected into one nullable schema state;
- a Rust lifetime omitted by a wire-schema projection;
- a conditional or mapped type widened because a target lacks the operator.

This allows target compatibility to be queried before rendering. Loss policy
can reject, warn, preserve as an extension, or accept a declared widening.

## 6. Connection to the compiler clocks

```text
COMPILE CLOCK

syntax facts + native checker facts
                |
                v
             TSI facts
                |
                v
        DL7 comptime rules
                |
                v
 closed type graph + logical relational IR
                |
                v
 execution IR and target emitters


RUNTIME CLOCK

runtime reflection or extraction event
                |
                v
        versioned fact arrivals
                |
                v
 runtime relational rules and target-specific effects
```

Compile-time type extraction is keyed by source content, compiler version,
language options, dependency identities, and extractor version. Runtime
reflection uses the same fact vocabulary when a host can supply it, with facts
arriving on runtime ticks.

## 7. Open decisions

1. Protocol serialization: DL7 fact stream, a binary columnar form, or both.
2. Stable cross-repository identity: SCIP symbol strings directly, or an
   interned ID plus retained SCIP symbol text.
3. Witness granularity: one proof ID per relation, or a derivation graph with
   premises.
4. Native checker boundary per language: compiler API, language server index,
   SCIP producer extension, or dedicated extractor.
5. Operator normalization: retain native operators only, or derive a shared
   operator family beside them.
6. Version negotiation for core and namespaced fact families.
