# FAILURE-REPORT-DECL-ORDER

## Blocker

The declaration-order gates through `just golden-flex` pass. The required
TypeScript gate fails in generated `golden-flex.ts`.

## Reproduce

```text
cd /Users/chrishafley/projects/sprefa/.boop-worktrees/fix/decl-order-msort/v6
just golden-flex && just typecheck && just tsv2-test
```

Output:

```text
GOLDEN FLEX HOLDS
gen_emitted/golden-flex.ts(3531,3): error TS2322: Type 'Observable<unknown>' is not assignable to type 'Observable<ITickDeltas>'.
error: recipe `typecheck` failed on line 122 with exit code 1
```

The failure is in generated `run_naive_tick`, where `apply_arrivals` returns
`Observable<unknown>` and the pipeline requires `Observable<ITickDeltas>`.
