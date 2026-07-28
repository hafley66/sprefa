# CODEX BRIEF: match block sugar (sol-class)

User go, 2026-07-29. Design record: match_block_word ruling (the block
word is match), the match-frontier lab's SUGAR-SCOPE slot, syntax rec
(6) never `=>` or `|` in term form. Surface:

```prolog
match resp_raw(Ep, Status, Tag, Body) (
    fetch_result_fresh(Ep, Tag, Body) <- Status == 200
  ; fetch_result_unchanged(Ep)        <- Status == 304
  ; fetch_result_error(Ep, Status)    <- Status >= 400
).
```

Arm separator = the decl semicolon (prolog disjunction, zero new
tokens); arm arrow = the existing `<-` (a `<+` arm is legal too and
keeps its edge semantics). The match head is one positive rel atom;
its bindings scope over every arm.

## The centralized move (the enum-arc pattern exactly)

Pure sugar. Desugar: each arm becomes the rule you would write by
hand, `ArmHead <- MatchAtom, ArmGuards.` (or `<+` respectively). Term
form RETAINS the match block (G1 round-trip exact; print_dl reproduces
the block). ONE shared expansion predicate beside expand_enum_program/2
in v6/prolog/0_enum_expand.pl or a sibling 0_*.pl (name it), consumed
by both the oracle engine and compile.pl. Zero new evaluator
constructs.

## The exhaustiveness check (the payoff beyond sugar)

When every arm head is a variant rel of ONE enum decl, the block must
cover ALL of that enum's variants; a missing variant is a named
refusal (match_nonexhaustive(Enum, MissingVariant) shape). Blocks
whose arm heads are not enum variants get no coverage obligation.
Stated rx lowering for the docs row: one shared source (a rel is
already share()d by construction) + one filter per arm; partition and
groupBy are explicitly NOT the story.

## Fixtures

(a) the three-arm classify block above, byte-identical to its
hand-written desugar (write both, grade both); (b) an edge-arm (`<+`)
inside a block; (c) the nonexhaustive refusal over an enum. Existing
fixture movement must be zero.

## Laws + summary shape

Worktree/branch/base given at launch; READ-ONLY git only, tree left
dirty, coordinator commits (standing sandbox limit). Registry rows +
generated SYNTAX.md section. Descriptive variables; no em dashes;
banned words provenance, substrate, load-bearing, regime. Full
conformance max 3 runs. Final summary: expansion predicate name and
home, fixture counts, sugar-vs-desugar byte-identity receipt, all
grades (conformance/roundtrip/sweep/plunit/tsv2/gate), per-fixture
movement, cracks.
