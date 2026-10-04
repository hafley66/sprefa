# Brief: shared narrowing (projection pushdown follow-up)

Base: local main `5d242c08c` (sqlite_ivm `db1e57c`). Prior: `plans/v8/2026-10-04-projection-pushdown.findings.md`.

Problem: per-goal narrowing stops arrangements from being shared when two goals read one relation
through different column sets: arrangements 337 -> 344 (c15 unit), 879 -> 906 (comptime).

Do: for each (relation, join key) pair, narrow to the union of the columns every reader of that
relation keeps, so readers share one arrangement; each reader then projects its own columns from
the shared one inside its join (fused Mfp, as ivm-dd already does). Measure before/after:
arrangements, arranged cells, registry build time/RSS on both engines, suite times. Keep the
arranged-cell savings of pushdown (target: arrangements <= the pre-pushdown 337 / 879, cells not
above today's 7214 / 252806). Add a "How it works" entry to the findings file.

Method, laws, receipt: as `plans/v8/2026-10-04-projection-pushdown.brief.md` (own worktree,
sqlite_ivm clone inside it from main `db1e57c`, branch `perf/shared-narrowing`; all edits first,
then test; rows byte-identical; registry cmp; both suites; sqlite_ivm crate tests; no merge/push).
