---
created: 2026-08-25
updated: 2026-08-25
type: feature
assignee: codex
status: open
priority: high
epic: relational-semantic-planes
labels:
- area:dl6
- area:compiler
- intent:type-system
- size:large
- model:large
size: L
lane: semantic-planes
lane_seq: 20
collision: [generic-type-core, compiler-oracle]
---

# Define ordinary relation application semantics

## Description

# Define ordinary relation application semantics

## Description

Specify how a declared relation is applied in type position. Construction uses the declared inputs and return facade. No undeclared relation is inferred.

## Signatures

```dl6
rel X(Input: type) -> type.

// Equivalent relation shape
rel X(Input: type, Return: type).
```

## Cases

- Zero return members: application has no type result and receives a diagnostic where a type is required.
- One return member: `X(Arg)` resolves to that return target.
- Multiple return members: the application resolves to a declared relation-shaped return facade.
- Partial application: syntax, arity, and resulting identity require an explicit ruling before support.

## Lifetime

Applications are derived during compiler refreeze from ground constructor and argument IDs. Their identities remain stable for the same declaration and arguments.

## Storage And Uniqueness

The logical key is the declared relation identity plus ordered argument IDs. Existing structural `application(...)` terms remain compatibility carriers until migration is approved.

## Acceptance Criteria

- [ ] Zero, one, and many return-member behavior is specified and tested.
- [ ] Undeclared constructors fail instead of creating inferred declarations.
- [ ] Groundness and arity diagnostics are deterministic.
- [ ] Existing generic construction and head-term lowering retain parity.
- [ ] Partial application is either specified or diagnosed as unsupported.

## Tests Run

Compiler relation application tests, functional-head lowering tests, generic type tests, diagnostics snapshots.

## Implementation Notes

Execution tier: Large, size `L`, model `large`. Direct design and implementation because this changes canonical type identity semantics.
