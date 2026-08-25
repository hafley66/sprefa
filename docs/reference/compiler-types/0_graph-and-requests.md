# Compiler types as relations

> Status, 2026-08-25: request-row implementation notes remain useful. The
> document's member-node, synthetic member identity, generic application, and
> constraint sections predate the held relational model. Use
> `plans/2026-08-25-relational-semantic-planes.md` for current semantics until
> this reference is rewritten.

This document describes the V6 compiler type graph and the request relations
used to construct derived relation types. It distinguishes the relational
language model from the current internal carrier terms.

Current implementation status:

| Layer | Status | Representation |
|---|---|---|
| User-land compiler queries | implemented | `type.node`, `type.edge`, `type.member`, `type.project`, `type.requested` |
| User-land derived type construction | implemented | the three `derived_*_request` relations |
| Canonical compiler storage | implemented internal carrier | `declaration(...)`, `member(...)`, `member_role(...)`, and related rows inside `semantic_type_rows` |
| Runtime planning | implemented internal carrier | `rel/5` and its column plans |
| Schema constraint graph | planned by `userland-constraint-graph` | `schema.constraint` and `schema.constraint_member` |

The language-facing direction is that every non-primitive construct is
queryable as relations, nodes, edges, members, and applications. Some compiler
internals still use specialized Prolog terms after those relations are erased.

## 1. Core model

Primitive value domains are the leaves of the type graph:

```text
int  float  bool  text  bytes  json
```

Every non-primitive type is represented by a semantic ID. Named relation
types, enum variants, anonymous types, type applications, type parameters,
and members all have IDs.

```text
named relation type:    User
type application:       option(User)
member:                 User.name
anonymous type:         Request.payload
```

The ordinary compiler-facing graph relations are:

```dl6
type.node(Id, Kind, Label).
type.edge(Edge, Owner, Role, Position, Label, Target).
type.member(Member, Owner, Position, Name, Target).
type.path(Id, Segments).
```

`type.member/5` is an indexed schema view of member edges. It gives member
rules direct access to the member ID, owner type ID, ordinal, name, and target
type ID.

```text
User type node
    |
    +-- member edge [position 1, label id] --> int
    |
    +-- member edge [position 2, label name] --> text
```

Equivalent rows:

```dl6
type.member(UserIdMember, User, 1, id, int).
type.member(UserNameMember, User, 2, name, text).
```

The runtime planner's internal `rel/5` term has a different job. It describes
execution and storage for one compiled relation. `type.member/5` describes the
semantic schema as queryable compiler data.

## 2. Annotations and roles

An annotation is a declared relation applied in type position:

```dl6
rel key(Target: type) -> Target.

rel User(
  id: key(int),
  name: text
).
```

The annotation application has an annotation-site node and an annotation edge
from the member to that site. This preserves the application itself as graph
data.

```text
User.id member
      |
      +-- annotation --> key(int) application site
```

A member role is the current compact projection used by downstream compiler
rules:

```dl6
type.member_role(Member, Role, Argument).
```

For `id: key(int)`, the projected fact is:

```dl6
type.member_role(UserIdMember, key, '').
```

Roles are metadata predicates on member identities. The current canonical
carrier is `member_role(MemberId, RoleTerm)`. The compiler-facing relation
splits `RoleTerm` into `Role` and `Argument`.

Conceptually, a role can be represented as another graph edge:

```text
member node -- role --> role application or role declaration
```

The current implementation retains the specialized member-role relation
because storage and type generators already consume it. Annotation edges keep
the general relation application available for user-land rules.

## 3. Constraint facts

A constraint fact describes a relation-wide invariant. Composite constraints
need a group node because one constraint can own several ordered members.

```text
Revision
   |
   +-- primary/default
           |
           +-- 1 --> tenant_id
           +-- 2 --> revision_id
```

Planned normalized relations:

```dl6
schema.constraint(Owner, Kind, Group).
schema.constraint_member(Owner, Kind, Group, Ordinal, Member).
```

Example facts:

```dl6
schema.constraint(Revision, primary, default).
schema.constraint_member(Revision, primary, default, 1, RevisionTenant).
schema.constraint_member(Revision, primary, default, 2, RevisionId).
```

Member roles provide annotation evidence. Constraint facts assemble that
evidence into complete relation-level objects. Emitters can project the same
constraint rows into SQLite, PostgreSQL, documentation, or another target.

The compiler also has existing `constraint(...)` semantic rows for generic
type bounds. Those rows represent `T` satisfying an interface application.
They are separate from `schema.constraint(...)` storage/schema invariants.

## 4. Why request rows exist

A type mapper computes a relation schema from other type data:

```dl6
rel Partial(Source: type) -> type.

rel Holder(value: Partial(User)).
```

`Partial(User)` has a canonical semantic application ID immediately:

```text
application(Partial, [User])
```

At that point the compiler knows the identity being referenced. It still
needs rows describing the generated relation and each generated member.

Request relations are the boundary between those two facts:

```text
application identity exists
          |
          v
type.requested(Application, Constructor, Arguments)
          |
          v  user-land DL6 fixpoint
derived_relation_request(...)
derived_member_request(...)
derived_member_role_request(...)
          |
          v  validation and materialization
canonical relation, member, role, node, and edge rows
```

A request row is a declarative statement about the shape that must exist. It
has no arrival order and performs no mutation by itself. Set semantics remove
duplicate derivations.

## 5. Request relation signatures

### `type.requested/3`

```dl6
type.requested(Application, Constructor, Arguments).
```

Meaning:

```text
The compiled program contains a closed use of Constructor(Arguments), whose
canonical result identity is Application.
```

Example:

```dl6
type.requested(PartialUser, Partial, [User]).
```

The compiler seeds this row when a closed type application is used in a type
position or matched by a compiler rule. User-land mapper rules consume it.
The constructor must already be declared. This relation does not invent a
constructor from an unknown name.

The internal carrier spelling is `type_requested/3`; dotted compiler syntax
exposes it as `type.requested/3`.

### `derived_relation_request/4`

```dl6
derived_relation_request(
  Application,
  Constructor,
  Arguments,
  MemberCount
).
```

Meaning:

```text
Materialize Application as a relation type produced by
Constructor(Arguments), with exactly MemberCount members.
```

Example:

```dl6
derived_relation_request(PartialUser, Partial, [User], 2).
```

This is the generated relation header. Each application must derive exactly
one distinct header.

### `derived_member_request/4`

```dl6
derived_member_request(Application, Position, Name, Type).
```

Meaning:

```text
Application has the member Name at one-based Position, targeting Type.
```

Example:

```dl6
derived_member_request(PartialUser, 1, id, option(int)).
derived_member_request(PartialUser, 2, name, option(text)).
```

The complete member set must contain every position from `1` through
`MemberCount` exactly once. Member names must be unique within the generated
relation.

### `derived_member_role_request/4`

```dl6
derived_member_role_request(Application, Position, Role, Argument).
```

Meaning:

```text
Attach Role(Argument) metadata to the generated member at Position.
```

Example:

```dl6
derived_member_role_request(PartialUser, 1, optionalized, '').
derived_member_role_request(PartialUser, 2, optionalized, '').
```

Role rows are optional. Every role must point at a requested member position.
For one `(Application, Position, Role)` key, exactly one argument is allowed.

## 6. `Partial` as a complete mapper

```dl6
rel Partial(Source: type) -> type.

derived_relation_request(Output, Partial, [Source], Count) <-
  type.requested(Output, Partial, [Source]),
  type_field_count(Source, Count).

derived_member_request(Output, Position, Name, option(MemberType)) <-
  type.requested(Output, Partial, [Source]),
  type.member(_, Source, Position, Name, MemberType).

derived_member_role_request(Output, Position, optionalized, '') <-
  type.requested(Output, Partial, [Source]),
  type.member(_, Source, Position, _, _).
```

For this input:

```dl6
rel User(id: int, name: text).
rel Holder(value: Partial(User)).
```

the request closure describes:

```text
Partial(User)
   member 1: id   -> option(int)   role optionalized
   member 2: name -> option(text)  role optionalized
```

The TypeScript type-level analogue is:

```ts
type PartialRelation<T> = {
  [K in keyof T]?: T[K]
}
```

The DL6 version exposes each mapped member as a row, allowing other compiler
rules to join, filter, annotate, or assemble those rows.

## 7. Validation and atomicity

The host validates the complete request closure before materializing anything.
For each application it requires:

1. A corresponding `type.requested/3` demand.
2. Exactly one distinct relation header.
3. Header constructor and arguments equal to the application identity.
4. A non-negative integer member count.
5. Exactly that number of member rows.
6. Member positions exactly `1..MemberCount`.
7. Unique member names.
8. Valid, ground semantic type IDs.
9. Roles referencing existing member positions.
10. One ground argument per `(Position, Role)`.

Failure rejects the whole generated shape. No compiler round observes a
partially generated relation.

## 8. Lifetime

```text
parse program
    |
    v
freeze authored type graph
    |
    v
seed type.requested rows
    |
    v
run compiler-relation fixpoint
    |
    v
validate request closure
    |
    v
materialize and refreeze generated types
    |
    v
erase compiler-only request relations
    |
    v
build runtime plans and target artifacts
```

Request rows live only during compilation. Generated semantic type and member
rows survive for catalog and type-generator consumers. Runtime relations never
receive the compiler request protocol.

This compiler cycle is separate from the runtime IDB fixpoint and from runtime
tick transitions such as `<+`.
