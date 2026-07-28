# EMITTER P2+P3 HEADER (planner contract): the 1M-competition entry

Arc parent: plans/2026-07-28-incremental-sql-emitter-header.md (P1 landed:
incremental default for non-recursive level rules, 165x at s2/100k, curve
flat). This header covers the two phases that remain between tsv2 and the
rust store's 960k-row retract competition (v6/sprefa-store/PERF-REPORT.md).

## The competition standings tsv2 must enter

DAG 960k (800k-row retract): dd 172ms (resident, not our class),
sqlite-count 443ms / 29 stmts (rust heap 0.12MB -- the retract is pure
sqlite C), count-scc 1998ms, dred-loop 2054ms, dred-cte 2520ms. CYC 960k:
bare count is WRONG (830478 vs 815240 survivors); count-scc correct.
Same cycle failure independently re-proven by the sqlite-retraction lab
(support_count leaves cycle rows alive) and the types lab.

TARGET: the emitted-SQL thesis says a prolog-emitted program driven from
node should post sqlite-count-CLASS numbers, delta = driver overhead only,
because the rust 443ms never touches rust-side memory. That claim is the
grade.

## P2: recursive strata, semi-naive across ticks

- Frontier tables in SQL (the delta/next/promote shape lowerSql.ts already
  runs WITHIN a stratum; lift it across ticks), emitted per program.
- Removes the P1 naive fallback for recursive rels.
- Grade: sweep byte-identity (per-fixture unchanged); the recursive
  fixtures currently riding the fallback flip to incremental; EXPLAIN
  SEARCH-not-SCAN on frontier reads; statement counts flat.

## P3: retraction as emitted support-count SQL + cycle guard

- Statement patterns: the sqlite-retraction lab verdict
  (plans/2026-07-28-sqlite-retraction-verdict.md) + the rust store's
  sqlite-count 29-statement shape (src/engine.rs is the authority).
- CYCLE GUARD IS NOT OPTIONAL: bare count ships only with the guard the
  perf report and both labs demand -- the recursive-CTE reseed referee (8ms
  at 10k in the lab, no depth ceiling) or the scc variant; pick by
  measurement, record the choice.
- Removes the P1 naive fallback for retraction ticks and negative bodies.
- Grade: sweep byte-identity incl the retraction fixtures; crash-mid-
  cascade recovery matches the lab's receipts.

## The competition grade (user directive: old harness, nothing new)

The existing rig only: a tsv2 runner in v6/sprefa-store/bench/engines/
beside sqlite-count/dd (tsv2_gen.sh is the precedent shape, same CSV
protocol, same reach workload, same oracle hash gate as perf_report's
input-hash check). Rows for DAG 60k/240k/960k + CYC 960k, pasted into the
SAME standings table. No new harness, no new workload, no new report
format.

## Sequencing

- BEHIND the enum-variants arc (codex/enum-variants owns
  parse/analyze/lower right now).
- P2 before P3 (retraction fixtures exercise recursive views).
- Runner: sol-class, no-commit flow (codex sandbox cannot write git
  metadata; coordinator verifies and commits -- the P1 precedent).

## AMENDMENT (user, 2026-07-29): common memory columns + the 6.2.0 gate

Memory reporting gets ONE shape across implementations, extending the
existing 8-field CSV protocol (pure_wrap.sh header is the authority),
never a second format: peak RSS via /usr/bin/time -l (already common),
host_peak_mb (rust = CappedAlloc high-water; TS = v8 heap high-water
sampled per tick -- the host-residency gauge), sqlite_hw_mb (rust =
sqlite3_memory_highwater; TS = N/A-with-reason until the driver exposes
it, never a fudged number), db_mb (file size). P3's competition rows
carry all columns so they drop into PERF-REPORT.md's standings table
unmodified.

THE 6.2.0 GATE (user: "drive this home"): v6.2.0 is tagged on main when
(1) P2 landed, recursive rels incremental; (2) P3 landed, retraction +
mandatory cycle guard, naive fallback surface reduced to named
residuals; (3) tsv2 rows sit in the PERF-REPORT standings with the
common memory columns; (4) every suite green on the tagged commit
(conformance, sweep, roundtrip, plunit, tsv2 + gate, dl, store,
ratchet, endurance). Tag is local like every commit; push stays a human
decision.
