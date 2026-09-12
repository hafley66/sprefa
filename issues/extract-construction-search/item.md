---
created: 2026-09-12
updated: 2026-09-12
type: feature
status: open
priority: normal
related: ['@extract-semantic-fact-roundtrip', '@extract-dependency-corpus']
labels:
- area:extract
---

# sprefa-extract: typed construction search requirements and coverage gaps

## Description

## Consumer request

Expose enough trustworthy facts to ask, over indexed Rust APIs:

```text
next(MyGame): which callables accept this type, under which obligations?
paths(MyGame, PlayableGame, depth=3): which construction expressions reach it?
```

The caller needs a candidate expression, type substitutions, required additional
values/impls, construction-hop count, unresolved obligations, and source witnesses.
This records analysis and requirements, not authorization to change compiler semantics.

## Existing substrate and evidence

- `v6/sprefa-extract/src/tsi/registry.rs` already describes callable inputs/outputs,
  generic parameters/applications, trait implementations, associated types,
  ownership and lifetimes. Reuse this vocabulary.
- `docs/extract-tsi.md` documents run/fact/witness/coverage and reverse ingestion.
  @extract-semantic-fact-roundtrip records landed acceptance receipts. Its original
  missing-feature report is historical, not the current capability inventory.
- `v7/README.md` documents Extract -> DL7 joins/fixpoints -> SQLite SQL/IVM or
  Rust runtime plans, and Rust type/constructor emission through Soopy.
- V8 is active in another worktree. The README read at
  `/private/tmp/sprefa-v8-725/v8/README.md` describes the Rust DL7 compiler port,
  including TSI loaders, checking, comptime, reification and semi-naive evaluation.
  Main's missing `v8/` directory is not evidence that V8 is absent from the project.
  V8 parity statements here are README claims; no V8 tests were run for this report.
- `v6/README.md` explicitly identifies itself as partially stale.

Prior local probe (2026-09-12): installed Extract build `8aba008df2f1`,
`extract --witness --family type ../hafley-games/crates/rollback/src/lib.rs`.
Observed 18 tsi.callable, 25 tsi.input, 12 tsi.output, 13 tsi.parameter,
8 tsi.called, 8 tsi.argument, 3 rust.impl, 3 tsi.conforms facts.
Coverage emitted was partial; diagnostics were empty. This was syntax extraction,
not a native-checker proof or a constructor-search test. The donor can change;
these counts are a historical receipt, not an acceptance baseline.

## Missing from the consumer workflow, implementation status unverified

1. A documented bounded query/example returning constructor paths and missing
   prerequisites from existing TSI facts. First locate equivalent DL7/V8 work.
2. Shared generic substitution across all inputs, outputs and trait/associated-type
   obligations. Reuse existing unification if applicable; identify unsupported Rust
   obligations explicitly. Term unification alone does not discharge Rust bounds.
3. Receiver and ownership-aware applicability: self, &self, &mut self, moves,
   reborrows, lifetimes, visibility, cfg/features, and scope of imported APIs.
4. On-demand native-checker validation for proposed expressions, with diagnostics
   attached to candidates. Static enumeration must not imply compiler acceptance.
5. Compact machine-readable results with witness paths and coverage reasons, so
   agents can distinguish compiler-checked, conditional, unresolved, and rejected.
6. Search limits: indexed corpus, maximum depth/results, cycles, cancellation and
   truncation indication. An exhausted partial corpus cannot prove unreachable.

Observed adapter cautions from the earlier source read:
`src/lang/rust_type_edges.rs::tsi_callable` skips receiver self in syntax mode.
`src/lang/rust_checker_ra.rs` reports sampled conforms rather than enumerating
blanket/auto traits, and does not enumerate all assignability/subtyping relations.
Recheck current branch before treating these as current defects. A represented
relation and complete coverage of that relation are distinct acceptance gates.

## Concrete semantics

```rust
fn open(endpoint: Endpoint) -> Result<Socket, Error>;
fn connect<S: Slice>(game: Game<S>, socket: Socket, party: Party) -> Online<S>;
```

Given `Game<Lovers>`, reaching `Online<Lovers>` requires Socket and Party plus
the `Lovers: Slice` obligation. Using open adds a callable application and an
explicit Result-handling obligation. S=Lovers must remain consistent everywhere.
Two required inputs are an AND prerequisite set; alternative constructors are OR
choices. Count callable applications separately from code/impl work remaining.
This construction relation does not assert the SQL functional dependency
`(game,socket,party) -> online`, purity, or unique runtime results.

Suggested output content, not a new approved schema: target, expression tree,
substitution, required values, obligations, callable hops, verification status,
witnesses, corpus/build identity, truncation reason. Let existing TSI/DL7 contracts
determine its concrete representation. Keep search policy in a consumer library
where existing facilities permit; avoid duplicating extraction or compiler engines.

## Acceptance Criteria

- [ ] Inventory existing V7/V8 search/unification facilities and classify each
      requested capability as present, integration-needed, or unsupported.
- [ ] Add a minimal indexed Rust fixture for generic constructor composition;
      exact expected substitutions, prerequisites and hop counts are asserted.
- [ ] Reject inconsistent generic bindings and reuse of a moved non-Copy value;
      preserve Result handling and associated-type obligations in results.
- [ ] Distinguish missing facts under partial coverage from proven rejection;
      scoped compiler checks provide reproducible candidate receipts.
- [ ] Expose a bounded CLI/query example with deterministic machine output and
      explicit truncation, without requiring agents to read implementation source.

## Tests Run

Read-only help/source/README inspection and the syntax probe above. No new
implementation, compiler validation, or test suite execution in this filing.

## Implementation Notes

Related @extract-semantic-fact-roundtrip and @extract-dependency-corpus.
Call-path reachability in @extract-reach-entry-sink answers a different query
from constructor applicability; reuse facts without conflating the two results.
Follow-up ownership belongs to sprefa-extract for factual coverage and the existing
DL7/V8 consumer for construction search. Kernel semantic changes still require
the repository's explicit user-participation gate.
