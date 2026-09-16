# fix/decl-order-msort FINISH: emit typing for the GENERICS section

## Where you are
Branch tip a87bf345 (WIP, banked): fix A is DONE and gate-green through
`just golden-flex` (msort removed from 0_generic_expand.pl, permutation
test rewritten, GENERICS section live in golden-flex.dl6, fixture
0_decl_order.pl added, repo-root FAILURE-REPORT.md deleted). Read
FAILURE-REPORT-DECL-ORDER.md in the worktree root: the ONLY blocker is a
typecheck failure in the GENERATED golden-flex.ts.

## The blocker (verbatim from the report)
```
gen_emitted/golden-flex.ts(3531,3): error TS2322: Type 'Observable<unknown>'
is not assignable to type 'Observable<ITickDeltas>'.
```
`run_naive_tick`'s `apply_arrivals` call loses its type parameter with the
new GENERICS section present. Diagnose in the EMITTER: find where the
compile pipeline prints run_naive_tick / apply_arrivals calls (grep
"apply_arrivals" in v6/prolog/*.pl and v6/tsv2/runtime/) and why the new
list-constructor decls make the emitted expression widen to unknown (likely
an emitted table-name union or generic parameter list that goes empty or
non-literal for minted instance rels). The fix belongs in the emitter's
printed code shape or the runtime signature it calls, whichever is the
smaller HONEST fix; a cast (`as Observable<ITickDeltas>`) in generated
output is acceptable ONLY if the runtime signature genuinely cannot carry
the type, and then the emitter comment states why in one line.

## Deliverables
1. typecheck green on the branch with the GENERICS section intact.
2. Then delete FAILURE-REPORT-DECL-ORDER.md and finish the original arc's
   leftovers if any remain (rulings.pl decision row for fix A: check
   whether the WIP already added it; add if absent).

## Files you own
- the emitter seam you diagnose (v6/prolog compile emission or
  v6/tsv2/runtime type signatures; name the file in your commit)
- golden-flex regenerated outputs, FAILURE-REPORT-DECL-ORDER.md deletion,
  v6/prolog/conformance/rulings.pl row
Everything committed at a87bf345 is yours to keep, avoid rewriting it.

## Setup (REQUIRED; absolute cd each command)
```bash
cd <worktree>/v6/tsv2 && pnpm install
cd <worktree>/v6/sprefa-store/js && pnpm install
cd <worktree>/v6/sprefa-extract && cargo build --release --features cli --bin extract
```

## Gate (all green, no exceptions; main is fully green)
```bash
cd <worktree>/v6 && just conformance && just plunit && just text-door && just roundtrip && just golden-flex
cd <worktree>/v6/tsv2 && bash scripts/sweep.sh
git checkout -- v6/prolog/compile/out/pokeapi_shape.ts
cd <worktree>/v6 && just typecheck && just tsv2-test
```

## Rails
- NEVER git merge / pull / rebase in the worktree.
- Blocked -> update FAILURE-REPORT-DECL-ORDER.md, exit NONZERO. rc=0 with
  red gates or a dirty tree is a defect.
- NEVER --no-verify. Comment budget: max 2 consecutive comment lines.
- Up to 2 commits, prefix `prolog:` (or `tsv2:` if the fix lands runtime-side).

## Style
Comments state only constraints the code cannot show. Banned words, prose
and identifiers: provenance, substrate, load-bearing, regime, refusal.
