# Canonical Type Row Pipeline

## Context

DL6 currently carries one relation schema through overlapping compiler shapes:

- mutable declaration facts such as `type_decl/2`, `col_type/3`, and `keyed/2`;
- normalized `semantic_type_rows/1`, including `declaration`, `member`,
  `parameter`, `application`, `argument`, `constraint`, `implementation`, and
  `derived_from` rows;
- the physical `rel/5` record consumed by SQLite, TypeScript, and Rust
  lowering;
- dense catalog `row/11` artifacts;
- the derived `schema_member/7` query view.

The source contracts are documented in `prolog/0_rel_record.pl` and produced
by `normalized_type_rows/2` in `prolog/0_generic_expand.pl`. The physical plan
is currently assembled in `compile:program_plan/3`. Catalog rows are assembled
by `lower:catalog_decl_rows/6`.

The representations share relation names, member names, positions, and type
information. Their phase-specific additions differ:

| Shape | Additional information |
| --- | --- |
| declaration facts | mutable surface and expansion terms |
| semantic type rows | module-qualified semantic identity and type graph edges |
| `rel/5` | physical table name, storage type, relation kind, key positions |
| catalog `row/11` | artifact-local dense coordinates |
| `schema_member/7` | authored type, resolved value type, derived member roles |

The normalized graph is currently captured before every generated type is
fully available. Concrete generic and anonymous-product members may exist only
in later `type_decl/2` terms. Option lowering may replace an authored wrapper
with a generated relation name before a later reader observes it. Later
consumers consequently consult more than one representation.

The TypeSpec parity probe in `dl/fixtures/0_typespec_basic_probe.dl6` also
needs compiler-time field iteration. The field reflection surface must query
the canonical graph rather than create another field schema.

## Type signatures

The semantic graph remains target-independent:

```text
declaration(TypeId, Scope, Name, Kind, Phase)
parameter(ParameterId, OwnerTypeId, Position, Name)
member(MemberId, OwnerTypeId, Position, Name, FieldTypeRef)
application(ApplicationTypeId, ConstructorTypeId)
argument(ArgumentId, ApplicationTypeId, Position, ArgumentTypeRef)
constraint(ConstraintId, ParameterId, InterfaceTypeId, Patterns?)
implementation(ImplementationId, SubjectTypeId, InterfaceApplication)
derived_from(TypeId, OriginTypeId)
member_role(MemberId, Role)
```

Physical lowering refers to semantic identities and adds physical facts:

```text
storage_relation(TypeId, TableName, RelationKind)
storage_column(MemberId, StorageType)
storage_key(TypeId, MemberId)
```

Ergonomic compiler relations are views:

```text
field(OwnerTypeId, Name, FieldTypeRef, Position) :-
    member(MemberId, OwnerTypeId, Position, Name, FieldTypeRef).

field_role(OwnerTypeId, Name, Role) :-
    member(MemberId, OwnerTypeId, _, Name, _),
    member_role(MemberId, Role).
```

The Prolog implementation signatures are:

```prolog
freeze_type_rows(+ExpandedDecls, -TypeRows).
derive_storage_rows(+TypeRows, +Rules, +World, -StorageRows).
compiler_type_sources(+TypeRows, +ViewName, -Rows).
serialize_catalog(+TypeRows, +StorageRows, -CatalogRows).
```

## Instance timeline

One compilation has these lifetimes:

```text
parse
  mutable declaration carriers exist

resolve and expand
  module, generic, annotation, anonymous, option, enum, and key phases may
  create or rewrite declaration carriers

freeze
  every generated declaration has a semantic identity
  canonical type rows are materialized once
  semantic uniqueness and completeness are checked

compiler queries
  authored compiler relations read canonical rows and derived views
  query results may request additional type construction

bounded refreeze
  newly requested types are minted
  the graph reaches a fixpoint or produces a named cycle/non-convergence refusal

physical lowering
  storage rows reference canonical TypeId and MemberId values
  mutable declaration carriers leave the downstream plan

artifact emission
  TS, Rust, SQLite, JSON Schema, and catalog outputs read canonical semantic
  rows plus target storage rows
```

Compiler views have compilation lifetime only. They never become SQLite
tables, boot facts, arrivals, differential collections, or host payloads.

## Storage and identity

Semantic identities remain the module-qualified constructors in
`prolog/0_type_ids.pl`. No artifact-local numeric catalog identifier may enter
semantic type equality.

Uniqueness contracts:

```text
declaration key = TypeId
member key      = MemberId
member identity = OwnerTypeId + position + exact member name
application key = constructor TypeId + ordered argument TypeIds
storage column  = one row per MemberId and target
catalog id      = dense serialization coordinate, scoped to one artifact
```

`schema_member/7`, `field/4`, and role projections are computed views. They do
not own stored rows or identities.

`rel/5` may remain temporarily as a compatibility projection while emitters
move to storage rows. Its fields must be derived from the frozen graph and may
not become an independent semantic input.

## Read and write sequence

1. Parse source declarations into mutable expansion carriers.
2. Resolve modules and complete all syntax-directed expansion.
3. Mint concrete generic, anonymous, enum, option, and annotation-produced
   declarations.
4. Freeze and validate canonical type rows.
5. Evaluate authored compiler relations over canonical rows and query views.
6. If evaluation requests new types, mint them and repeat steps 4 and 5 until
   the canonical graph is unchanged.
7. Derive physical target rows from the final graph and rule/world analysis.
8. Serialize catalog and target artifacts.
9. Discard expansion carriers and compiler-query closure before runtime plans
   are returned.

The fixpoint compares canonical semantic row sets. Source term order and Prolog
variable identity do not participate.

## Decisions

1. `semantic_type_rows` is the sole semantic authority after the freeze.
2. Field reflection queries existing `member` rows and application edges.
3. Member roles are derived relations keyed by canonical `MemberId`.
4. Physical target facts reference canonical IDs and add only target data.
5. Catalog rows are serialization output.
6. Surface declaration carriers remain during syntax-directed expansion, then
   leave the downstream plan.
7. Freeze occurs after all existing minting phases and before user compiler
   queries that inspect complete types.
8. Type-producing compiler queries use a bounded refreeze fixpoint.
9. TypeSpec `...Base`, `extends`, and `is` map to ordinary relational
   composition. They do not add DL6 syntax or inheritance semantics.

Rejected alternatives:

- Persisting `type_field` rows would create another semantic schema.
- Treating `rel/5` as semantic authority would mix SQLite decisions into the
  target-independent type graph.
- Treating dense catalog IDs as TypeIds would make type equality artifact-local.
- Reading both `type_decl/2` and semantic rows after freeze would retain two
  authorities.

## Sequencing

1. Inventory every producer and consumer of declaration carriers, semantic
   rows, `rel/5`, schema-member views, and catalog rows.
2. Move the canonical freeze after all current generated-type minting.
3. Add completeness and uniqueness checks for declarations, members,
   applications, and anonymous origins.
4. Make compiler relations read canonical rows through ephemeral views.
5. Remove post-freeze semantic reads of `type_decl/2`, `col_type/3`, and
   `keyed/2`.
6. Derive physical rows keyed by canonical IDs.
7. Adapt `rel/5` consumers, then remove the compatibility projection when its
   consumer count reaches zero.
8. Generate catalog rows from canonical semantic and physical rows.
9. Add the TypeSpec parity ledger and field-iteration example to the exhaustive
   language fixture.

<!-- todo(feature): Freeze one complete canonical semantic type graph after every existing generated-type minting phase. -->
<!-- todo(feature): Expose canonical type rows to authored compiler relations through storage-free query views. -->
<!-- todo(feature): Replace post-freeze declaration-carrier reads with canonical row queries. -->
<!-- todo(feature): Derive target storage rows by canonical TypeId and MemberId without copied semantic fields. -->
<!-- todo(feature): Serialize catalog artifacts from canonical semantic and physical rows. -->
<!-- todo(docs): Record TypeSpec parity mappings and intentional relational-composition mappings in an executable DL6 fixture. -->

## Verification

Compiler CI:

- one fixture containing module-qualified records, generic applications,
  wrappers, anonymous products and sums, interfaces, keys, and compiler-time
  type applications;
- canonical row snapshots taken before physical lowering;
- one member row per declared and generated member;
- no post-freeze `type_decl`, `col_type`, or `keyed` terms in the runtime plan;
- compiler and oracle produce equal canonical row sets;
- authored field iteration emits one result per member in source order;
- repeated compilation produces byte-identical canonical rows;
- TS and Rust type artifacts compile;
- SQLite executable plans create and exercise keyed replacement, option, enum,
  relation-reference, and anonymous-value storage;
- JSON Schema generation returns a document or a named refusal;
- the TypeSpec parity probe compiles through executable and type-artifact doors.

Structural checks:

- grep reports zero downstream semantic reads of mutable carriers after the
  freeze function;
- every storage row references an existing canonical TypeId or MemberId;
- every catalog semantic coordinate resolves to one canonical row;
- compiler-query view rows are absent from boot, runtime SQL, DD inputs, and
  host plans.

## Staffing

- Reflection lane: Sol reviewer piloting Terra in
  `/private/tmp/sprefa-type-field-reflection`, based on `65607a8d5`; paused
  until canonical freeze work lands.
- Canonicalization lane: a second Sol reviewer piloting Terra in a separate
  worktree from current `origin/main`.
- Root lane: owns this plan, issue DAG, TypeSpec parity fixture, integration,
  and final CI.
- Full compiler CI budget: one run in each implementation lane after focused
  CI, then one integration run after both commits are combined.
- No lane pushes or merges itself.
