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

# User-land type graph and compiler semantics

## Goal

Move compile-time type, projection, constraint, temporal, and storage-name semantics into ordinary DL6 relations and rules over the canonical type graph. Keep the host kernel limited to parsing, safe fixpoint evaluation, canonical identity interning, diagnostics, and mechanical target emission.

## Decisions Already Held

- Brace blocks contribute name prefixes only. Parent references are explicit typed columns.
- Dots address semantic paths and projections in every reference-bearing position.
- Functional terms may appear in compiler rule heads when they lower to explicit Datalog operations.
- Surface syntax uses calls, parentheses, colons, commas, arrows, and dots. The old `is` form is gone. Space-delimited `log keep` is removed after call-form parity.
- Canonical `type.member/5` rows expose semantic targets. Physical storage rows remain separate target-plan data keyed by member identity.
- Higher-kinded type flow is outside this epic.
- Any implementation choice outside this contract stops and asks Chris before mutation.

## Before and After

| Area | Before | After | Owning card |
|---|---|---|---|
| Integration | Temporal, projection, and generic compiler changes share one dirty worktree. | Every hunk has keep, move, replace, or discard ownership and a merge order. | @typegraph-integration-plan, @temporal-v2-salvage |
| Type graph | Compiler relations expose specialized canonical facts without one stable user-land node/edge view. | DL6 can query canonical nodes and typed edges during compiler refreeze. | @typegraph-node-edge-view |
| Members | Authored and rewritten storage members shared one plane-bearing compiler view. | `type.member/5` exposes one canonical semantic member; physical storage projection stays target-specific and keyed by member identity. | @typegraph-member-planes, @remove-type-member-plane |
| Type terms | Functional heads construct canonical applications; structural type-pattern matching is absent. | Construction and matching lower to safe explicit Datalog operations. | @type-pattern-lowering |
| Brace nesting | Argument-bearing brace parents can inject parent capture and shift child keys. | Braces contribute name prefixes; parent links are explicit typed columns. | @dot-brace-nesting |
| Dot projection | Projection lived in a PL experiment pass and compiler builtin source. | Imported DL6 declares keyed `type.project/3` and derives it from canonical graph edges. | @userland-dot-projection |
| Anonymous sums | A member-owned anonymous sum lacks the complete `A.x.variant` projection contract. | Owner, member, and variant paths resolve with deterministic collision rules. | @anonymous-sum-dot-projection |
| Keys and constraints | Key wrappers are interpreted through feature-specific compiler behavior. | Primary, unique, index, and foreign-key groups are first-class compiler rows. | @userland-constraint-graph, @sqlite-constraint-emitter |
| Temporal schema | Temporal requests use compiler builtins and retain the legacy suffix surface. | DL6 annotations derive storage rows; call syntax replaces the suffix after parity. | @userland-temporal-annotations, @remove-temporal-suffix |
| SQLite names | Backend names are constrained by flattened generated spellings. | Semantic IDs map to correctly quoted physical SQLite identifiers and companion names. | @quoted-sqlite-storage-names |
| Type operators | `Partial`, `serializable`, `extends`, `impl`, and `concat` lack a complete common user-land substrate. | The operators are DL6 libraries over canonical node, edge, member, application, and annotation rows. | @userland-type-operators |
| Host compiler | Superseded temporal, projection, and constraint special cases remain after replacement. | Reference-counted special cases are removed after parity. | @retire-type-specialcases |
| Cross-target proof | Receipts are distributed among focused experiments. | One final golden covers Prolog, SQLite, TypeScript, Rust, ProgramJson, and generated schemas. | @userland-typegraph-golden |

## Model Routing

| Tier | Marker | Execution |
|---|---|---|
| Small | `size:small`, `S` | Flash4 maximum thinking through a Boop OpenCode lane; completion hail required. |
| Medium | `size:med`, `M` | Native Terra-high with Boop communication and completion hail. |
| Large | `size:large`, `L` | Current Codex performs the work directly; architectural forks yield for user confirmation. |

Planning, reconnaissance, implementation, and closeout all receive a tier. Agent acknowledgement does not count as completion; artifacts and tests are the receipts.

## Task Graph

```text
typegraph-integration-plan [L]
  +-> temporal-v2-salvage [L] -> dot-brace-nesting [M]
  +-> typegraph-node-edge-view [M]
  |     +-> typegraph-member-planes [M]
  |     |     +-> userland-dot-projection [M]
  |     |     |     +-> anonymous-sum-dot-projection [M]
  |     |     +-> userland-constraint-graph [M]
  |     |           +-> sqlite-constraint-emitter [S]
  |     +-> type-pattern-lowering [L]
  |           +-> compiler-plane-expression-parity [L]
  |                 +-> userland-type-operators [M]
  +-> quoted-sqlite-storage-names [M]

temporal-v2-salvage + userland-constraint-graph
  -> userland-temporal-annotations [M]
  -> remove-temporal-suffix [S]

constraint + temporal + operators
  -> retire-type-specialcases [M]

all implementation leaves
  -> userland-typegraph-golden [S]
```

Existing blockers remain authoritative: `@canonical-type-reflection`, `@canonical-storage-projection`, and `@typed-annotation-corrections`.

## Issues

- [x] @typegraph-integration-plan
- [x] @temporal-v2-salvage
- [x] @typegraph-node-edge-view
- [x] @typegraph-member-planes
- [x] @remove-type-member-plane
- [x] @type-pattern-lowering
- [x] @compiler-plane-expression-parity
- [x] @dot-brace-nesting
- [x] @userland-dot-projection
- [x] @anonymous-sum-dot-projection
- [ ] @userland-constraint-graph
- [ ] @sqlite-constraint-emitter
- [ ] @userland-temporal-annotations
- [ ] @remove-temporal-suffix
- [ ] @quoted-sqlite-storage-names
- [x] @userland-type-operators
- [ ] @retire-type-specialcases
- [ ] @userland-typegraph-golden

## Acceptance Criteria

- [ ] Projection, constraint, temporal, and type-operator rows are derived by user-land DL6.
- [ ] Canonical node, edge, member, application, and annotation rows are queryable during the compiler fixpoint.
- [ ] Composite and alternate SQL constraints lower from first-class rows.
- [ ] SQLite storage names preserve approved punctuation through correct identifier quoting.
- [x] Anonymous member sums follow the approved dot projection model.
- [ ] Old temporal suffix syntax and obsolete compiler special cases are removed after parity.
- [ ] Cross-target gates cover Prolog, SQLite, TypeScript, Rust, ProgramJson, and generated schemas.

## Tests Run

Pending child-card completion.

## Implementation Notes

Related foundations: `@relational-type-schema`, `@applicative-type-annotations`, and `@compiler-derived-relation-construction`. Decision context: `@comptime-type-model` and `@type-plane-design`.

## Decisions

### 2026-08-25T13:11:35Z · @codex

Canonical type.member/5 exposes semantic member targets. Target-specific storage rows remain outside the compiler relation and join through member identity.
