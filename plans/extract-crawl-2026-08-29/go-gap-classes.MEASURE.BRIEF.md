# Lane `chore-go-gap-classes` (glm53f, measure only): what the last 26.6% of go call edges are

Read `plans/extract-bench-2026-08-29/ORACLES.REPORT.md` section 12 and
`plans/extract-crawl-2026-08-29/go.REPORT.md` Fixes 5 and 6. After #563 our
go call edges reach 73.4% of `go/callgraph` vta (bare names). Classify the
vta-only remainder.

## First action
```
git merge --ff-only 29885363035af7d5f52d209e5f307f291c2fa6f9
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```

## Measure
`bench.py plans/extract-bench-2026-08-29/go.parse.call.tsv go.oracle.call.vta.bare.tsv`
(the tsv #563 re-emitted). Take the vta-only set; for a random 300-row
sample open each site in the corpus and classify: interface method dispatch
(receiver is an interface, vta names the implementer), method value /
func-typed field or param, generic instantiation, embedded-struct promoted
method, receiver from a multi-hop chain (`a.b().c()`), stdlib callee we
correctly skip, closure named by vta as its enclosing fn but we already
mirrored, other (say what). One table: class, count in sample, projected
count over the whole vta-only set, 2 file:line examples each, and which
existing leg (go.rs fn name) would take it. Also the ours-only set: same
treatment, 200 rows.

## Ownership
`plans/extract-crawl-2026-08-29/go.GAPS.md` and tsvs beside it. No `src/`.

## Receipt
Push `chore/go-gap-classes`, `gh pr create --base main`, hail
`boop beep --no-wait --as chore-go-gap-classes sprefa-coordinator "go gaps: PR #N, top class <name> <pct>, second <name> <pct>"`.
Laws: no em dashes, tables over prose, every extract call under timeout 10.
