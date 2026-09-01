# DL7 Reified Call Sites and Monomorphic Relational Emission

## Context

DL7 now freezes compiler-generated relation declarations before strict source
lowering. Named, punned, omitted, and nested partial calls erase to static
Datalog, but `v7/src/2_comptime/0_lowerer.pl` still owns their slot policy and
rejects an unfinished partial application escaping through a bind.

The compiler relation graph must support arbitrary curryable functional types. A
functional return is one ordinary `return` edge whose target may be another
callable node. Emitters query that graph as relations. An emitter may be
implemented as SWI-Prolog predicates today or as DL7 rules as the self-hosted
compiler surface grows.

The pipeline contains DL7 source, SWI-Prolog compilation, closed compiler
relations, and selected emitters. TypeSpec remains outside the pipeline; its
emitter ergonomics are only a comparison point.

## Decisions

1. Calls, supplied slots, and partial specializations become compiler facts.
   Prelude rules derive remaining slots, generated declarations, forwarding
   rules, defaults, and completion.
2. A partial specialization is an interned callable node. Its bound-slot edges
   are closure captures; its unbound callable edges form its public signature.
3. Classical currying binds the next positional edge. Named partial application
   binds any edge. Both lower through the same bound-slot relation.
4. One functional `return` edge may target a primitive, product, sum, or callable
   node. Full relations retain arbitrary arity and userland mode facts.
5. Compiler lowering and compiler evaluation repeat until callable declarations
   and source calls stabilize. The existing compiler-round limit bounds this
   process.
6. Every emitter reads a closed compiler relation graph. The relational
   application emitter produces fixed relation identities, fixed arities, and monomorphic
   Datalog suitable for DBSP, SQL, Rust, and TypeScript execution.
7. A finite row-bound callable set lowers through userland closure alternatives
   and static dispatch rules. Open runtime predicate lookup is outside the
   monomorphic Datalog contract.

Rejected alternatives:

- Runtime `apply`: leaves predicate identity dynamic in emitted programs.
- Backend-owned type specialization: duplicates type and closure semantics in
  every emitter.
- Prefix-only partial identity: prevents named partial application from sharing
  the same graph model.

## Graph and phase contract

```text
source syntax
    │
    ▼
compiler relation graph
    │
    ├── call_site(Call, Callable, Use, Source)
    ├── supplied_slot(Call, Index, Value, Kind)
    ├── partial(Partial, Callable)
    └── bound_slot(Partial, Index, Value, Kind)
             │
             ▼ userland compiler fixpoint
    effective slots and callable specializations
             │
             ▼
closed compiler relation graph
    │
    ├── DL7-authored emitters
    ├── SWI-Prolog emitters
    └── relational application emitter
             │
             ▼
       monomorphic Datalog
             │
             ├── DBSP
             ├── SQL
             ├── Rust
             └── TypeScript
```

The first vertical slice reifies an escaping partial bind, derives a callable
specialization and forwarding rule in the prelude, then calls that generated
relation from authored facts and rules. Nested completion continues to erase
directly during lowering while the carrier is generalized.

<!-- todo(refactor): Reify every call site and move named, punned, omitted, and defaulted slot policy from 0_lowerer.pl into prelude rules. -->
<!-- todo(feature): Add the compiler-relation emitter protocol and monomorphic Datalog application emitter boundary. -->

## Verification

- A unary partial bind returns a callable relation and captures its argument.
- A returned callable works in facts, rule heads, rule bodies, and expressions.
- Two and three stage currying chains resolve to one static relation call.
- Named partial slots produce the same specialization identity regardless of
  source argument order.
- Repeated compilation produces identical compiler rows and runtime IR.
- Runtime IR contains no call-site, partial, bound-slot, or dynamic predicate
  transport.
- The complete V7 SWI suite and tree-sitter corpus remain green.

## Staffing

- Implementation: Codex directly, high reasoning.
- Review: focused call-carrier review after the first vertical slice.
- Worktree: `/private/tmp/sprefa-v7-reified-calls`.
- Branch: `feature/v7-reified-call-sites`.
- Base SHA: `89080a587`.
- Suite budget: focused entrypoint tests after each commit; full V7 suite before
  every push.
