## Costs and limitations

### 1. `_id` is tied to named fields

This works:

```dl6
File.revision_id
```

This does not provide identity for an arbitrary expression:

```dl6
choose(A.revision, B.revision)
some(revision(...))
collect(...)
```

The language may eventually need:

```dl6
id(Value)
```

for computed relation values.

### 2. Storage IDs are local

SQLite `__id` values are meaningful only inside one database state.

```text
database A: revision 42
database B: revision 42
```

Those may identify unrelated revisions. They cannot safely serve as:

- network identifiers
- persistent external references
- identities across database rebuilds
- identities across separate DL6 runtimes

Cross-runtime identity still needs domain keys such as repository plus Git object ID.

### 3. `_id` exposes physical identity semantics

Two structurally equal rows may have the same interned ID today because relations are content-interned. A future storage target might assign identity differently.

Programs using:

```dl6
A.revision_id == B.revision_id
```

become dependent on the runtime’s row identity contract.

DL6 must specify whether relation identity means:

- surrogate storage identity
- structural value identity
- declared key identity

Those currently coincide in some paths and diverge in others.

### 4. Lists contain two identities

```text
History.revisions_id
Member.value_id
```

The suffix alone cannot explain which level is being selected. It depends on the receiver.

This becomes visually dense:

```dl6
Batch.groups.value.members.value_id
```

An explicit member binding is required to keep index, container ID, member ID, and followed value readable.

### 5. Wrapper identities become difficult to navigate

For:

```dl6
option(list(option(revision)))
```

the outer column has one ID, the list has another ID, each inner option has another ID, and each present revision has another ID.

`Column_id` selects only the outer stored identity. Access to inner identities requires explicit wrapper decomposition. Dot access alone cannot express all layers clearly.

### 6. Automatic following hides query work

```dl6
File.revision.object
```

requires a join to `revision`.

```dl6
File.revision_id
```

does not.

A long path can introduce several joins:

```dl6
File.revision.repository.owner.organization.name
```

The syntax does not expose:

- join count
- whether relations are already available in the rule
- whether repeated paths share one join
- requested load depth
- batching boundaries

The compiler must deduplicate identical follows and expose the resulting graph in diagnostics or IR.

### 7. It cannot request partial objects

`File.revision` means the relation value. It does not express:

```text
load only revision.object
load revision without repository
load these three fields
```

DL6 rules can select individual fields afterward, but host serialization needs an explicit projection shape to avoid reconstructing the complete object.

### 8. Cyclic object serialization remains unresolved

```dl6
rel node(parent: option(node)).
```

Following one field in a rule is finite. Serializing `node` as a recursively embedded JSON object is not.

The `_id` convention does not specify:

- maximum expansion depth
- cycle markers
- where expansion switches back to IDs
- whether repeated objects become references

### 9. Polymorphic references are not directly covered

A column whose target can be one of several unrelated relations needs an enum:

```dl6
rel owner(
  file(value: file)
; module(value: module)
).
```

There is no single:

```text
Id<owner target>
```

unless the enum instance itself owns the identity. Accessing the payload target identity requires variant decomposition.

### 10. Composite domain identity remains separate

A Git source may be identified structurally by:

```text
repository + revision + path
```

Its SQLite row also has a surrogate `__id`.

```dl6
Source.id
```

would expose the surrogate. It does not expose the domain identity tuple. The language still needs ordinary typed fields and declared keys for domain identity.

### 11. Historical identity needs an explicit database scope

A row ID from one engine snapshot may disappear or be reused after compaction, regeneration, or migration.

A durable reference would need something closer to:

```text
DatabaseId
SchemaVersion
RelationType
RowIdentity
```

The proposed `Id<T>` only carries the relation type statically.

### 12. Scalar intern identities remain inaccessible

SQLite interns text and JSON too, but:

```dl6
Row.name_id
```

would currently be illegal because `name` is scalar.

Therefore the feature cannot express identity-level equality for:

- interned text
- JSON dictionary entries
- byte blobs
- other scalar dictionaries

That boundary is intentional in the plan, but it means `_id` does not expose every physical ID.

### 13. Constructed values may not have an ID yet

```dl6
Revision := revision(Object)
```

can describe a relation value before lowering interns it. Requesting its ID during the same expression requires ordering:

```text
construct value
intern value
obtain identity
continue evaluation
```

Field access on an already stored owner avoids this problem. Arbitrary constructed-value identity needs an explicit interning operation or compiler phase.

### 14. `_id` names can collide

```dl6
rel file(
  revision: revision,
  revision_id: text
).
```

The plan rejects this. Existing programs using such names would require migration.

It also reserves an expanding namespace because nested/generated members may create names such as:

```text
value_id
list_id
owner_id
```

### 15. Target languages cannot fully enforce runtime scope

Rust and TypeScript can distinguish:

```text
Ref<Revision>
Ref<Repository>
```

They cannot distinguish two different database instances without another type parameter or runtime token:

```rust
Ref<DatabaseA, Revision>
Ref<DatabaseB, Revision>
```

That would complicate generated types and host boundaries.

### 16. It does not express loading policy

The surface distinguishes identity access from one followed value. It does not express Active Record-style policies such as:

- preload
- eager load
- lazy load
- cache-only
- refuse missing
- load at a specific revision
- cap fan-out
- batch size

Those belong to query planning, host projection, or an additional explicit loading operator.

### 17. Missing targets need semantics

A stored reference can point at a target unavailable in the current frontier due to retraction, partial import, or revision filtering.

`File.revision_id` can still return the reference. `File.revision` needs a defined outcome:

```text
no derived row
option(revision)
typed dangling-reference error
```

The `_id` syntax does not choose among them.

### 18. Identity equality and value equality need separate operators

If ordinary equality compares stored IDs, then:

```dl6
A.revision == B.revision
```

already means identity equality, despite appearing to compare objects.

If ordinary equality compares reconstructed structures, it may require recursive comparison.

The language needs explicit definitions for:

```text
Id equality
relation value equality
declared-key equality
deep structural equality
```

The proposed accessors provide the operands, but they do not define those equality modes.
