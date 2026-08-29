# Lane `chore-go-module-plane-receipt` (glm53f): corpus receipt for PR #558

Read `plans/extract-corpus-2026-08-28/COMMON.md` (10-second law, background
batteries) and `plans/extract-crawl-2026-08-29/go.REPORT.md` sections
"Fixes" and "Fixes 2" (they show the exact commands that produced
`all_resolved.jsonl` and `defs.tsv` for `go.crawl.py`).

## First action
```
git merge --ff-only 50102c851d75fb7b18a026367f4b6d1632c7456a
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Binary: `v6/sprefa-extract/target/release/extract` in YOUR worktree.

## Measure, on /Users/chrishafley/projects/typescript-go (read-only)
1. Whole-project `extract --resolve` over every `.go` file (one process,
   `timeout 10`, the way Fixes 2 did it) -> `resolved_edge` by kind,
   `resolved_import` by kind, `unresolved` by reason.
2. `plans/extract-crawl-2026-08-29/go.crawl.py <scratch>`: reachable from
   the 104 program roots, 4,832 (after #554) -> n.
3. Normalize `resolved_import` rows to the bench normal form and run
   `plans/extract-bench-2026-08-29/bench.py` against
   `go.oracle.module.tsv` (2,152 rows): recall, precision. Normalize
   call edges and run against `go.oracle.call.vta.tsv`: recall 5.6% -> n.
4. Append "Fixes 3 (module plane, PR #558)" to go.REPORT.md with those
   tables and the commands.

## Ownership
`plans/extract-crawl-2026-08-29/go.REPORT.md` (append only) and new tsvs
beside it. No `src/` edits; a defect is a report row with `file:line`.

## Receipt
Push branch `chore/go-module-plane-receipt`, `gh pr create --base main`,
hail `boop beep --no-wait --as chore-go-module-plane-receipt sprefa-coordinator "go receipt: PR #N, resolved_import <n>, module recall <r>, call recall 5.6%-><r>, reachable 4,832-><n>"`.
Laws: no em dashes, tables over prose, every extract call under timeout 10.
