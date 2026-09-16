# shared-sqlite-frontier LAB

Chris 2026-08-19: "i want a lab showing we can have fine perf with less
tables bc codegen is massive." This is a MEASUREMENT lab. It does not change
`lower.pl`, either emitter, or either runtime. Output is numbers.

## Read first (mandatory)
- `plans/2026-08-19-shared-sqlite-frontier.md` (the design under test)
- `issues/shared-sqlite-frontier/item.md`
- `.claude/skills/sqlite-costs/SKILL.md`, `.claude/skills/sql-relational-design/SKILL.md`
- `v6/labs/BENCHMARKS.md` sections 1-2 (the truth stack; how numbers are banked)
- `v6/prolog/lower.pl`: the per-relation transient tables the lowering mints
  today (`__frontier_<rel>`, `__next_frontier_<rel>`, `__support_next_<rel>`,
  `__delta_<rel>`, `__departure_frontier_<rel>`); find the DDL site for each
  and cite file:line in the report.

## Base
Worktree `~/projects/sprefa-worktrees/shared-sqlite-frontier`, branch
`lab/shared-sqlite-frontier`, HEAD = origin/main b62ea5b9e. `git log -1`
first; if not b62ea5b9e STOP.

## Own
`v6/labs/shared_frontier/**` only (new). Forbidden: everything else. The
lab may READ emitted programs under `v6/prolog/compile/out/` and the
pokeapi sources `v6/tsv2/gen/pokeapi_gen.dl6` / `pokeapi_expansion.dl6`.

## Questions the lab answers (each gets a table)
Q1. Today's table bill. For the emitted pokeapi program (compile it with the
    text door, `dl6c`/`compile.pl`, cite the exact command), count: durable
    tables, transient tables per relation, indexes, total DDL bytes, total
    CREATE statements. Same counts for 3 small fixtures from
    `compile/out/manifest.json` (pick one keyed, one recursive, one with
    negation). Plain counts from `sqlite_master` after boot, not estimates.
Q2. Tick cost, per-relation transient tables vs shared. Build a hermetic
    rig in SQLite (`@libsql/client` or better-sqlite3, whichever
    `v6/tsv2/scripts/scale-bench.ts` already uses; same driver) with N
    relations, N in {16, 64, 256, 1024}, each relation a typed durable table
    with an integer `__id` key. Arm A: per-relation `__frontier_<rel>` /
    `__support_next_<rel>` tables exactly as lower.pl shapes them today
    (copy the DDL). Arm B: ONE `frontier(relation_id,row_id,tick,sign)` and
    ONE `support_count(relation_id,row_id,rule_id,count)` as the plan
    writes them. Workload per tick: arrivals spread over k of the N
    relations (k in {1, N/8, N}), each arrival inserts the durable row and
    the frontier row, then a read `SELECT typed.* FROM frontier f JOIN
    rel ON rel.__id=f.row_id WHERE f.relation_id=? AND f.tick=?` per
    touched relation, then frontier delete for the tick. Report ms/tick
    median of 5 runs over 200 ticks, statements/tick, and `EXPLAIN QUERY
    PLAN` for the join in both arms (must be SEARCH on both; a SCAN is a
    finding, not a tuning target).
Q3. Boot cost. Time `CREATE` of all tables for N in {16,64,256,1024} for
    both arms; page_count after boot; bytes of DDL text.
Q4. Contention. One writer, frontier rows for 1024 relations in one tick,
    arm B only: does the single shared PRIMARY KEY
    (relation_id,row_id,tick,sign) cost more than 1024 small btrees?
    Number, not adjective (the sqlite-costs skill has the btree write rates
    to compare against).
Q5. Where time goes. For the worst arm-B cell, one `--cpu-prof` or
    `sqlite3_profile` breakdown: insert vs join vs delete.

## Rules
- Rig in TypeScript under `v6/labs/shared_frontier/`, run with the same
  node the repo uses (`v6/tsv2/package.json` engines). One `run.sh` that
  prints every table. No new top-level deps beyond what `v6/tsv2` has.
- Every number in the report comes from a command whose exact spelling is
  in the report. Medians, never single runs.
- Surrogate keys only; no composite TEXT keys anywhere in the rig.
- No prose conclusions beyond one line per table. Chris reads numbers.
- The 10-second law: a single rig cell over 10s is a finding; report it,
  do not wait it out.

## Deliverables (all under v6/labs/shared_frontier/)
- `rig/*.ts`, `run.sh`
- `REPORT.md`: TOC; Q1..Q5 tables; the EXPLAIN output; the lower.pl
  file:line citations; a final table "plan claim vs measured" with one row
  per claim in the plan's Context + Storage sections.
- `REPORT.visual.human.unga.md`: plain words, a mermaid of the two arms,
  the Q2 numbers as one table, zero citations.
Commit on the branch, push, open a PR titled "lab: shared sqlite frontier,
per-relation vs shared transient tables". Do not merge. Final message three
lines: PR url; the Q2 headline row (N=256, k=N/8, arm A ms vs arm B ms);
anything undone with exact error text.
