# Relational Type Schema Wrappers and Literals

## Context

DL6 already has most of the lowering seams needed for a relational type system:

- generic relation and enum applications are specialized in
  `v6/prolog/0_generic_expand.pl` before runtime relation planning;
- semantic type rows already describe declarations, parameters, members,
  constraints, applications, arguments, implementations, and derivations;
- relation arrows in `v6/prolog/compile/parse_dl_dcg.pl` append exactly one
  ordinary column named `return`;
- `v6/prolog/0_rel_record.pl` carries one relation record as
  `rel(Ref, StorageName, Kind, Cols, KeyOrNone)`;
- `keyed(Ref, Positions)` already lowers into `KeyOrNone = key(Positions)` and
  the existing SQLite key/upsert path;
- legacy `Key(T)`, `Min(T)`, and `Max(T)` column wrappers are parsed and then
  refused by name in `parse_dl_dcg.pl`;
- relation-valued columns, enums, lists, options, and relation identities
  already select storage representations through `0_type_plane.pl`.

The desired authored surface keeps existing generic application and constraint
syntax:

```dl6
rel Box(T: capability(any))(
  value: T
).
```

The constrained type is `T`. `capability(any)` is an interface application
pattern. `any` is the only new type-pattern term in that expression.

The wider direction is to describe compile-time capabilities as relations over
type IDs, express functional outputs with the existing relation arrow, express
schema roles through transparent type applications such as `key(T)`, and allow
anonymous product and sum types wherever a type expression is accepted.

## Decisions

### `type` is a compiler value domain backed by `SemanticTypeId`

```dl6
rel capability(
  Self: type,
  Format: type
).
```

Each cell contains a deterministic `SemanticTypeId`. Generic variables in
compile-time rules bind semantic IDs. Canonical type applications mint nodes
from their constructor and ordered argument IDs.

```text
text                 -> Primitive(text)
Document             -> Named(ModuleId, relation, Document)
list(Document)       -> Apply(ListId, [DocumentId])
Result(Error, Value) -> Apply(ResultId, [ErrorId, ValueId])
```

Three identity domains remain separate:

```text
SemanticTypeId      module-qualified compiler identity
CatalogRowId        dense ID allocated for one emitted catalog
RuntimeEndpointId   dense identity of one stored relation value
```

Compiler type relations contain only `SemanticTypeId`. Named IDs include
module identity. Application IDs recursively contain semantic constructor and
argument IDs. Catalog and runtime IDs never enter compiler type relations.

Relations containing `type` columns belong to the compiler plane. They do not
become application SQLite/DD relations unless a later explicit reflection
feature copies selected rows across the phase boundary.

### Capability evidence is Datalog data

Facts:

```dl6
capability(Document, json).
capability(File, text).
capability(File, bytes).
```

Rules:

```dl6
capability(list(T), json) <-
  capability(T, json).
```

The retained generic-bound surface:

```dl6
T: capability(any)
```

lowers into the compile-time query:

```dl6
capability(T, _).
```

`any` is a bound-pattern wildcard and never mints a stored `TypeId`. Exact
arguments remain exact queries. The existing `interface` and `is` spellings
may remain compatibility sugar while programs move toward ordinary facts and
rules.

Current value-rule parsing treats bare identifiers as variables. After the
compiler partitions type-valued relations, a declared-type-aware elaborator
rewrites arguments in `type` columns:

```text
declared primitive/name/application -> SemanticTypeId constant
rule-scoped variable                 -> SemanticTypeId variable
ambiguous identifier                -> named refusal
```

`any` is legal only as one direct interface-bound argument. Nested wildcard
matching and `any` in stored facts, relation columns, implementations, or
concrete generic applications receive named refusals.

The current `compile_type_conformance` code is a fixed interface adapter, not
a general Datalog evaluator. General type relations require a shared
compiler/oracle partition, rule-safety and recursion contracts, set semantics,
functional-conflict diagnostics, and erasure before runtime planning.

### Ordinary arguments and determined outputs share relation shape

```dl6
rel convert(
  Self: type,
  Input: type
) -> type.
```

Canonical columns:

```dl6
rel convert(
  Self: type,
  Input: type,
  return: type
).
```

The current arrow only appends `return`; it does not currently add uniqueness.
Functional or Rust-associated-type semantics require the selecting columns to
form a key:

```dl6
rel convert(
  Self: key(type),
  Input: key(type)
) -> type.
```

This states:

```text
(Self, Input) -> return
```

Rust can emit the relation as:

```rust
trait Convert<Input> {
    type Output;
}
```

A nonfunctional capability relation can emit a generic trait application:

```dl6
rel codec(Self: type, Format: type).
```

```rust
trait Codec<Format> {}
```

The compiler must never infer generic-versus-associated cardinality from the
currently visible implementation rows. The declared key determines it.

### `key(T)` is a transparent schema wrapper

```dl6
rel User(
  id: key(int),
  name: text
).
```

`key(T)` is parsed and interned as an ordinary generic type application. During
column-schema normalization it splits into an underlying value type and a
column role:

```text
value(key(T))   = value(T)
storage(key(T)) = storage(T)
role(key(T))    = key
```

All outer `key(...)` columns collectively form the relation key. The existing
`keyed(Ref, Positions)` and `rel/5` `KeyOrNone` fields remain the canonical
post-normalization lowering target.

```text
column(id, key(int))
        |
        +-- column(id, int)
        +-- keyed(User/2, [1])
```

`key(T)` creates no table and no runtime wrapper value. Generated TypeScript,
Rust, JSON Schema, arrivals, and query rows expose `T`. Catalog metadata may
retain the authored wrapper and its derived column role.

The legacy relation-level `key(Positions)` modifier remains accepted as
compatibility syntax and normalizes into the same keyed-column representation.

`key` is legal only as the outer wrapper of a relation column in the first
slice. Nested occurrences such as `list(key(int))` receive a named refusal.
`key(option(T))` retains the current named refusal. Repeated `key(key(T))`
also receives a named refusal. Wrapper keys and legacy positional keys must
select identical positions or receive a named conflict. Composite key order is
relation-column order.

Normalization runs after concrete generic substitution and before option
expansion, relation-value mirror discovery, storage analysis, and `rel/5`
planning. Runtime relations project roles into existing `keyed/2`. Compiler
relations enforce set deduplication and reject distinct non-key projections for
one complete key without entering runtime keyed-level validation.

### General annotations remain outside this slice

No `@[...](syntax)` system is introduced here. `key(T)` has an existing type
application parser and an existing key-plan destination. A general annotation
system would require stable syntax-site IDs, arbitrary-node attachment,
phase-specific handlers, preservation policy, and diagnostics for every target
kind. Those mechanics remain a separate plan if another feature requires them.

### Anonymous product literals are owner-scoped relation types

```dl6
rel resident(input: text) -> (
  a: int,
  b: text
).
```

The arrow still returns exactly one value. Its type is an anonymous product:

```text
anonymous product at resident.return
        |
        +-- a: int
        +-- b: text
```

The compiler mints an owner-scoped internal relation type, then reuses ordinary
relation-value lowering:

```dl6
rel resident_return(a: int, b: text).
rel resident(input: text, return: resident_return).
```

The internal name is diagnostic-only. Semantic identity derives from module,
owner declaration ID, member site, and the canonical structural type hash.
Two identical anonymous products at different owner sites remain distinct
nominal types unless an explicit named type connects them.

Owner identity is assigned after module resolution and contains a recursive
type-expression site path. Generated types carry explicit anonymous-kind and
origin metadata. TypeScript, Rust, and JSON emission uses semantic reachability
rather than the current `__` substring filter.

Anonymous products currently lack an authored contextual value constructor.
Before runtime-value tests, this arc must either add construction and matching
syntax or keep the first slice schema/arrival-only with named refusals for rule
construction.

### Anonymous sum literals are owner-scoped enum types

```dl6
rel A(
  a: int,
  b: (
    Derp(value: int);
    Derpy(value: float)
  )
).
```

The compiler mints an enum owned by `A.b`, then reuses existing enum expansion,
storage, and TypeScript/Rust/JSON Schema union emission.

```dl6
rel ResultBox(T)(
  result: (
    Ok(value: T);
    Error(message: text)
  )
).
```

Generic substitution occurs before the concrete anonymous enum ID is minted.
The TypeId therefore contains the concrete owner application and variant
payload IDs.

Current enum payloads are named fields with atomic types. The first anonymous
sum slice keeps named fields, widens payloads to complete type expressions,
and materializes anonymous sums before enum context construction. Positional
payloads remain a separate language feature.

Static TS/Rust/JSON enum artifacts are tagged unions, while current runtime
ProgramJson enum cells are integer endpoints. Tagged runtime ingress/egress is
a separate contract that must be selected and implemented in both runtimes
before runtime sum tests claim tagged values.

Anonymous products and sums are valid wherever `type_expr//1` is accepted.
They are not multi-column relation arrows. The arrow continues to append one
`return` column whose value has the anonymous type.

### Multiple associated outputs use one product output

```dl6
rel ConversionTypes(
  Output: type,
  Error: type
).

rel convert(
  Self: key(type),
  Input: key(type)
) -> ConversionTypes.
```

An anonymous spelling is equivalent:

```dl6
rel convert(
  Self: key(type),
  Input: key(type)
) -> (
  Output: type,
  Error: type
).
```

Rust may project product fields as multiple associated types only through a
target-independent type-relation IR that identifies `Self`, trait inputs, key
members, return, and product fields. Current Rust interface emission produces
empty traits, so this projection is new emitter work. DL6 still sees one
product-valued `return` column.

### Canonical IR separates authored convenience from mechanics

Before target emitters, semantic rows expose declaration roles directly:

```text
schema_member(
  MemberId,
  OwnerSemanticTypeId,
  Position,
  Name,
  AuthoredType,
  ValueSemanticTypeId,
  Roles
)

type_relation(
  RelationSemanticTypeId,
  SelfMemberId,
  InputMemberIds,
  ReturnMemberIdOrNone,
  KeyMemberIds
)
```

`Self` is validated as exactly one first `type` member of a trait-like
compiler relation and receives the `self_subject` role. Emitters consume that
role and never render `Self` as an ordinary Rust field. Roles also carry key,
return, and anonymous owner/origin information through catalog and typegen
transport.

The pending interface-bound slice must preserve semantic IDs and complete
bound patterns through `typegen_export`; direct Prolog emitters and DL6
renderers must consume the same rows. Rust wildcard output must be defined and
compiled before that slice is considered end to end.

The compiler converges all surfaces before relation planning:

```text
authored type applications and literals
        |
        v
resolved type graph with TypeIds
        |
        v
generic substitution and concrete type minting
        |
        v
column schema normalization
  - underlying type
  - storage class
  - column roles
        |
        v
existing rel/5 + keyed/2 + type_decl/enum_decl IR
        |
        v
SQLite / Rust / TypeScript / JSON Schema emitters
```

Zig-style schema construction and the turnkey DL6 syntax meet at this IR.
Generated code may expose schema descriptors, while authored DL6 keeps concise
type applications and literals. Emitters do not receive surface-only wrapper
or anonymous-literal syntax.

## Type signatures

```text
internType
  : ResolvedTypeExpression
  -> SemanticTypeId

specializeType
  : Bindings<TypeParameter, SemanticTypeId> × TypeExpression
  -> SemanticTypeId

normalizeColumnSchema
  : MemberId × SemanticTypeId
  -> ColumnSchema {
       valueType: SemanticTypeId,
       storageType: StorageType,
       roles: Set<ColumnRole>
     }

mintAnonymousProduct
  : OwnerSiteId × [NamedTypeId]
  -> SemanticTypeId

mintAnonymousSum
  : OwnerSiteId × [Variant<TypeId...>]
  -> SemanticTypeId

compileTypeRelation
  : RelDecl<SemanticTypeId columns> × TypeFacts × TypeRules
  -> TypeRows

elaborateTypeArgument
  : TypeEnvironment × RuleScope × SurfaceTerm
  -> SemanticTypeId | TypeVariable | Refusal

validateFunctionalRows
  : KeyMemberIds × Set<Row<SemanticTypeId>>
  -> Set<Row<SemanticTypeId>> | ConflictingReturn
```

Body sketches:

```text
normalizeColumnSchema(member, key(inner)):
  child = normalizeColumnSchema(member, inner)
  require key is outermost
  return child with roles += key

mintAnonymousProduct(site, fields):
  children = fields.map(internType)
  id = hash(module(site), owner(site), member(site), product(children))
  emit declaration/member rows under id
  return id
```

## Instance timeline and lifetime

1. Parse generic applications, `key(T)`, arrows, and recursive anonymous-type
   AST nodes without selecting storage or minting owner identity.
2. Resolve modules, declarations, generic parameters, and compiler-relation
   schemas.
3. Mint module-qualified named semantic IDs and recursively canonical
   application IDs.
4. Partition compiler relations from runtime relations in the shared compiler
   and oracle path.
5. Elaborate arguments in `type` columns into semantic constants or scoped
   variables; validate `Self`, `any`, rule safety, and interface arity.
6. Evaluate compiler relations to a fixpoint, validate functional keys, and
   query generic bounds.
7. Substitute concrete generic arguments.
8. Mint owner-scoped anonymous product/sum IDs after concrete ownership is
   known and before enum context, option, mirror, and storage phases.
9. Normalize transparent schema wrappers into underlying types plus member
   roles.
10. Project runtime key roles into existing `keyed/2` and `rel/5`; retain
    compiler key roles for functional validation.
11. Materialize ordinary product/enum declarations and lower runtime relations.
12. Erase compiler-only facts, rules, proofs, and wildcard patterns. Retain
    semantic catalog rows needed by type emitters and graph inspection.

Compile-time facts live for one compiler invocation. Deterministic TypeIds and
owner-scoped anonymous type identity remain reproducible across invocations.

## Storage, reads, writes, and uniqueness

- Type-valued compile relations store `SemanticTypeId` cells in the compiler plane.
- `any` is a query wildcard and is never stored.
- `key(T)` uses `T`'s storage representation and adds no cell.
- All `key(T)` columns collectively define the existing relation key.
- Anonymous products use existing relation-value identity and struct-plane
  storage.
- Anonymous sums initially use existing enum endpoint storage; tagged public
  runtime values require the separate ingress/egress contract above.
- A product-valued return remains one owner column referencing one product
  value; it is not flattened into sibling output columns.
- Functional type relations reject two distinct return values for one complete
  key.
- Nonfunctional capability relations allow several argument rows for one
  implementing `Self` type.

## Rejected alternatives

- General `@[...](syntax)` annotations in this slice: no additional feature
  currently requires arbitrary syntax-node metadata.
- Inferring associated types from observed row counts: adding evidence would
  change the emitted trait contract.
- Treating `any` as a stored top-type value: wildcard matching is query
  behavior.
- Flattening anonymous product returns into arrow output columns: changes the
  one-return-column contract.
- Structural deduplication of anonymous types across owner sites: introduces
  accidental type equivalence.
- A second type-ID registry for compile-time relations: duplicates the existing
  semantic type graph.

## Sequence

1. Repair the pending interface-bound vertical slice: preserve semantic IDs and
   complete patterns through typegen export, define Rust wildcard output, and
   compile emitted TS/Rust artifacts.
2. Establish module-qualified recursive `SemanticTypeId` while preserving
   separate catalog and runtime ID domains.
3. Add canonical semantic member/type-relation IR with `self_subject`, key,
   return, and anonymous-origin roles.
4. Add type-valued argument elaboration and a shared compiler/oracle relation
   partition. Specify safe rules, fixpoint recursion, set semantics, and
   functional conflicts before enabling authored compiler facts/rules.
5. Split generic specialization from option lowering where required by schema
   normalization.
6. Normalize lowercase `key(T)` after specialization and before option/mirror/
   storage phases; project runtime roles through existing `keyed/2`.
7. Add recursive anonymous type AST, printer, Tree-sitter CST, module-resolved
   owner paths, and recursion diagnostics.
8. Materialize anonymous products, define reachability, and settle contextual
   construction/matching before runtime value CI.
9. Add type-relation-to-Rust-trait lowering, including implicit `Self` and
   product-field associated outputs.
10. Materialize anonymous sums after widening enum payload types and fixing
    enum-context order; select ID-valued or tagged runtime representation.
11. Port selected built-in interface evidence to ordinary compiler facts and
    rules. Compatibility sugar changes require a separate ruling.

Each sequence item must leave one canonical post-normalization IR. No emitter
may independently reinterpret `key(T)` or anonymous syntax.

## Verification

### Parser and printer

- Parse, print, and reparse `key(T)` in generic and concrete columns.
- Parse, print, and reparse anonymous products and sums in columns and arrow
  return positions.
- Preserve the existing one-type relation-arrow grammar.
- Produce named refusals for nested key placement and unsupported recursive
  anonymous types.

### Type graph

- TypeId determinism across reordered unrelated declarations.
- Owner-site distinction for structurally equal anonymous types.
- Concrete generic arguments included in anonymous type IDs.
- No second IDs for the same named or concrete generic type.

### Relational and storage IR

- `key(T)` and legacy `key(Positions)` produce identical `keyed/2`, `rel/5`,
  SQLite DDL, upsert SQL, and replacement behavior.
- Composite `key(type)` columns enforce functional compile-time relation
  outputs.
- `key(T)` emits the same public TS/Rust/JSON value type as `T`.
- Anonymous products reuse relation-value storage without flattening.
- Anonymous sums reuse enum storage; static tagged artifacts and runtime
  endpoint values are verified as separate contracts until ingress/egress is
  implemented.

### Cross-target CI

- Golden DL6 source covers one generic key wrapper, one product return, one
  inline sum, and one type-valued functional relation with concise comments.
- Prolog compiler CI exercises parse, expansion, semantic rows, and emitted SQL.
- TypeScript runtime CI executes concrete product values after construction is
  defined; sum values wait for the runtime representation decision.
- Rust emitted-program CI compiles and executes the corresponding values.
- Type artifact CI compares TypeScript, Rust, and JSON Schema structure.
- Existing explicit forms remain equivalent to their sugared forms.
- Real `.dl6` fixtures, rather than hand-authored JSONL only, pass through
  parser, expansion, catalog, and typegen export.
- Generated TypeScript passes `tsc --noEmit`; generated Rust passes a temporary
  crate build; generated JSON Schema passes metaschema and instance validation.
- The typegen CI is included in the repository CI battery.

## Staffing

- Two Sol review lanes inspect this plan and current code/goldens independently.
  They are read-only and must report concrete contradictions, missing seams,
  ordering hazards, identity/storage errors, and a corrected dependency DAG.
- Implementation is split only after both reviews are reconciled.
- Every implementation lane uses an isolated worktree based on the then-current
  `origin/main`.
- Focused CI runs during implementation. One compiler CI and one cross-target
  emitted-program CI run after integration.
- Formatter and linter status are outside acceptance.

<!-- todo(decision): Select contextual construction and matching semantics for anonymous product values. -->
<!-- todo(decision): Select ID-valued or tagged runtime ingress and egress for anonymous sums. -->
<!-- todo(feature): Normalize lowercase `key(T)` through the existing keyed relation IR. -->
<!-- todo(feature): Add owner-scoped anonymous product and sum type literals. -->
<!-- todo(feature): Represent type-valued capability and functional-output relations over semantic type IDs. -->
