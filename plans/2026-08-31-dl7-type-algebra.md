# DL7 userland type algebra

## Context

DL7 already represents named and anonymous products as type identities with
ordered `:/4` edges. Generic application is an ordinary relation whose final
`return` edge is projected in expression position. `Partial`, `Pick`,
`Exclude`, `Key`, and `HistoryV1` are prelude relations evaluated by the same
fixpoint machinery as authored compiler rules.

The remaining type-algebra slice needs no interface declaration syntax. A
contract is an ordinary product node. Conformance is a relation over a source
type and contract type. Intersection constructs another product node. An impl
witness is ordinary typed data that can be checked by the same conformance
relation. A generic constraint is a conformance goal in a generic rule body.

The current prelude is one 375-line file. This arc separates it by dependency
and reading order before adding more userland operators.

## Decisions

1. Product edges remain the only structural contract representation.
2. `Conforms(Source, Contract, Proof)` is an ordinary generic relation. Its
   proof identity is interned from the ordered arguments.
3. Conformance succeeds when every contract edge has a source edge with the
   same label and target. Extra source edges are accepted.
4. `missing_contract_edge/5` exposes failed proof data to compiler rules and
   tests. Failure remains absence of `Conforms/3`.
5. `ConformsAll(Source, Contracts, Proof)` recursively checks a closed `cons`
   list. This is the initial interface-intersection operation.
6. `Intersect(Left, Right, Result)` constructs one canonical product identity,
   emits left edges followed by right-only edges, and deduplicates equal
   label-target pairs.
7. Equal labels with unequal targets derive `intersection_conflict/3` and
   prevent construction of the result edges.
8. `implements(Contract, Source, Witness)` is authored evidence.
   `valid_impl/3` derives when the witness conforms to the contract.
9. Relation-valued edges need no new representation. Their targets are the
   same type identities used by primitive-valued and product-valued edges.
10. Generic constraints are ordinary body goals. No generic-only checker or
    declaration form is added.
11. `HistoryV1` options gain a contract edge. History construction requires a
    conformance proof before generating relation and rule rows.
12. The expression-flow implementation from `feature/dl7-count-aggregate`
    remains the branch base and enters the same PR.
13. Mercury-style determinism categories remain outside this arc.

## Relation signatures

```text
Conforms(Source: type, Contract: type, return: type)
missing_contract_edge(Source: type, Contract: type,
                      Name: any, Target: any, Index: int)

ConformsAll(Source: type, Contracts: any, return: type)

Intersect(Left: type, Right: type, return: type)
intersection_conflict(Left: type, Right: type, Name: any)

implements(Contract: type, Source: type, Witness: type)
valid_impl(Contract: type, Source: type, Witness: type)
invalid_impl_edge(Contract: type, Source: type, Witness: type,
                  Name: any, Target: any)
```

Rule bodies use the same relations:

```text
NamedBox(Source, Result) <-
    Conforms(Source, Named, Proof),
    intern(NamedBox, [Source], Result).
```

## Instance timelines

### Structural conformance

```text
(Conforms User Named)
    -> intern Conforms+[User, Named]
    -> frozen application identity
    -> inspect frozen User and Named edges
    -> derive zero or more missing_contract_edge rows
    -> derive Conforms only when no edge is missing
    -> enclosing expression receives the proof identity
```

### Intersection

```text
(Intersect Left Right)
    -> intern Intersect+[Left, Right]
    -> frozen application identity
    -> compare labels and targets
    -> conflict rows or one ordered merged edge set
    -> node/product facts for the result identity
```

### HistoryV1

```text
(HistoryV1 Source Options)
    -> read Options.contract
    -> Conforms(Source, Contract, Proof)
    -> intern HistoryV1+[Source, Options]
    -> copy type edges
    -> emit def/head/body rows
    -> assemble checked runtime relation and rule
```

## Storage, reads, writes, and uniqueness

- Type nodes are canonical semantic identities already emitted as `ref(...)`.
- Product edges are `:/4` rows keyed by owner plus label and owner plus index.
- Application identities keep the functional dependency
  `(Constructor, ordered Arguments) -> Result`.
- Conformance proofs keep
  `(Source, Contract) -> Proof` through the same intern relation.
- Conformance reads frozen `edge_snapshot/4` rows and writes proof and failure
  facts in compiler closure.
- Intersections read two frozen edge sets and write one canonical frozen edge
  set in a later compiler round.
- Impl evidence is authored compiler data. Validation writes derived proof or
  failure rows and does not mutate the witness.
- History generation reads the validated proof and writes generated-program
  carrier rows once its canonical application is frozen.

## Milestones

1. Load a lexically ordered prelude file set and split the existing prelude.
2. Add canonical `Conforms` application and reverse lookup rules.
3. Add structural edge matching, missing-edge rows, and successful proofs.
4. Add `ConformsAll` over closed `cons` lists.
5. Prove relation-valued contract edges.
6. Prove a generic body constrained by `Conforms`.
7. Add canonical `Intersect` application and ordered edge candidates.
8. Add deduplication, dense result ordering, and conflict rows.
9. Add explicit impl evidence, valid proofs, and invalid-edge rows.
10. Require a contract in `HistoryV1` options.
11. Add one userland composition operator implemented through intersection.
12. Consolidate exact fixture and PLUnit receipts.
13. Record the type-algebra surface and compiler-round behavior in V7 docs.

## Verification

- Parse every ordered prelude file with Tree-sitter.
- Run focused PLUnit cases after each relation cluster.
- Assert exact proof identities, missing-edge rows, merged edge order,
  conflict rows, impl rows, and generated HistoryV1 behavior.
- Run the complete V7 SWI suite and Tree-sitter corpus before the final push.
- Report only current build and test results.

## Staffing

- Branch: `feature/dl7-type-algebra`.
- Worktree: `.boop-worktrees/feature/dl7-type-algebra`.
- Luna may perform the isolated ordered-prelude split after this plan commit.
- A second lane may review relation safety, stratification, and refreeze depth.
- The coordinating high-reasoning lane owns conformance, intersection,
  HistoryV1 integration, final review, PR, and merge.
- Each implementation cluster receives its own commit.

