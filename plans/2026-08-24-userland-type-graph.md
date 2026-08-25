# User-land type graph integration plan

## Status

- Issue: `@typegraph-integration-plan`
- Epic: `@userland-type-graph`
- Main at reconnaissance: `2c366a932b8bb18f2a417bd140e8ca4ff87855d1`
- Temporal source worktree: `/private/tmp/sprefa-temporal-v2`
- Temporal branch and base: `feature/temporal-relations-v2` at `9e4b468157bb2a189960b8ec69daad10af372862`
- Implementation authorization: approved by Chris on 2026-08-24, excluding
  inferred type reflection for undeclared rule-head relations
- Focused verification: 127 PLUnit tests passed; cross-target typegen golden
  completed with `TYPEGEN GOLDEN: HOLDS`

The implementation target is a small compiler kernel around ordinary DL6
relations. The kernel owns parsing, module/path resolution, compiler-plane
partitioning, safe positive fixpoint evaluation, canonical semantic identity,
bounded construction refreeze, contract validation, diagnostics, and
mechanical target emission. Type operators, projections, constraints, temporal
annotations, and storage-name selection are DL6 rules over compiler rows.

## Held language decisions

1. Brace blocks contribute name prefixes only. Parent references are explicit
   typed columns. No implicit parent member and no child-key shift remain.
2. Dots identify semantic paths in every reference-bearing position.
3. Functional terms in compiler heads are accepted when lowering produces
   explicit relational goals. The compiler evaluator receives function-free
   rules.
4. Calls, parentheses, colons, commas, arrows, and dots are the surface
   composition tools. No whitespace modifier surface is added.
5. Canonical application identity remains
   `application(ConstructorTypeId, OrderedArgumentTypeIds)`.
6. One compiler round observes one immutable semantic graph. Construction
   requests become visible after the next freeze.
7. Compiler rows use deterministic set semantics. Complete declared keys
   enforce functional dependencies.
8. Higher-kinded constructor variables, partial application, unrestricted
   recursive type construction, runtime schema mutation, reducers, and history
   capture are outside this epic.
9. An undeclared rule-head relation remains a runtime IDB identified by
   `Name/Arity`. It receives no `TypeId`, no canonical members, and no `$type`
   rows. Authored and compiler-generated declarations enter the type graph.
10. Any implementation choice outside these decisions stops and asks Chris.

The notation `type.*` below is the ordinary dotted namespace. Existing prose
that says `$type` refers to this compiler-time relation family. This plan adds
no `$` lexer form. Existing unnamespaced ABI relations remain compatibility
aliases until `@dot-brace-nesting` and `@userland-dot-projection` land.

## 1. Type signatures

### 1.1 Compiler value domains

```text
TypeId       = primitive(Name)
             | named(ModuleId, DeclarationKind, Name)
             | application(ConstructorTypeId, OrderedTypeIds)
             | parameter(OwnerTypeId, Position, Name)
             | anonymous(OwnerTypeId, SourcePath, Shape)

MemberId     = member(OwnerTypeId, Position, Name)
ArgumentId   = argument(ApplicationTypeId, Position)
TypeNodeId   = TypeId | MemberId | ArgumentId
             | derivation(MaterializedTypeId, SourceNodeId)
             | annotation_site(MemberId, SourcePath, Ordinal)
RelationRef  = Name / Arity
Backend      = sqlite | typescript | rust | program_json | json_schema
```

`TypeId`, `MemberId`, `ArgumentId`, and `TypeNodeId` are structural compiler
values. Only `TypeId` values may be member target types. Text hashes and dense
catalog integers are artifact encodings and never enter type equality.

### 1.2 Generic compiler-plane allowance

```prolog
partition_compiler_relations(
  +Declarations,
  -CompilerRelations,
  -RuntimeDeclarations
).

compiler_relation_columns(
  +RelationRef,
  +OrderedColumnDomains
).

compiler_only_enum_domains(
  +Declarations,
  +CompilerRelationRefs,
  -CompilerOnlyEnumNames
).
```

Selected rule:

```text
A declared relation is compiler-plane when at least one column has domain
`type`. Its other columns may use compile-time-known primitive or declared enum
domains. All facts and derived rows must still be ground after joins.
```

This permits ordinary authored relations such as:

```dl6
rel optional_member(
  Owner: type,
  Position: int,
  Name: text,
  Target: type
).
```

Compiler-only enum domains erase with the compiler relation unless a runtime
relation also reaches the enum. Shared enum domains remain available to both
planes.

### 1.3 Canonical semantic input ABI

The host projects the frozen specialized rows into compiler-source relations.
These projections are read-only and have compilation lifetime.

```text
type.declaration(
  TypeId,
  Scope,
  Name,
  DeclarationKind,
  Phase
)

type.path(
  TypeId,
  OrderedPathSegments
)

type.member(
  MemberId,
  OwnerTypeId,
  logical,
  Position,
  Name,
  TargetTypeId
)

type.member_role(
  MemberId,
  Role,
  RoleValue
)

type.application(
  ApplicationTypeId,
  ConstructorTypeId
)

type.argument(
  ArgumentId,
  ApplicationTypeId,
  Position,
  TargetTypeId
)

type.derived_from(
  MaterializedTypeId,
  SourceNodeId
)

type.annotation(
  MemberId,
  SourcePath,
  Ordinal,
  AnnotatorRelationId,
  InputTypeId,
  ApplicationValue,
  OutputTypeId
)
```

The natural annotation key is `(MemberId, SourcePath, Ordinal)`. The field
order follows the existing `annotation_evidence(Member, Site, Ordinal, Input,
Application, Output, AnnotationRow)` carrier after replacing `AnnotationRow`
with its canonical annotator relation ID.

The new relations project directly from canonical rows. Compatibility aliases
retain current `type_decl/4`, `type_member/5`, `type_member_role/3`,
`type_application/2`, `type_argument/4`, `type_application_site/4`,
`type_field/5`, and retained annotation evidence. `type.declaration/5` cannot
be an alias of `type_decl/4` because the legacy view omits scope. Adapters
unwrap `type_ref(...)`, `type_atom(...)`, and materialized-owner transport
before exposing `TargetTypeId`.

### 1.4 Generic node and edge query view

```text
type.node(
  NodeId,
  NodeKind,
  Label
)

type.edge(
  EdgeId,
  OwnerNodeId,
  EdgeRole,
  Position,
  Label,
  TargetNodeId
)

type.project(
  OwnerTypeId,
  MemberName,
  TargetTypeId
)
```

Node kinds initially cover declaration, primitive, application, parameter,
member, argument, anonymous, derivation, and annotation-site nodes. Edge roles
initially cover member, argument, constructor, materializes, nested, variant,
path, and annotation. Structural edge IDs include every canonical identity
component. A derivation edge uses `derivation(Materialized, Source)` rather
than either endpoint alone.

The specialized canonical rows remain semantic authority. `type.node` and
`type.edge` are ephemeral query projections. They are not another stored type
graph.

Functional dependencies:

```text
NodeId -> NodeKind, Label
EdgeId -> OwnerNodeId, EdgeRole, Position, Label, TargetNodeId
(OwnerNodeId, EdgeRole, Position, Label) -> EdgeId, TargetNodeId
(OwnerTypeId, MemberName) -> TargetTypeId
```

Exact duplicate projected edges deduplicate. Distinct targets under the same
owner/role/position/label produce a named edge conflict. `type.project` owns
the stricter member-name lookup and its ambiguity diagnostic.

### 1.5 Type construction ABI

The generic construction boundary already exists. Its semantic contract stays
stable while compatibility names later receive dotted aliases.

```text
type.apply(
  ConstructorTypeId,
  OrderedArgumentTypeIds,
  ApplicationTypeId
)

type.requested(
  ApplicationTypeId,
  ConstructorTypeId,
  OrderedArgumentTypeIds
)

type.derived_relation(
  ApplicationTypeId,
  ConstructorTypeId,
  OrderedArgumentTypeIds,
  MemberCount
)

type.derived_member(
  ApplicationTypeId,
  Position,
  Name,
  MemberTypeId
)

type.derived_member_role(
  ApplicationTypeId,
  Position,
  Role,
  RoleValue
)
```

Compatibility names are `type_apply/3`, `type_requested/3`,
`derived_relation_request/4`, `derived_member_request/4`, and
`derived_member_role_request/4`.

Functional dependencies:

```text
(ConstructorTypeId, OrderedArgumentTypeIds) -> ApplicationTypeId
ApplicationTypeId -> ConstructorTypeId, OrderedArgumentTypeIds
(ApplicationTypeId, Position) -> Name, MemberTypeId
(ApplicationTypeId, Name) -> Position, MemberTypeId
(ApplicationTypeId, Position, Role) -> RoleValue
```

`MemberCount` applies to a complete relation-like derived application. Plain
applications have no member-count dependency.

### 1.6 Logical and physical member planes

Logical member rows retain authored and generated semantic types. Physical
rows add target facts keyed by canonical IDs and copy no member names,
positions, owners, or logical target types.

```text
storage.relation(
  OwnerTypeId,
  Backend,
  PhysicalRelationKind
)

storage.column(
  MemberId,
  Backend,
  PhysicalType
)

storage.key(
  OwnerTypeId,
  Backend,
  MemberId,
  Ordinal
)
```

`storage.key` is a compatibility view derived from ordered
`storage.constraint_member` rows for the primary group. It is not an
independent source of key authority.

`PhysicalType` is one of the executable target representations already
distinguished by lowering: integer, real, text, blob, JSON, typed JSON list,
relation/reference ID, interned ID, or target-specific wrappers retained by
the selected backend.

Storage projection is a pure read of a completed semantic graph and the
executable relation plan:

```prolog
project_storage_rows(
  +CompletedSemanticTypeRows,
  +RelationPlans,
  +Backend,
  -StorageRows
).
```

It emits storage rows only. It never inserts declarations or members into the
semantic graph. Every referenced `OwnerTypeId` and `MemberId` must already
exist in `CompletedSemanticTypeRows`. The projection skips an undeclared IDB
whose `rel/5` plan has no canonical owner. Existing lowerers and emitters keep
using `rel/5` during this migration.

The six-column convenience query is derived, not stored:

```text
type.member(
  MemberId,
  OwnerTypeId,
  storage(Backend),
  Position,
  Name,
  PhysicalType
)
```

It joins the logical member row to `storage.column` by `MemberId`.

### 1.7 Constraint graph

Constraint identity uses a natural relational key. No opaque generated
constraint registry is required.

```text
storage.constraint(
  OwnerTypeId,
  Backend,
  ConstraintKind,
  Group
)

storage.constraint_member(
  OwnerTypeId,
  Backend,
  ConstraintKind,
  Group,
  Ordinal,
  MemberId
)

storage.foreign_target(
  OwnerTypeId,
  Backend,
  Group,
  Ordinal,
  TargetOwnerTypeId,
  TargetMemberId
)
```

Constraint kinds initially cover primary, unique, index, and foreign_key.
Existing `key(T)` evidence derives one `primary/default` group. All members in
that group form one ordered composite key. Alternate groups use distinct
authored group values.

Functional dependencies:

```text
(Owner, Backend, Kind, Group, Ordinal) -> MemberId
(Owner, Backend, Kind, Group, MemberId) -> Ordinal
(Owner, Backend) -> at most one primary group
(Owner, Backend, foreign_key, Group, Ordinal)
  -> TargetOwner, TargetMember
```

### 1.8 Storage names

```text
storage.name_override(
  TargetNodeId,
  Backend,
  ObjectRole,
  PhysicalName
)
```

Object roles distinguish table, index(Group), delta(Sign), frontier, trigger,
dictionary, and refcount names. The target emitter quotes every identifier and
escapes embedded quotes. Source identifiers containing punctuation remain
outside this card; a compiler rule may still select a physical text name that
contains dots, hyphens, spaces, or approved Unicode.

Functional dependencies:

```text
(TargetNodeId, Backend, ObjectRole) -> at most one override PhysicalName
(Backend, SQLiteCaseFold(PhysicalName)) -> TargetNodeId, ObjectRole
```

The host derives a default only when no override exists, then emits one
selected `storage.name` row. SQLite case-fold collision checking applies only
to the `sqlite` backend.

### 1.9 Temporal annotation and output rows

```dl6
rel temporal.Retention(all(); count(value: int)).

rel temporal.log(Target: type).
rel temporal.keep(Target: type, Policy: temporal.Retention).
rel temporal.history(Target: type).
```

These ordinary compiler relations derive target metadata:

```text
storage.relation_kind(TargetTypeId, log)
storage.relation_keep(TargetTypeId, all)
storage.relation_keep(TargetTypeId, count(Limit))
```

Functional dependencies are `TargetTypeId -> RelationKind` and
`TargetTypeId -> RetentionPolicy`. Surface `all()` elaborates to internal
`all`; `count(N)` remains one ground policy value.

The target rows are a mechanical compiler-output ABI. Temporal policy is in
DL6. The Prolog evaluator contains no `relation_kind_request` or
`relation_keep_request` feature semantics.

This epic proves call-form parity with the old event-log behavior. Runtime
history capture, per-identity version allocation, timestamps, reducers, and
state-diff retention remain outside this epic.

### 1.10 User-land type operators

```text
type.serializable(TypeId)
type.extends(ChildTypeId, ParentTypeId)
type.impl(SubjectTypeId, InterfaceTypeId)
type.concat(LeftTypeId, RightTypeId, OutputTypeId)
type.partial(SourceTypeId, OutputTypeId)
```

`partial` and `concat` may emit complete type construction rows. `extends`,
`impl`, and `serializable` are graph relations. None requires constructor-valued
variables or a kind-arrow system.

Functional dependencies are `SourceTypeId -> OutputTypeId` for `partial` and
`(LeftTypeId, RightTypeId) -> OutputTypeId` for `concat`.

## 2. Pseudocode bodies

### 2.1 Compiler-plane partition

```prolog
classify_relation(Decls, Ref, compiler) :-
    ordered_column_domains(Decls, Ref, Domains),
    member(type, Domains).

classify_relation(_, _, runtime).

partition_program(Decls, Rules, Compiler, Runtime) :-
    classify_declared_relations,
    close compiler-only enum-domain reachability,
    move compiler relation declarations and rules to Compiler,
    retain enum declarations reached by any runtime relation,
    reject rule edges crossing compiler and runtime planes,
    erase compiler-only declarations after final closure.
```

The type column is the phase witness. This preserves one surface grammar and
avoids a `comptime` declaration keyword. Relations with no type column remain
runtime in this epic.

### 2.2 Immutable compiler round and bounded refreeze

```text
round N:
  finish syntax-directed expansion
  freeze declared and compiler-generated canonical semantic rows N
  project compiler source rows N
  evaluate authored positive rules to set fixpoint
  validate declared functional dependencies
  collect complete type-construction requests
  collect target metadata rows, but do not emit them yet

if no new construction request and canonical rows equal round N-1:
  keep only the final closure and final target metadata
  build an ephemeral effective declaration view
  run semantic and temporal validation
  project storage rows for canonical declared/generated relations
  retain runtime rel/5 plans for declared and undeclared IDBs
  erase compiler declarations, rules, requests, evidence, and projections
else:
  materialize the deduplicated construction frontier
  discard target rows from round N
  start round N+1

refuse:
  a constructor-producing recursive SCC
  a non-ground construction request
  an unknown constructor
  an arity mismatch
  a conflicting complete shape
  exhaustion of the existing 16-round cap
```

Target metadata is recomputed from the final immutable snapshot. Rows from an
earlier round do not accumulate into the final target plan. Runtime inference
for an undeclared IDB remains outside the semantic graph and cannot seed
compiler type rules.

### 2.3 Node and edge projection

```dl6
type.node(Type, declaration, Name) <-
  type.declaration(Type, _, Name, _, _).

type.node(Application, application, '') <-
  type.application(Application, _).

type.node(Member, member, Name) <-
  type.member(Member, _, logical, _, Name, _).

type.edge(Member, Owner, member, Position, Name, Target) <-
  type.member(Member, Owner, logical, Position, Name, Target).

type.edge(Argument, Application, argument, Position, '', Target) <-
  type.argument(Argument, Application, Position, Target).

type.edge(constructor_edge(Application), Application, constructor, 0, '', Constructor) <-
  type.application(Application, Constructor).

type.edge(derivation(Materialized, Source), Materialized, materializes, 0, '', Source) <-
  type.derived_from(Materialized, Source).

type.project(Owner, Name, Target) <-
  type.member(_, Owner, logical, _, Name, Target).
```

The implementation may seed `type.node` and `type.edge` directly from the
specialized canonical rows when doing so avoids circular bootstrapping. The
mapping above remains the test oracle and no projected row persists.

### 2.4 Structural term lowering

Direction is determined by compiler-rule position.

```text
head type term:
  lower a declared constructor application inside-out to type.apply goals
  replace the term with the resulting ApplicationTypeId variable

body type term:
  lower a semantic structural pattern to type.node/type.edge joins
  do not request construction merely by matching

bare fact with variables:
  reject under existing range-restriction safety
```

Examples:

```dl6
output(Source, list(Source)) <- input(Source).
```

lowers to:

```dl6
output(Source, Output) <-
  input(Source),
  type.apply(list, [Source], Output).
```

```dl6
serializable(primitive(Name)) <- scalar_name(Name).
```

lowers to:

```dl6
serializable(Type) <-
  scalar_name(Name),
  type.node(Type, primitive, Name).
```

`serializable(primitive(Name)).` with an unbound `Name` remains an unsafe fact.

### 2.5 Member planes

```dl6
type.member(Member, Owner, storage(Backend), Position, Name, StorageType) <-
  type.member(Member, Owner, logical, Position, Name, _),
  storage.column(Member, Backend, StorageType).
```

Logical operators query `logical`. SQLite and runtime emitters query
`storage(sqlite)`. TypeScript, Rust, ProgramJson, and JSON Schema type artifacts
query logical members unless an executable storage plan explicitly requires a
physical representation.

### 2.6 Constraints

```dl6
storage.constraint(Owner, sqlite, primary, default) <-
  type.member(Member, Owner, logical, _, _, _),
  type.member_role(Member, key, '').

storage.constraint_member(
  Owner,
  sqlite,
  primary,
  default,
  Ordinal,
  Member
) <-
  type.member(Member, Owner, logical, Position, _, _),
  type.member_role(Member, key, ''),
  key_ordinal(Owner, Position, Ordinal).
```

The first bridge groups all existing key-role members into one ordered primary
constraint. Later annotation libraries derive distinct unique, index, and
foreign-key groups without changing the host evaluator.

`key_ordinal` reads the authored key-list ordinal. It cannot substitute column
position: `keyed([2,1])` must remain ordered as members 2 then 1.

The SQLite emitter groups complete rows, validates the functional
dependencies, quotes names, and renders DDL. It does not inspect `key(...)`,
annotation relation names, or temporal relations.

### 2.7 Temporal call-form parity

```dl6
storage.relation_kind(Target, log) <- temporal.log(Target).
storage.relation_keep(Target, Policy) <- temporal.keep(Target, Policy).

storage.relation_kind(Target, log) <- temporal.history(Target).
storage.relation_keep(Target, all) <- temporal.history(Target).
```

Only the final target rows reach runtime planning. The old suffix parser is
removed after runtime declarations, SQL plans, SQLite retention, and reference
timelines are equal for call and suffix fixtures.

### 2.8 Storage names

```dl6
storage.name_override(Type, sqlite, table, Requested) <-
  storage.sqlite_name(Type, Requested).
```

The host supplies a default for a target with no override, validates one
selected name per target and object role, and emits `storage.name`. SQLite
rendering always uses double-quoted identifiers and doubles embedded quotes.
Companion names derive from `(Target, ObjectRole)` before collision checking.

### 2.9 Type operators

```dl6
type.serializable(Type) <- type.node(Type, primitive, _).

type.serializable(Type) <-
  type.declaration(Type, _, _, relation, _),
  type.member_count(Type, Count),
  type.serializable_member_count(Type, Count).

type.extends(Child, Ancestor) <- type.extends(Child, Parent),
  type.extends(Parent, Ancestor).

type.impl(Type, Interface) <- type.direct_impl(Type, Interface).
type.impl(Type, Interface) <- type.extends(Type, Parent),
  type.impl(Parent, Interface).
```

`partial` reuses the landed complete derived-relation request contract.
`concat` emits one complete ordered member set after detecting incompatible
duplicate names. Recursive serializability traverses a finite frozen graph;
construction recursion remains governed by the existing refreeze policy.

The transitive `extends` and `impl` examples fit the current positive
fixpoint. `concat` position shifting and universal serializability require
bounded integer/order and aggregate facilities that the current compiler
evaluator does not expose. Their inclusion is a user gate in section 5.

## 3. Instance timelines and lifetimes

| Row family | Created | Read | Lifetime | Erasure |
|---|---|---|---|---|
| Parse carriers `type_decl`, `col_type`, `keyed` | parser and syntax expansion | generic, anonymous, enum, annotation, option, key phases | mutable expansion | removed from semantic authority after final freeze; compatibility consumers retire incrementally |
| Canonical specialized rows | `freeze_type_rows/2` | compiler sources, refreeze comparison, typegen, storage projection | one immutable snapshot per round; final set survives compilation | serialized or erased at artifact boundary |
| `type.node` and `type.edge` | source adapter or DL6 projection from one snapshot | user-land compiler rules | one compiler round | always before runtime planning |
| Compiler closure | positive safe evaluator | type requests, target rows, diagnostics | one compiler round; final closure may feed catalog metadata | no runtime table, boot row, arrival, DD relation, or host payload |
| Construction frontier | final closure of round N | generic materializer | between N and N+1 | after request materialization and deduplication |
| Logical members | canonical freeze | type operators, schema reflection, type artifacts | final semantic graph | retained only as compiled metadata where requested |
| Physical storage rows | final target projection | SQLite and executable target planning | final target plan | serialized into generated artifacts |
| Constraint rows | final compiler closure and storage projection | target DDL emitters | final target plan | serialized into DDL/catalog artifacts |
| Temporal annotation facts | authored DL6/compiler closure | temporal library rules | compiler invocation | erased after target metadata extraction |
| Storage-name rows | final compiler closure | backend emitter | final target plan | serialized spelling only |

## 4. Storage, reads, writes, and uniqueness

### 4.1 Read/write sequence

```text
1. Parse declarations, rules, imports, and dotted paths.
2. Resolve modules and declared relation paths.
3. Expand generic, enum, anonymous, option, annotation, match, dot, and rule carriers.
4. Freeze canonical rows for authored and compiler-generated declarations.
5. Project immutable compiler input rows from that graph.
6. Evaluate user-land compiler rules to set fixpoint.
7. Validate functional dependencies and complete type requests.
8. If construction frontier grows, materialize carriers and return to step 3.
9. On stability, derive final constraint, temporal, and storage-name target rows.
10. Build an ephemeral effective declaration view and run semantic/temporal checks.
11. Build runtime `rel/5` plans; undeclared IDBs remain runtime-only.
12. Project storage rows for plans with canonical owners and members.
13. Validate physical references and backend name uniqueness.
14. Emit SQLite, TypeScript, Rust, ProgramJson, JSON Schema, and catalog artifacts.
15. Erase compiler sources, closure, requests, and proof rows.
```

### 4.2 Uniqueness matrix

| Subject | Key | Determined values |
|---|---|---|
| declaration | `TypeId` | scope, exact name, kind, phase |
| logical member | `MemberId` | owner, position, exact name, target |
| logical member position | owner, position | member ID, name, target |
| application | constructor plus ordered arguments | application TypeId |
| application argument | application, position | argument ID, target |
| annotation site | member, source path, ordinal | annotator, input, application value, output |
| generic edge | edge ID | owner, role, position, label, target |
| projection | owner, exact name | target |
| storage relation | owner, backend | physical kind |
| storage column | member, backend | physical type |
| constraint member | owner, backend, kind, group, ordinal | member ID |
| storage-name override | target, backend, object role | at most one physical name |
| SQLite name | backend plus case-folded physical name | target and object role |

No uniqueness contract depends on source declaration insertion order, Prolog
variable identity, generated display names, or dense catalog coordinates.

## 5. Architectural forks

### 5.1 Generic graph as authority versus adapter

| Choice | Evidence | Result |
|---|---|---|
| Replace specialized canonical rows with only node/edge rows | A single graph shape is compact for traversal, but declaration, member, application, argument, and role families have different completeness and functional dependencies. | Rejected for this epic. |
| Keep specialized canonical rows and expose node/edge adapters | Matches `semantic_type_rows`, preserves landed IDs and closure checks, and gives user-land graph traversal without a second stored authority. | Selected. |

### 5.2 Structural Prolog-term unification versus relational shape rows

| Choice | Evidence | Result |
|---|---|---|
| Let authored compiler rules depend directly on Prolog compound unification | Couples language semantics to host term layout and makes construction versus matching context implicit inside the evaluator. | Rejected for authored lowering. |
| Lower construction to `type.apply` and matching to node/edge joins | Preserves function-free evaluator IR, current construction demand, and explicit Datalog safety. | Selected. |

### 5.3 Stored member planes versus joined physical facts

| Choice | Evidence | Result |
|---|---|---|
| Store complete logical and storage member rows | Copies owner, name, and position and recreates two semantic authorities. | Rejected. |
| Store logical members once; add `storage.column(MemberId, Backend, Type)` | Matches the canonical storage projection contract and permits an explicit derived plane query. | Selected. |

### 5.4 Constraint ID versus natural relation key

| Choice | Evidence | Result |
|---|---|---|
| Mint an opaque constraint ID | Requires another interning rule and mapping for user-authored groups. | Rejected for storage constraints. |
| Use `(Owner, Backend, Kind, Group)` | Directly represents composite and alternate constraints and has explicit FDs. | Selected. |

Existing generic-bound `constraint(...)` semantic rows retain their current
meaning. Storage constraints use the `storage.constraint` namespace and do not
reuse the generic-bound row constructor.

### 5.5 Temporal feature builtins versus target rows

| Choice | Evidence | Result |
|---|---|---|
| Interpret `relation_kind_request` and `relation_keep_request` in Prolog | The temporal worktree proves parity but embeds annotation names and conflict logic in the evaluator. | Replaced after salvage. |
| Derive normalized target rows in DL6 and validate them mechanically | Keeps annotation policy in user-land while preserving one target emission seam. | Selected. |

### 5.6 Mutate one frozen graph versus bounded refreeze

| Choice | Evidence | Result |
|---|---|---|
| Add canonical members during one compiler closure | Later joins can observe partial construction and rule order changes meaning. | Rejected. |
| Freeze, evaluate, collect, materialize, refreeze | Already landed for `type.apply` and `Partial(User)` with stable row comparison and a 16-round cap. | Selected. |

### 5.7 Declared-only reflection versus program-level semantic completion

| Choice | Evidence | Result |
|---|---|---|
| Reflect only declared and compiler-generated schemas | Matches explicit schema identity. An undeclared IDB still receives `rel/5` runtime storage and remains absent from compiler-time type queries. | Selected. |
| Infer runtime shapes and manufacture canonical rows | Would make rule-head-only predicates queryable as types and requires a program-level analysis/refreeze loop. | Deferred to `@inferred-idb-type-reflection`. |

### 5.8 Canonical path rows versus path adapter input

| Choice | Evidence | Result |
|---|---|---|
| Add canonical `path(TypeId, Segments)` rows | Makes namespace and nesting edges part of the frozen graph and allows node/edge projections to use one authority. Brace blocks remain name prefixes only. | Selected. |
| Feed `rel_path_decl` separately into the node/edge adapter | Avoids widening canonical rows, but path queries depend on a second mutable expansion carrier. | Available alternate. |

### 5.9 Compiler scalar/order and aggregate facilities

| Choice | Evidence | Result |
|---|---|---|
| Add concat-specific compiler builtins | Supplies the immediate arithmetic but embeds one type operator in the host evaluator. | Rejected. |
| Reuse ordinary DL6 scalar, order, and aggregate semantics in the compiler plane | Enables member-position shifting for `concat` and finite all-members checks for `serializable` through general language facilities. | Selected as separate `@compiler-plane-expression-parity` work before full type operators. |

## 6. Existing issue ownership

| Issue | Current fact | Ownership in this epic |
|---|---|---|
| `@compiler-derived-relation-construction` | Done in `b5c5effa0`; owns functional head construction and complete derived-relation requests. | Reuse, no duplicate implementation. |
| `@type-apply-refreeze` | Done; owns structural application identity, frontier, refreeze, and termination diagnostics. | Reuse, no duplicate implementation. |
| `@applicative-type-annotations` | Done; owns nested type applications and annotation evidence. | Reuse evidence, normalize query view. |
| `@canonical-type-reflection` | Verified and closed on main at `bef4acbb3`; 101 focused compiler/type tests, 26 brace/path tests, and the cross-target typegen golden passed. | Reuse; no duplicate implementation. |
| `@typed-annotation-corrections` | Verified and closed on main at `2e9c3c792`; 101 focused compiler/type tests, 26 brace/path tests, and the cross-target typegen golden passed. | Reuse; no duplicate implementation. |
| `@inferred-idb-type-reflection` | Deferred. Undeclared IDBs remain runtime-only in the active design. | Backlog only; blocks no active card. |
| `@compiler-plane-expression-parity` | Required for general `concat` ordinal arithmetic and universal member checks. | Separate Large compiler card before full `@userland-type-operators`. |
| `@canonical-storage-projection` | Dirty implementation worktree is 429 commits behind main. | Preserve row contract; reimplement against current `program_plan/3` and emitters. |
| `@dot-brace-nesting` | Name-prefix ruling is recorded; current parent-capture removal waits for temporal salvage. | Own parser/path and implicit-parent removal only. |
| `@comptime-type-model` | Open decision epic. | Context only; this plan implements only previously held bounded first-order rules. |
| `@type-plane-design` | Open user-gated design epic. | Context only; unresolved higher-kinded and mixed-stage items remain outside. |
| `@remove-rel-is` | Done; relation conformance suffix removed. | No work. Expression-registry `is/2` remains unrelated. |

## 7. Temporal-v2 dirty worktree partition

Reconnaissance counted 10 tracked modified files, 5 untracked files, 32 total
hunks, `+712/-23` including untracked file lines. The worktree index is clean.

### 7.1 Per-file ownership

| File | Ownership |
|---|---|
| `v6/prolog/0_compiler_relations.pl` | Split: keep mixed compiler domains and compiler-only enum reachability; replace temporal request builtins. |
| `v6/prolog/0_generic_expand.pl` | Keep path-resolution imports provisionally; replace PL projection exports/includes. |
| `v6/prolog/0_generic_expand/0_expand.pl` | Keep pre-refreeze dotted reference resolution; coordinate with dot-brace. |
| `v6/prolog/0_generic_expand/2_compiler_plane.pl` | Keep enum argument elaboration; replace temporal request interpretation. |
| `v6/prolog/0_generic_expand/5_type_freeze.pl` | Replace projection-specific validation with user-land projection output validation. |
| `v6/prolog/0_unsupported_messages.pl` | Move projection diagnostics to `@userland-dot-projection`. |
| `v6/prolog/compile/test/4_braced_nested_relations.test.pl` | Move projection receipts to dot-projection and anonymous-sum cards. |
| `v6/prolog/compile/test/compiler_relations.test.pl` | Keep mixed scalar-domain receipt. |
| `v6/prolog/compile/test/plunit_tests.pl` | Move temporal test registration with temporal card. |
| `v6/prolog/compile/test/typegen_golden.sh` | Move temporal fixture registration with temporal card. |
| `v6/dl/fixtures/1_temporal-relations-v2.dl6` | Move call-form parity fixture to temporal card. |
| `v6/dl/std/0_temporal.dl6` | Split: retain annotation declarations; replace request heads with normalized storage rows. |
| `v6/dl/std/README.md` | Move and rewrite around selected target-row contract. |
| `v6/prolog/0_generic_expand/5a_type_projection.pl` | Replace with user-land DL6 projection rules; retain as behavioral oracle until parity. |
| `v6/prolog/compile/test/compiler_relations/0_temporal_relation_annotations.test.pl` | Split generic enum-domain tests from temporal parity; replace builtin-row assertions. |

### 7.2 Hunk ledger

| IDs | Decision | Content |
|---|---|---|
| C1-C6 | KEEP | Mixed compiler scalar/enum domains, compiler-only enum reachability and erasure, partition signature changes. |
| C7-C8 | REPLACE | `relation_kind_request/2` and `relation_keep_request/2` builtin/request registrations. |
| G1,G3 | REPLACE | PL type-projection exports and include. |
| G2,E1 | KEEP provisionally | `declared_path/3` and `resolve_relation_paths/3` before refreeze; dot-brace preserves these interfaces. |
| P1-P5 | REPLACE | Temporal demand, modifier collection, runtime declaration injection, conflict logic, and source signatures. |
| P6-P7 | KEEP | Declared enum-domain values and recursive enum field elaboration. |
| F1 | REPLACE | Freeze-time PL projection target validation. |
| U1-U2 | MOVE | Ambiguous projection diagnostics and labels. |
| B1-B2 | MOVE | Projection and anonymous-sum projection receipts. |
| T1 | KEEP | Mixed scalar-domain compiler relation test. |
| L1,K1,N1,N3 | MOVE | Temporal module loader, golden registration, fixture, and documentation. |
| N2 | SPLIT | Keep temporal annotation declarations; replace request rules with storage rows. |
| N4 | REPLACE | Entire `5a_type_projection.pl` experiment with DL6 rules. |
| N5 | SPLIT | Keep generic enum erasure receipts; move temporal parity; replace builtin closure assertions. |

### 7.3 Preservation commits

The dirty worktree is first made reviewable without merging feature semantics:

1. `compiler: accept mixed compile-time value domains`
   - C1-C6, P6-P7, T1, and extracted generic enum tests.
2. `compiler: preserve dotted type-projection oracle`
   - G1-G3, E1, F1, U1-U2, B1-B2, and N4.
3. `temporal: preserve call-form parity oracle`
   - C7-C8, P1-P5, L1, K1, N1-N3, and temporal portions of N5.

Only commit 1 is eligible for immediate integration after plan approval and
focused review. Commits 2 and 3 remain source oracles for their owning cards;
their PL feature semantics do not merge as final architecture.

## 8. Implementation and merge sequence

```text
0. Plan approved on 2026-08-24.
   keep undeclared IDBs outside the type graph
   record @inferred-idb-type-reflection as backlog only
   create @compiler-plane-expression-parity [large]

1. @temporal-v2-salvage [large]
   create the three preservation commits above
   reapply the generic compiler-domain commit to current main
   leave the temporal and projection oracle commits isolated
   clean the source worktree

2. @dot-brace-nesting [medium]
   remove implicit parent capture and key shifting
   preserve declared_path/3 and resolve_relation_paths/3
   establish final name-prefix and rule-provenance behavior

3. Parallel after dot-brace
   @canonical-storage-projection [existing direct blocker]
     reimplement pure projection on current main
     project only plans with canonical declared/generated owners
     retain rel/5 as the executable compatibility authority
   @typegraph-node-edge-view [medium]
     land mixed-domain substrate if not already landed
     add normalized canonical source adapters
     add node/edge/path projections and erasure tests

4. @compiler-plane-expression-parity [large]
   reuse ordinary scalar, order, and aggregate semantics in compiler rules
   add no concat-specific host builtin

5. Parallel after node/edge prerequisites
   @typegraph-member-planes [medium]
   @type-pattern-lowering [large]

6. Parallel feature libraries
   member planes + dot-brace -> @userland-dot-projection [medium]
   member planes + typed annotations -> @userland-constraint-graph [medium]
   node/edge + member planes + patterns + expression parity
     -> @userland-type-operators [medium]
   integration plan + storage projection -> @quoted-sqlite-storage-names [medium]
   temporal salvage + constraints + typed annotations
     -> @userland-temporal-annotations [medium]

7. Leaf implementations
   dot projection -> @anonymous-sum-dot-projection [medium]
   constraint graph -> @sqlite-constraint-emitter [small]
   temporal annotations -> @remove-temporal-suffix [small]

8. @retire-type-specialcases [medium]
   remove only predicates whose DL6 replacement has parity receipts
   record before/after call-site counts for every deletion

9. @userland-typegraph-golden [small]
   run the complete cross-target fixture and compiler-row erasure gate
```

Cards sharing `generic-type-core`, `storage-lowering`, `parser-paths`, or
`type-emitters` collision tokens do not edit concurrently even when their
logical dependencies are closed.

## 9. Verification matrix

### 9.1 Reconnaissance receipts

```text
rg -c '^compiler_builtin_ref\(' v6/prolog/0_compiler_relations.pl
  13

rg -c '^compiler_type_source_signature\(' \
  v6/prolog/0_generic_expand/2_compiler_plane.pl
  13

rg -l 'semantic_type_rows\(' v6/prolog --glob '*.pl' | wc -l
  18 files

rg -l '\b(type_decl|col_type|keyed)\(' v6/prolog --glob '*.pl' | wc -l
  110 files

wc -l v6/prolog/0_compiler_relations.pl v6/prolog/0_generic_expand/*.pl
  397 compiler evaluator lines
  4,076 numbered generic expansion lines

direct relplan helper reads by runtime target
  lower.pl      79
  emit_ts.pl    23
  emit_rust.pl  12

storage_type_rows consumers
  0
```

The 13 current compiler builtins pair with 13 source signatures. Retirement
work records the count after every replacement rather than deleting the list
in one change.

Verification run on main `2c366a932`:

```text
compiler_relations + annotation_surface + type_relation_ir
  101/101 passed

braced_nested_relations
  26/26 passed

typegen_golden.sh
  TYPEGEN GOLDEN: HOLDS
```

### 9.2 Focused gates by card

| Card | Focused gates |
|---|---|
| temporal salvage | compiler relation partition, enum-domain erasure, path resolution, diff check |
| node/edge view | canonical reflection fixtures, imported/generic/anonymous rows, compiler erasure |
| compiler expression parity | scalar arithmetic, ordering, aggregates, compiler/runtime expression parity, determinism, erasure |
| member planes | option, enum, relation-value, dictionary, compiler-generated relation, storage projection; undeclared IDBs produce no rows |
| pattern lowering | parser, compiler safety, nested construction, structural match, recursive refusal |
| dot projection | deep dotted reference matrix in every reference-bearing position, ambiguity diagnostics |
| constraints | key wrapper, composite key, alternate unique/index, foreign-key shape, conflicts |
| temporal annotations | call/suffix declaration equality, SQL equality, SQLite retention, reference timeline |
| quoted names | embedded quotes, dot, hyphen, space, Unicode, ASCII case-fold and companion collision |
| type operators | Partial and transitive extends/impl; add concat and universal serializability only if section 5.11 expands scope |
| closeout | full PLUnit, typegen golden, executable SQLite, TypeScript, Rust, ProgramJson, JSON Schema |

### 9.3 Global invariants

- No compiler input, closure, request, proof, enum helper, or target-selection
  relation creates a runtime SQLite table or boot row.
- Programs that do not query the new views retain byte-identical generated
  artifacts until their target emitter card intentionally changes output.
- One compiler round never observes a partially generated type.
- Every physical member and constraint references an existing canonical ID.
- Every accepted SQLite identifier is emitted through one quoting function.
- Full-suite counts are recorded from the run that closes each card.

## 10. Deferred items

- Constructor variables and kind signatures.
- Partial application and higher-kinded type parameters.
- General chase-termination proofs beyond the current SCC refusal and round cap.
- Runtime history capture, version allocation, timestamps, replay receipts,
  reducers, snapshots, state diffs, and retention compaction.
- A general compiler/runtime mixed-stage parameter system for relations with no
  `type` column.
- Canonical type reflection for undeclared rule-head IDBs. These retain
  runtime `Name/Arity` identity and inferred physical columns only.
- Authored source identifiers containing punctuation.
- Removal of the unrelated expression-registry `is/2` operator.

Any implementation need that requires one of these items stops the active card
and asks Chris before mutation.
