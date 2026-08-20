# Relation Value Identity Access

## Context

DL6 already lowers a column whose declared type names another relation to
`ref(Target)`. SQLite stores the target relation's dense `__id`, while rendered
rows reconstruct the target object. The language currently lacks a surface
choice between those two views.

Lists add a second identity layer. A `list(T)` column stores the interned list
entity ID. Its member relation stores `(list_id, idx, value)`. When `T` names a
relation, `value` is itself a target relation ID. The existing storage shape is
covered for scalar members by
`v6/prolog/conformance/fixtures/19_list_value_position.pl`, for relation members
by `v6/prolog/conformance/fixtures/10_list_elements.pl`, and by the `ref(_)`
lowering in `v6/prolog/lower.pl`.

The language surface should expose these existing identities without authored
`repository_id(value: text)` or similar mirror relations. Domain relations keep
domain names. Generated `_id` access selects the identity already stored by the
runtime.

## Type signatures

For a relation `R`, its nominal reference type is written `Id<R>` in this plan.
`Id<R>` is compiler IR notation, not proposed authored DL6 syntax.

```text
relation column value:     get<R>(row, column)       -> R
relation column identity:  get_id<R>(row, column)    -> Id<R>

list column value:         get_list<T>(row, column)  -> list<T>
list container identity:   get_list_id<T>(row, col)  -> Id<list<T>>

list member value:         member<T>(list, index)    -> T
relation member identity:  member_id<R>(list, index) -> Id<R>
```

The compiler vocabulary is:

```text
Id<T>          database-epoch-local integer identity
Key<T>         finite portable logical key
State<T>       one-level current fields; relation fields remain typed IDs
Expansion<T,P> explicit followed projection along path set P
Stored<T>      physical typed cell used for T
```

`Value<T>` is avoided because it conflates current stored state with recursive
object expansion.

Authored surface:

```dl6
rel file(revision: revision, path: text).

# `revision` follows the relation value.
file_revision(File, Revision) <- file(File), Revision := File.revision.

# `revision_id` returns the stored typed reference.
file_revision_ref(File, RevisionId) <-
  file(File), RevisionId := File.revision_id.
```

The `_id` suffix is an accessor synthesized from the declared column. It is not
an additional stored column.

## Instance timelines

### Relation-valued column

```text
author writes revision: revision
    -> type plane resolves ref(revision)
    -> storage writes file.revision = revision.__id
    -> File.revision_id returns Id<revision>
    -> File.revision joins/decodes revision only when referenced
```

### List-valued column

```text
author writes revisions: list(revision)
    -> list value is content-interned
    -> owner column stores list entity __id
    -> Owner.revisions_id returns Id<list<revision>>
    -> Owner.revisions traverses ordered member rows
    -> member value follows revision
    -> member identity returns Id<revision>
```

### Retraction

```text
owner row retracts
    -> owner-to-list or owner-to-relation edge retracts
    -> target/list state has an independent lifetime
    -> owner identity and followed projections disappear
```

No accessor creates durable state.

```text
target replacement -> parent `_id` stays; followed projection changes
target deletion    -> parent `_id` stays; ordinary follow yields no solution
```

## Storage and uniqueness

The selected storage convention is a database-local integer surrogate. A
content hash may participate in a domain key, but it is never the stored
relation reference.

```text
logical identity = relation type + canonical declared-key values
storage identity = database-local integer assigned to that logical identity
```

The compiler preserves the other views as relations over the same row:

```text
row_key(Relation, State, Key)
row_id(Relation, Key, Id)
row_state(Relation, Id, State)
follow(Relation, Id, State)
```

These are compiler semantics, not three competing storage formats. A query may
select the local integer, portable logical key, followed value, or fields of the
followed value without changing the stored column.

| Authored column | Stored cell | Value access | Identity access |
|---|---|---|---|
| `text` | dictionary-backed scalar | `Row.name` | none |
| `revision` | `revision.__id` | `Row.revision` | `Row.revision_id` |
| `list(text)` | list entity `__id` | `Row.names` | `Row.names_id` |
| `list(revision)` | list entity `__id` | `Row.revisions` | `Row.revisions_id` |
| `option(revision)` | absent/present relation edge | `Row.revision` | `Row.revision_id: option(Id<revision>)` |
| enum payload `revision` | variant payload target ID | followed payload field | payload field with `_id` |

Uniqueness domains remain distinct:

```text
Id<revision>       != Id<repository>
Id<list<revision>> != Id<revision>
Id<list<R>>        != Id<R>
```

All may be represented by SQLite integers. The compiler and generated targets
retain their nominal type parameters.

### Identity laws

```text
same Key<T> in one database epoch -> same Id<T>
Id<T> -> at most one current State<T>
State<T> -> exactly one finite Key<T>
```

With no explicit `key(...)`, all declared columns form `Key<T>`. Structural
value identity is therefore the default key policy. With an explicit key, the
selected columns form `Key<T>` and non-key columns are replaceable state for
the same `Id<T>`.

`row_id` is monotone and tombstoned for one database epoch. Integers are never
reused for another key during that epoch. State removal retains the key-to-ID
mapping, so delete followed by reinsert recovers the same `Id<T>` in that epoch.

The snapshot invariant for an identity-bearing entity relation is:

```text
for each Key<T>, at most one distinct live State<T>
```

Any ingress or derivation violating it produces a deterministic conflict. A
replacement across frontiers is an atomic `-old, +new` transition retaining
`Id<T>`. Existing keyed edge and log relations retain their occurrence or
last-write semantics until explicitly declared as entity relations.

Cross-database transport carries `Key<T>` or complete domain state:

```text
resolve(Key<T>)  -> option(Id<T>)
upsert(State<T>) -> Id<T>
```

A key resolves existing state and cannot manufacture missing non-key columns.
Portable keys are finite and recursively canonical:

```text
key_component(scalar)    = typed scalar value
key_component(relation)  = Key<relation>
key_component(list<T>)   = ordered key_component(T), duplicates retained
key_component(option<T>) = none | some(key_component(T))
key_component(enum E)    = variant tag + ordered payload key components
```

When the default all-column key dependency graph cycles, the compiler requires
an explicit finite nonrecursive key. Genuine cyclic graphs without one have no
portable `Key<T>` in this arc.

## Important cases

### 1. Scalar relation column

```dl6
rel revision(object: git_oid).
rel file(revision: revision, path: text).
```

`File.revision_id` is `Id<revision>`. `File.revision` is `revision`.

### 2. Multiple references to the same target

```dl6
rel comparison(left: revision, right: revision).
```

`Comparison.left_id == Comparison.right_id` tests relation identity without
loading either row. State equality is finite fieldwise equality at one snapshot;
identity-bearing fields compare their typed IDs. Deep followed equality requires
an explicit recursive relation and cycle policy.

### 3. List of scalars

```dl6
rel document(lines: list(text)).
```

`Document.lines_id` exposes the list container identity. Spread or membership
over `Document.lines` yields `text`. Scalar member dictionary IDs stay internal.

### 4. List of relations

```dl6
rel history(revisions: list(revision)).
```

The container and element references are separate:

```text
History.revisions_id -> Id<list<revision>>
member.value_id      -> Id<revision>
member.value         -> revision
```

The list traversal IR therefore needs a typed member binding containing
`index`, `value_id`, and `value`. Existing spread syntax may bind `value`; an
explicit member form must expose `index` and `value_id`.

Conceptual relational projection:

```text
list_value(ListId, List)
list_member(ListId, Index, StoredValue)
follow_member(StoredValue, State)
```

For relation-valued members, `StoredValue` is `Id<T>`. `Member.value_id` is a
typed view of the existing member `value` cell. `Member.value` adds one follow
join. No fourth stored member column is created. Equal ordered member keys
produce the same list key and list ID within one database epoch. Reordering
produces another list key.

Array/value ingress interns portable ordered member keys and rewrites them to
local list/member IDs. Raw `Id<list<T>>` ingress is database-local and cannot
cross a host boundary.

<!-- todo(decision): Select the authored spelling for explicit list membership with index and relation member identity after inspecting existing spread and relation-edge syntax. -->

### 5. Option of relation

```dl6
rel checkout(head: option(revision)).
```

`Checkout.head_id` yields `option(Id<revision>)`. The current companion relation
represents `none` by absence and has no outer `Id<option<revision>>`.
`Checkout.head` yields no row for `none` or one followed `revision` for `some`.
A dangling present ID remains distinct from `none`; ordinary following is an
inner join and yields no solution for the missing target.

```text
stored state: none | some(Id<revision>)
followed result: zero rows | revision
```

This case requires the current `option_in_key_column` restriction to be
reconciled with enum payload and option storage expansion.

### 6. Nested lists and options

```dl6
rel matrix(rows: list(list(revision))).
rel candidates(values: option(list(revision))).
```

Every materialized list or enum wrapper contributes one identity level. Option
uses absence/presence unless authored as an identity-bearing enum relation.
`_id` selects the actual outer stored representation. Explicit decomposition
exposes the next wrapper or member identity.

### 7. Enum payload relation

```dl6
rel revision_source(
  branch(head: revision)
; detached(commit: revision)
).
```

Variant payload access follows the same rule: `head` yields `revision`, and
`head_id` yields `Id<revision>`. The enum instance keeps its own independent
identity.

### 8. Recursive acyclic chains

```dl6
rel node(parent: option(node), name: text).
```

The current option companion represents acyclic parent chains. A genuine cycle
requires an explicit finite nonrecursive key and a storage change to the current
acyclic guard. `Node.parent_id` remains finite. `Node.parent` adds one explicit
follow. Serializers use an explicit expansion path and retain deeper edges as
IDs.

### 9. Generated-name collision

```dl6
rel file(revision: revision, revision_id: text).
```

This declaration conflicts with the synthesized `revision_id` accessor. The
compiler reports the relation, source column, conflicting authored column, and
location. No silent renaming occurs.

### 10. Host and target boundaries

Host descriptors distinguish `Id<R>` from followed `R`, and
`Id<list<T>>` from `list<T>`. Rust and TypeScript receive nominal reference
types so unrelated integer identities cannot be exchanged accidentally.

```rust
struct Ref<T> {
    value: i64,
    marker: PhantomData<fn() -> T>,
}
```

```ts
declare const relationRef: unique symbol
type Ref<Name extends string> = number & {
  readonly [relationRef]: Name
}
```

JSON transport uses a tagged or schema-qualified reference representation. Raw
SQLite integers do not cross the host boundary without their target type.

## Lowering sketch

Compiler IR distinguishes these projections:

```text
Id<T>                    database-epoch-local integer reference
Key<T>                   finite canonical logical key values
State<T>                 one-level current stored state
Expansion<T, P>          explicit joins along projection path P
Member<list<T>, T>       list id, index, Stored<T>
Stored<scalar>           scalar
Stored<relation>         Id<relation>
Stored<list<T>>          Id<list<T>>
Stored<option<T>>        none | some(Stored<T>)
Stored<enum E>           Id<E>
```

```text
resolve_member(RowType, Column):
  column = lookup_declared_column(RowType, Column)
  return followed_type(column.storage_type)

resolve_member(RowType, Column + "_id"):
  source = lookup_declared_column(RowType, Column)
  require source.storage_type is ref(T), list(T), option(T), or enum-backed
  reject authored/synthesized name collision
  return nominal_identity_type(source.storage_type)

lower_followed_member(row, ref(T)):
  add target relation join keyed by stored id
  return typed target row binding

lower_identity_member(row, storage):
  return stored column expression with nominal Id<storage> type

lower_key(state: State<T>):
  project the declared key columns in canonical order
  return Key<T>
```

List traversal extends the binding rather than manufacturing another relation:

```text
bind_list_member(list<T>):
  return {
    index: int,
    stored: Stored<T>
  }
```

`Member.value_id` projects `stored` when it is identity-bearing.
`Member.value` follows it. Identity-only membership reads no target table;
requesting both identity and value shares one target join.

## Decisions

- Bare relation column declarations keep the current `ref(Target)` storage.
- `Row.column` follows the domain value.
- `Row.column_id` exposes the stored nominal identity.
- `_id` access is compiler-generated and does not add schema columns.
- Relation references are stored as database-local integers.
- Integer identity is scoped to `(database epoch, nominal relation type)`.
- Key-to-ID mappings are monotone and tombstoned for one database epoch.
- Content hashes remain domain values and may participate in `Key<T>`.
- Structural value identity is the default `key(all declared columns)` policy.
- An explicit `key(...)` makes non-key columns replaceable state for one ID.
- `Id<T>`, `Key<T>`, `State<T>`, and `Expansion<T,P>` remain independently
  queryable relational projections.
- List container identity and relation-member identity remain separate.
- Scalar dictionary IDs remain storage implementation details.
- Every followed access is visible in authored syntax and lowering IR.
- Recursive serialization is bounded by an explicit requested expansion shape.
- Options use absence/presence and expose `option(Id<T>)`; an outer option ID
  requires an authored identity-bearing enum relation.
- Rejected alternative: authored `ref(T)` versus `embed(T)` column wrappers.
- Rejected alternative: mirror relations such as `repository_id(value: text)`.
- Rejected alternative: automatic recursive side-loading at host serialization.
- Rejected alternative: content hashes or strings as stored relation IDs.

## Sequence

1. Add nominal identity types to the compiler type plane and ProgramJson type
   catalog.
2. Represent `Id<T>`, `Key<T>`, `State<T>`, and `Expansion<T,P>` as distinct
   compiler IR types and specify the default and explicit key laws.
3. Add the tombstoned key-to-integer identity map and deterministic one-live-state
   conflict rail for entity relations.
4. Add collision-checked `_id` member resolution for relation columns.
5. Add `key(State)` projection without changing storage.
6. Add followed relation member lowering with explicit join evidence in IR.
7. Add list-container `_id` resolution.
8. Choose and implement explicit list membership binding for index, value ID,
   and followed value.
9. Extend option and enum decomposition with payload identity access.
10. Emit nominal reference and key types in Rust, TypeScript, and JSON Schema.
11. Decode reference, key, and followed shapes through native hosts.
12. Replace authored source wrapper-ID relations with structural domain types.

## Verification

- Parser and resolver goldens for `column`, `column_id`, nested access, and
  imported relation types.
- Compiler refusal golden for authored `_id` collision.
- SQL golden proving identity access adds no join and followed access adds one
  keyed join.
- Runtime golden for two columns referencing the same target ID.
- Runtime golden proving default all-column keys and explicit subset keys map
  equal keys to one integer ID.
- Conflict golden for unequal simultaneous values with one explicit key.
- Replacement golden proving a later non-key change retains the integer ID.
- Cross-database golden proving the logical key survives while local integer IDs
  may differ.
- Runtime goldens for `list(text)`, `list(relation)`, duplicate relation members,
  empty lists, nested lists, and option-wrapped lists.
- Owner delete, target replacement, and dangling-target ticks prove the distinct
  identity and followed-projection timelines.
- Restart tests prove stored identities decode to the same nominal target.
- Rust and TypeScript compile tests reject assignment between unrelated
  `Ref<T>` types.
- JSON Schema distinguishes reference values from expanded objects.
- Query-plan check verifies identity-only access does not read the target table.
- List-member SQL goldens prove ID-only adds no target join, value-only adds one,
  and requesting both shares one join.
- Scale receipt compares identity-only and followed traversal over 100,000
  owner rows and reports wall time, peak RSS, target-table rows read, and result
  count.

<!-- todo(feature): Implement typed `_id` access for relation-valued columns and retain the target relation in compiler IR. -->
<!-- todo(feature): Add distinct Id<T>, Key<T>, State<T>, and Expansion<T,P> compiler projections with all-column default keys and explicit subset keys. -->
<!-- todo(decision): Define database epoch creation and persistence boundaries for tombstoned key-to-ID mappings. -->
<!-- todo(decision): Restrict entity identity semantics to declared entity/arrival relations or extend it to derived and keyed edge relations. -->
<!-- todo(decision): Confirm list relation-element keys use ordered portable Key<T> values while local member cells store Id<T>. -->
<!-- todo(decision): Confirm ordinary dangling-reference follow yields no solution and add a separate try-follow relation only if demanded. -->
<!-- todo(feature): Expose list container identity plus relation-valued member identity without flattening wrapper layers. -->
<!-- todo(feature): Reconcile option and enum payload storage so optional relation identities compile and decompose. -->
<!-- todo(feature): Carry nominal references through ProgramJson, Rust, TypeScript, JSON Schema, and native host decoding. -->
<!-- todo(bug): Reject authored columns whose names collide with synthesized `_id` accessors. -->
<!-- todo(perf): Measure identity-only versus followed access over 100,000 rows and assert the identity path performs no target-table read. -->

## Staffing

- Compiler surface and lowering: Terra-class agent in an isolated worktree.
- List, option, and enum runtime matrices: Luna-class agents after the compiler
  contract lands.
- Target emission and native host decoding: Terra-class agent after ProgramJson
  identity nodes land.
- Base SHA: the integration branch containing wrapper composition, persistent
  lists, and the named host type catalog.
- Suite budget: focused parser/lowering tests during implementation; fresh
  generated Rust and TypeScript runtime fixtures before commit; full Prolog,
  Rust, and TypeScript gates once immediately before integration.

## Independent review record

Two independent Sol reviews were run before implementation. Their common
corrections are incorporated above:

- Option-of-relation currently has presence/absence companion storage and no
  outer option entity ID.
- Key-to-ID lifetime requires a database epoch and tombstoned mapping.
- Current state and recursively followed expansion require separate types.
- Default all-column keys require finite dependency graphs; recursive cycles
  require explicit finite keys and additional storage work.
- List members retain the existing three stored fields; `value_id` is a typed
  view of `value`, and following adds a join.
- Owner deletion, target replacement, and dangling targets have distinct
  timelines.
- Entity-key conflict semantics cannot silently replace the existing semantics
  of occurrence/log and keyed edge relations.

A subsequent Flash4 review run was discarded after its transcript left the
task context and emitted unrelated repository paths. It made no file changes.
