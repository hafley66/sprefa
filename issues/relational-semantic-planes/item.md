---
created: 2026-08-24
updated: 2026-08-25
type: epic
owner: chris
status: open
priority: high
related: ['@relational-type-schema', '@applicative-type-annotations', '@compiler-derived-relation-construction', '@comptime-type-model', '@type-plane-design']
labels:
- area:dl6
- intent:type-system
- size:large
- model:large
- task-graph
size: L
---

# Relational semantic planes

## Goal

Expose compile-time semantics as ordinary DL6 relations over canonical IDs. Keep target names and target capabilities inside target plans and emitters.

The reconciled contract is recorded in `plans/2026-08-25-relational-semantic-planes.md`.

## Held Model

```text
Type Graph             Integrity Graph       Flow Graph
type.*                 integrity.*           flow.*
nodes and typed edges  keys/references/check occurrence, sign, delay,
                                              retention, replacement
         \                  |                 /
          \                 |                /
           +------ Materialization Graph ----+
                    storage.*
                    tables, columns, keys,
                    retained arrangements
                              |
                              v
                         Target Plan
                    target-specific names,
                    capabilities, artifacts
```

- A type node is a relation or primitive.
- The Type Graph consists of relation or primitive nodes and primordial named edges `(Owner, Name, Target, Index)`.
- Interfaces, enums, namespaces, generic applications, and anonymous products or sums are relation nodes with different relational classifiers and lowering algorithms.
- Generic parameters are compile-time placeholder nodes ranging over relation or primitive nodes.
- Projection, application, unification, annotation lookup, reachability, and fixpoint closure are algorithms over that graph.
- `key` is an ordinary relation applied to a target type.
- Relation application uses the declared relation signature and return facade.
- A relation occurrence and a retained record are separate concepts.
- The clock checker consumes Flow Graph facts.
- CSP protocols are DL6 libraries over Flow Graph facts and proven cardinalities.

## Task Graph

```text
completed type foundations
  +-> semantic-plane-rulings [L]
  +-> relation-application-semantics [L]
  +-> type-edge-view [M]

rulings + application + edge view
  +-> userland-integrity-graph [M]
  +-> userland-flow-graph [L]
        +-> clock-flow-projection [M]
              +-> flow-cardinality-proof [L]

integrity + flow + canonical storage projection
  +-> userland-materialization-graph [L]
        +-> quoted-sqlite-storage-names [M]

integrity + flow + materialization
  +-> userland-temporal-annotations [M]
  |     +-> remove-temporal-suffix [S]
  +-> csp-protocol-library [M]
  +-> retire-type-specialcases [M]
        +-> relational-semantic-planes-golden [S]

integrity
  +-> sqlite-integrity-emitter [S]
```

## Issues

- [ ] @semantic-plane-rulings
- [ ] @relation-application-semantics
- [ ] @type-edge-view
- [ ] @userland-integrity-graph
- [ ] @sqlite-integrity-emitter
- [ ] @userland-flow-graph
- [ ] @clock-flow-projection
- [ ] @flow-cardinality-proof
- [ ] @userland-materialization-graph
- [ ] @userland-temporal-annotations
- [ ] @remove-temporal-suffix
- [ ] @quoted-sqlite-storage-names
- [ ] @csp-protocol-library
- [ ] @retire-type-specialcases
- [ ] @relational-semantic-planes-golden

## Acceptance Criteria

- [ ] The four semantic planes have stable user-land signatures and uniqueness rules.
- [ ] Relation application semantics cover zero, one, and many outputs.
- [ ] Integrity facts lower independently of any target emitter.
- [ ] Flow facts feed the clock checker without backend vocabulary.
- [ ] Materialization facts distinguish logical identity from physical layout.
- [ ] Target plans own target names, capabilities, and artifact roles.
- [ ] Temporal and CSP libraries use the same user-land relations.
- [ ] Replaced host special cases are removed after parity receipts.

## Tests Run

Pending child-card completion.

## Implementation Notes

Execution tier: Large, size `L`, model `large`. Architectural forks remain on ruling cards and yield for user input before compiler mutation.
