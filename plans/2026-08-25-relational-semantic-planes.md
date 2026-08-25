# Relational semantic planes

Date: 2026-08-25

Status: planning reconciliation after the user-land type graph, integrity,
temporal history, clock-checker, and CSP discussions.

Issues: `@relational-semantic-planes`, `@type-edge-view`,
`@relation-application-semantics`, `@userland-integrity-graph`,
`@userland-flow-graph`, `@clock-flow-projection`,
`@flow-cardinality-proof`, `@userland-materialization-graph`, and
`@csp-protocol-library`.

## 0. Purpose

The compiler already exposes a canonical type graph and can run ordinary DL6
rules at compile time. The next work separates four semantic planes before a
target emitter chooses SQLite, PostgreSQL, DBSP, Rust, memory, or another
representation.

```text
Type Graph
  type nodes and primordial edges
        |
        v
Integrity Graph
  valid relational snapshots
        |
        v
Flow Graph
  changes, clocks, signs, rings, and cardinality
        |
        v
Materialization Graph
  required physical artifacts and retained state
        |
        v
Target Plan
  concrete target representation
```

This plan records current implementation facts separately from the language
model. Specialized Prolog carrier terms may remain during migration. They do
not define the final user-land relation signatures.

## 1. Decisions held from the conversation

### 1.1 Type nodes

A node is a type. A type is either a relation interface or a primitive leaf.
Primitives are the bootstrap exception to the everything-is-a-relation model.

```text
type = rel | primitive
```

A relation interface is a type because its return facade is the type exposed
through another relation edge.

### 1.2 Primordial edges

Each declared relation column contributes an edge between type nodes. Its
user-land tuple is ordered:

```text
(Owner, Name, Target, Index)
```

The held declaration shape is:

```dl6
rel type.edge(
  Owner: key(type),
  Name: key(text),
  Target: type,
  Index: int
).
```

`Owner` and `Name` form the natural identity. `Target` and `Index` are edge
data. `Owner.Name` is projection syntax for the pair, not another authored
identity.

The current compiler exposes the legacy `type.member/5` relation with a
synthetic `MemberId` first and stores `member(Owner, Position, Name)` as a
carrier. That representation is an implementation migration concern. The held
user-land graph uses `type.edge/4`.

### 1.3 `key` is a relation

`key` is an ordinary relation used in type position:

```dl6
rel key(Target: type) -> Target.

rel User(
  tenant: key(int),
  id: key(int),
  name: text
).
```

There is no positional declaration suffix such as `key(1, 2)`. Applying
`key(...)` to `Owner` and `Name` columns declares the natural identity of the
compiler relation itself.

### 1.4 Applications

A constructor is a relation. Applying it uses ordinary relation application
semantics. Arrow syntax names return columns of the same relation:

```dl6
rel X(x: type) -> type.
```

is structurally equivalent to:

```dl6
rel X(x: type, return: type).
```

No separate fundamental `type.application` object has been approved. The
current compiler's `application(ConstructorTypeId, Arguments)` term remains a
carrier until `@relation-application-semantics` rules how type-position calls
select return facades.

### 1.5 Annotations and roles

Annotations are ordinary relation applications. An application such as
`key(int)` is a type node and can be the target of `type.edge`. A relation-level
fact such as `serializable(User)` directly relates its annotation relation to
the `User` node. Current legacy `member_role(...)` rows are compact projections
consumed by existing code. There is no separate annotation-edge relation.

### 1.6 Target boundary

Type, integrity, flow, and materialization facts contain no SQLite-specific
semantics. Target namespaces appear only in emitter libraries and concrete
target plans.

## 2. Current implementation baseline

Implemented and merged before this plan:

```text
canonical type nodes and typed edges
legacy canonical logical type.member/5
type.project/3 derived in DL6
deep dotted paths and brace namespace prefixes
anonymous A.x and A.x.variant projection
functional type-pattern lowering
compiler scalar expressions and grouped count
stratified compiler negation
tabled compiler fixpoint
bounded generated-type refreeze
Partial, concat, extends, impl, serializable DL6 libraries
canonical storage projection and rel/5 compatibility reconstruction
clock dependency, ring, sign, grade, SCC, and boundary analysis in Prolog
```

Current compatibility carriers include:

```text
Semantic TypeId terms
legacy MemberId terms
application(Constructor, Arguments)
member_role(MemberId, Role)
keyed(RelationRef, Positions)
storage_relation/3
storage_column/2
storage_key/2
runtime rel/5 plans
```

The compatibility carriers are inventory targets for later retirement. Their
presence does not authorize equivalent user-land IDs.

## 3. Plane A: Type Graph

### 3.1 Primordial signatures

The complete structural graph substrate is:

```dl6
rel type.node(Node: key(type)).

rel type.edge(
  Owner: key(type),
  Name: key(text),
  Target: type,
  Index: int
).
```

All other `type.*` relations are derived indexes or algorithm results over
these facts and declarations. They do not add another graph element category.

The edge row is identified by its keyed `(Owner, Name)` columns. The model does
not introduce a synthetic `EdgeId`. Referring to an edge means referring to the
keyed `type.edge` row. The exact DL6 surface for an edge-valued argument remains
an open ruling.

Existing projection remains:

```dl6
rel type.project(
  Owner: key(type),
  Name: key(text),
  Target: type
).
```

### 3.2 Surface forms over the graph

Every concrete construct below is an authored or compiler-generated relation
node. A generic parameter is a compile-time placeholder that ranges over those
nodes. Primitives remain bootstrap leaf nodes.

| Surface form | Canonical graph pattern |
|---|---|
| Interface or product relation | Relation node with named outgoing field edges |
| Enum or sum relation | Relation node with outgoing edges to sibling variant relation nodes |
| Namespace relation | Relation/module node with outgoing naming edges to nested relation nodes |
| Generic parameter | Placeholder type node referenced from its declaring relation |
| Generic application | Application algorithm substitutes parameter nodes and interns a resulting relation node |
| Anonymous product | Compiler-generated product relation node reached through its owning edge |
| Anonymous sum | Compiler-generated sum relation node with sibling variant edges |

Product, sum, namespace, parameter, and edge-role classifications are ordinary
relational facts or typed edge relations over the same graph. The choice
between those two encodings remains open. Zig makes a similar distinction in
reflection for struct, enum, union, and opaque while implementing namespaces
and generics through container and comptime behavior.

Anonymous changes how the compiler derives canonical identity. It does not
change the node-edge shape:

```text
A --x--> anonymous product --a--> int
                         \--b--> text

A --x--> anonymous sum --a--> A.x.a
                     \--b--> A.x.b
```

### 3.3 Lowering

```dl6
type.project(Owner, Name, Target) <-
  type.edge(Owner, Name, Target, _).
```

Compatibility pseudocode:

```text
for each canonical legacy member carrier:
  emit type.edge(Owner, Name, Target, Index)

keep legacy MemberId internally while references remain
remove MemberId from the public graph relation
retire the carrier only after reference counts reach zero
```

### 3.4 Lifetime

Authored and generated declared relations enter the immutable compiler type
graph. Undeclared runtime IDBs remain outside unless the separate backlog card
`@inferred-idb-type-reflection` receives a later ruling.

### 3.5 Uniqueness

```text
(Owner, Name) -> (Target, Index)
(Owner, Index) -> Name
```

The first dependency is the natural edge identity. The second prevents two
edges from occupying one authored position.

### 3.6 Type graph algorithms

```text
projection/path traversal  follow named edges
application                resolve a declared relation plus arguments to output nodes
unification                equate node and edge patterns under substitutions
annotation lookup          match relation applications used as edge targets or facts over nodes
reachability               compute transitive paths and recursive containment
fixpoint closure           repeat derived graph rules until no new facts appear
canonicalization           intern equal semantic nodes and edge identities
```

These algorithms consume and derive relations. The structural graph remains
`type.node` plus `type.edge`.

## 4. Plane B: Integrity Graph

Integrity facts describe valid snapshots of a relation. They use database
theory concepts and do not name a SQL implementation.

### 4.1 Required signatures

```dl6
rel integrity.key(
  Owner: key(type),
  Name: key(text),
  Ordinal: int
).

rel integrity.unique(
  Owner: key(type),
  Group: key(text),
  Name: key(text),
  Ordinal: int
).

rel integrity.reference(
  Owner: key(type),
  Group: key(text),
  Name: key(text),
  TargetOwner: type,
  TargetName: text,
  Ordinal: int
).
```

`integrity.check` remains deferred until predicate values or quoted rule
expressions have a held relational representation.

### 4.2 First lowering

All edges targeting `key(T)` nodes on one owner form the default ordered identity:

```dl6
integrity.key(Owner, Name, Index) <-
  type.edge(Owner, Name, key(_), Index).
```

Pseudocode:

```text
collect edges targeting key applications by Owner
order edges by authored Index
validate one edge per ordinal and one target per Owner/Name
emit integrity.key rows
```

### 4.3 Timeline

Integrity derivation runs after type and annotation rows freeze and before flow
analysis or materialization. Invalid groups stop compilation before any target
plan is emitted.

### 4.4 Uniqueness and diagnostics

```text
key edge identity       (Owner, Name)
unique edge identity    (Owner, Group, Name)
reference edge identity (Owner, Group, Name)
```

Required diagnostics cover empty groups, duplicate ordinals, duplicate names,
conflicting target tuples, cross-owner group edges, and more than one default
identity definition.

Physical indexes are outside the Integrity Graph. An index changes access cost
while accepting the same relation contents.

## 5. Plane C: Flow Graph

Flow facts describe how relation snapshots evolve across the in-tick fixpoint,
tick boundaries, occurrences, deltas, and delayed effects.

### 5.1 Existing mathematical vocabulary

The current TICK-MODEL already uses:

```text
B  Boolean state/set relation
N  occurrence/log counting relation
Z  signed delta relation
```

Rule dependencies carry read ring, write ring, sign, grade, and role.

### 5.2 Required signatures

```dl6
rel flow.dependency(
  Rule: key(rule),
  From: key(type),
  To: key(type),
  ReadRing: ring,
  WriteRing: ring,
  Sign: sign,
  Grade: int,
  Role: dependency_role
).

rel flow.clock(
  Relation: key(type),
  Origin: key(type),
  Offset: int
).

rel flow.boundary(
  Rule: key(rule),
  Property: key(flow_property),
  Status: proof_status
).
```

Candidate semantic relations requiring a ruling:

```dl6
rel flow.state(Owner: key(type)).
rel flow.event(Owner: key(type)).
rel flow.time(Owner: key(type), Name: key(text)).
rel flow.sequence(Owner: key(type), Name: key(text)).
rel flow.history(Owner: key(type), Representation: history_representation).
```

### 5.3 Clock proof

The current `clock_dependency/8`, `inferred_clock/4`, `clock_fact/5`,
`clock_scc/3`, `clock_boundary/2`, and `clock_violation/2` terms provide the
implementation baseline.

Pseudocode:

```text
project expanded rule dependencies as flow.dependency rows
sum grades along causal paths
classify SCCs as constructive, productive delayed, or invalid
emit proof and boundary rows
apply only the refusal policy explicitly enabled by a ruling
```

### 5.4 Cardinality and determinism

The CSP lab requires a second analysis dimension:

```dl6
rel flow.cardinality(Relation: key(type), Class: cardinality_class).
rel flow.consumption(Rule: key(rule), Class: consumption_class).
rel flow.capacity(Resource: key(type), Count: int).
rel flow.batch_invariance(Rule: key(rule), Status: proof_status).
```

Candidate cardinality classes are `det`, `semidet`, `multi`, and `nondet`,
using Mercury's solution-count vocabulary. Their exact relation definitions and
composition table remain a Large ruling task.

### 5.5 Lifetime

Flow analysis consumes an expanded logical program and frozen integrity facts.
It creates no runtime table. Proof rows either authorize materialization,
produce a named boundary, or produce a named refusal.

## 6. Plane D: Materialization Graph

The Materialization Graph is a target-independent physical artifact plan. It
states what state must exist so an accepted flow can execute.

### 6.1 Required signatures

```dl6
rel storage.relation(
  Owner: key(type),
  Role: key(storage_role)
).

rel storage.column(
  Owner: key(type),
  Role: key(storage_role),
  Name: key(text),
  Target: type,
  Index: int
).

rel storage.lookup(
  Owner: key(type),
  Role: key(storage_role),
  Group: key(text),
  Name: key(text),
  Ordinal: int
).

rel storage.retention(
  Owner: key(type),
  Role: key(storage_role),
  Policy: retention_policy
).

rel storage.intern(
  Owner: key(type),
  Name: key(text)
).
```

Candidate artifact roles include current state, occurrence history, delta
frontier, retained history, dictionary, and refcount. These must become rel
enums or relation types before implementation.

### 6.2 Lowering from flow

```text
B state                  -> current-state artifact
N occurrence stream      -> occurrence-log or retained-trace artifact
Z signed deltas          -> delta-frontier artifact
pre or grade -1 read     -> previous-boundary access
grade +1 edge            -> carry artifact
productive delayed SCC   -> retained cycle state
identity lookup          -> lookup requirement
history representation   -> history artifact columns
retention annotation     -> storage.retention
```

Pseudocode:

```text
read accepted integrity and flow facts
derive required artifacts by semantic role
derive their semantic columns from type edges
validate artifact natural keys and target capabilities
hand the target-independent graph to one emitter
```

### 6.3 Lifetime and storage

Materialization rows exist during target planning and may be serialized into a
portable program plan. Runtime storage contains the artifacts they describe,
not the compiler rows themselves.

### 6.4 Uniqueness

```text
artifact identity (Owner, Role)
artifact column   (Owner, Role, Name)
lookup edge       (Owner, Role, Group, Name)
```

The plan must distinguish required artifacts from optimization hints. The
unsupported-target behavior remains an open ruling.

## 7. Target Plan

Target emitters consume type, integrity, accepted flow, and materialization
facts. Core compiler relations contain no backend name.

```text
SQLite emitter
  integrity key/unique/reference -> DDL enforcement
  storage lookup                 -> B-tree or available access path
  flow B/N/Z                     -> current, occurrence, and delta tables

DBSP emitter
  integrity identity             -> keyed arrangement where required
  flow B/N/Z                     -> collection, integration, differentiation
  delayed flow                   -> retained trace

memory emitter
  storage current                -> map or set
  storage occurrence             -> vector or ring buffer
  storage lookup                 -> hash or ordered index
```

Exact physical names, quoting, concrete SQL, ABI, and target capability checks
belong to the target plan.

## 8. CSP as a user-land protocol library

CSP constructs lower to ordinary relation families and rules:

```text
input
pending
cursor
ready
taken
output
```

The library consumes Flow Graph semantics and emits no separate channel node.
The current CSP lab established that eight of nine idioms are expressible, and
identified four checker requirements:

```text
W1 exactly-once worker consumption
W2 within-tick capacity safety
W3 aggregate-empty cardinality
W4 derived-trigger cross-engine timing
```

`@flow-cardinality-proof` owns W1 through W3. `@clock-flow-projection` and the
runtime parity cards own W4. `@csp-protocol-library` starts after those proofs
have explicit contracts.

## 9. Occurrence versus retained record

The existing TICK-MODEL distinction is preserved:

```text
occurrence happened       Flow Graph fact
record remains retained   Materialization Graph fact
```

An occurrence cannot be removed from causal history. A retention policy may
reclaim its stored record and currently emits a visible minus delta at the
boundary. Whether that storage reclamation should remain observable as an
ordinary flow minus is an explicit open ruling.

## 10. Undiscussed or unresolved contracts

Implementation must pause at these decisions:

1. **Node classification.** Decide whether product, sum, namespace, generic
   parameter, and generated-node classifications are ordinary facts or rel
   types implementing a common node contract.
2. **Edge classification.** Decide whether field, variant, contains, parameter,
   return, and annotation are ordinary facts about `type.edge` rows or distinct
   rel types implementing a common edge contract.
3. **Edge references and annotations.** Define DL6 syntax and typing for passing
   a keyed edge row to an ordinary relation such as `deprecated(Edge)`.
4. **Anonymous identity.** Define whether an anonymous node is keyed by its
   owning edge, source identity, canonical structure, or a stated combination.
5. **Return-facade lookup.** Define what `X(User)` selects when `X` has zero,
   one, or several outputs. Define whether partial application exists.
6. **Carrier retirement.** Decide whether the public relational view is enough
   while `TypeId`, legacy `MemberId`, and application terms remain internal, or whether
   the carriers themselves must disappear.
7. **Edge stability.** `(Owner, Name)` makes reorder stable and rename an
   identity change. Confirm that contract and migration from current
   `(Owner, Position, Name)` IDs.
8. **Identity reuse across planes.** Decide whether `integrity.key` always
   supplies the state replacement key, or whether Flow Graph identity may
   differ from snapshot identity.
9. **Integrity annotation surface.** Define named unique, reference, and later
   check applications using ordinary relation syntax.
10. **State/event surface.** Reconcile authored `flow.state/event/history` facts
   with the held `rel(0)`, `rel(1)`, and `rel` retention ruling, which currently
   says there are no separate state/event kinds.
11. **History representation.** Define whole-state, delta, and causal-event
   history and which edges are authored versus generated.
12. **Retention observability.** Decide whether reclaiming a stored record emits
   a flow minus or a separate materialization event.
13. **Proof enforcement.** Decide which clock and cardinality results are
   refusals, queryable boundaries, or target capability requirements.
14. **Per-row consumption.** Define the semantic operation that makes worker
   pools and capacities safe inside one tick without placing consuming reads
   inside the IDB fixpoint.
15. **Materialization requirement versus hint.** Define target behavior when a
   requested lookup, retention, or artifact cannot be represented exactly.
16. **Artifact roles.** Define the closed rel-enum vocabulary and whether a
   target may introduce private companion roles.
17. **Generic backend contract.** Define the minimum capabilities an emitter
   advertises before it accepts an integrity, flow, or storage fact.

## 11. Reconciled task graph

```text
completed Type Graph foundation
        |
        +--> type-edge-view [M]
        |
        +--> relation-application-semantics [L ruling]
        |
        +--> semantic-plane-rulings [L]
                  |
                  +--> userland-integrity-graph [M]
                  |         |
                  |         +--> sqlite-integrity-emitter [S]
                  |
                  +--> userland-flow-graph [L]
                  |         |
                  |         +--> clock-flow-projection [M]
                  |                    |
                  |                    +--> flow-cardinality-proof [L]
                  |
                  +--> userland-materialization-graph [L]
                            |
                            +--> quoted-sqlite-storage-names [M]

integrity + flow + materialization
        |
        +--> userland-temporal-annotations [M]
        |         |
        |         +--> remove-temporal-suffix [S]
        |
        +--> csp-protocol-library [M]
        |
        +--> retire-type-specialcases [M]
                  |
                  +--> relational-semantic-planes-golden [S]
```

## 12. Implementation order

1. Record and resolve the open rulings without compiler mutation.
2. Expose the primordial type-edge relation while retaining compatibility carriers.
3. Resolve relation application and return-facade semantics.
4. Implement Integrity Graph key parity first, followed by named unique and
   references.
5. Project existing clock facts into Flow Graph relations without changing
   refusal policy.
6. Add cardinality and batch-invariance proofs against the historical CSP
   receipts.
7. Derive the Materialization Graph from accepted integrity and flow facts.
8. Split integrity enforcement from lookup/index materialization in emitters.
9. Rebuild temporal annotations over Flow and Materialization Graph rows.
10. Implement CSP protocol libraries after exactly-once and capacity proofs.
11. Retire host special cases only after parity and reference-count receipts.
12. Run one cross-target golden over every plane and compiler-row erasure.

## 13. Verification

Each implementation card records:

```text
focused compiler relation tests
named invalid-program diagnostics
complete Prolog suite
runtime tick-log equality where flow changes
SQLite executable DDL where integrity or materialization changes
Rust and TypeScript artifact equality where target plans change
ProgramJson and schema snapshots
compiler-only relation erasure
```

TypeScript tests remain outside the current constraint work unless a later card
explicitly owns TypeScript target behavior.
