# Lane `chore-ts-gap-classes` (glm53f, measure only): the 30% of ts call edges tsc has and we do not

Read `plans/extract-bench-2026-08-29/ORACLES.REPORT.md` sections 7 and 11,
`plans/extract-crawl-2026-08-29/ts5.REPORT.md` section 11 (PR #566), and
`plans/extract-crawl-2026-08-29/go.GAPS.md` (the shape of the deliverable).
After #566 our ts call edges reach 70.05% of the TypeChecker's on
~/projects/TypeScript-5.9/src; 7,567 sites are still `ambiguous`.

## First action
```
git merge --ff-only 7bfc8d4a4e20128374f341b02890ff0880163112
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```

## Measure
ONE process: `extract --resolve --project-root ~/projects/TypeScript-5.9 $(find src -name '*.ts' ! -name '*.d.ts')`;
normalize (`plans/extract-bench-2026-08-29/normalize.py`), overwrite
`ts5.parse.call.tsv`, `bench.py ts5.parse.call.tsv ts5.oracle.call.tsv`.
Classify 300 random oracle-only rows and 200 ours-only rows by opening the
site: union-typed receiver; generic receiver `T extends X`; receiver from a
call result more than one hop; interface receiver needing implementer
fan-out; `this` in a namespace / function-style module; callback / func-
typed param; method on a namespace-merged declaration (`ts.foo` through
`_namespaces`); overload resolution (tsc picks a signature, we bind the
declaration group); optional chaining `a?.b()`; other (say what). Also
classify the 7,567 `ambiguous` rows by the same buckets (all of them, by
script, not a sample, since the `unresolved` row carries the site). Table:
class, sample count, projected, 2 file:line each, which `ts_receivers.rs`
/ `ts.rs` fn would take it.

## Ownership
`plans/extract-crawl-2026-08-29/ts.GAPS.md`, tsvs beside it,
`plans/extract-bench-2026-08-29/ts5.parse.call.tsv`. No `src/`.

## Receipt
Push `chore/ts-gap-classes`, `gh pr create --base main`, hail
`boop beep --no-wait --as chore-ts-gap-classes sprefa-coordinator "ts gaps: PR #N, top class <name> <pct>, second <name> <pct>, recall now <r>"`.
Laws: no em dashes, tables over prose, every extract call under timeout 10.
