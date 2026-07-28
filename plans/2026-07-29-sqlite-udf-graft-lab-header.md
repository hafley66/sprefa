# SQLITE UDF GRAFT LAB (planner contract; user go 2026-07-29)

v5 registered 16 scalar UDFs into sqlite (src/db.rs, names as of today:
sprf_sym_intern, sprf_lower, sprf_upper, sprf_lcfirst, sprf_ucfirst,
sprf_trim, sprf_norm, sprf_strip_prefix, sprf_strip_suffix, sprf_sym,
sprf_lines, sprf_replace_re, + 4 more registered at db.rs:419/481/518
call sites; read the file, inventory ALL 16 with their semantics). The
question: how does that string/expression capability graft into the
v6/tsv2 world under the expression_residency ruling (fuse to SQL
deltas, TS deopt last) and the host_residency law (rows stay in
sqlite).

Lab home: v6/prolog/labs/sqlite_udf/ + ONE verdict doc
plans/2026-07-29-sqlite-udf-graft-verdict.md. TOUCH NOTHING ELSE
(concurrent arcs own compile/* and labs/hosts_extraction/*; read
anything, write only your fence). Labs die on landing.

## Questions (each graded by executable checks, PASS-only stdout)

Q1 INVENTORY: all 16 v5 UDFs, per function: semantics (from the rust
   body), whether MODERN SQLITE CORE already covers it (lower/upper/
   trim/replace exist; regexp does not; document exact coverage with
   a real sqlite version check), and which v5 examples/rails actually
   USE it (grep examples/ and .dl/; usage count per function).
Q2 DRIVER REALITY: can the v6 TS seam register UDFs at all? Assess
   @libsql/client (the current driver) empirically, and IF it cannot,
   assess the named alternatives with a build-vs-buy table (standing
   law: no one-line dismissals): better-sqlite3 .function(),
   node-sqlite3, sql.js, a sidecar extract binary registering UDFs in
   the rust process. Real code runs, not doc reads.
Q3 GRAFT SHAPES, priced per function CLASS (pure-string, regex,
   intern/sym, line-splitting), graded both ways where feasible:
   (a) SQL-native rewrite: emit the sqlite-core expression when
       coverage exists (fuse ruling's first choice);
   (b) UDF registration at the driver seam (needs Q2 yes);
   (c) TS deopt: post-SQL map over delta rows only (never full
       tables, host_residency);
   (d) emit-time: prolog computes it at compile time when arguments
       are constants.
   Criteria: correctness parity with the rust UDF on a shared input
   corpus (byte-compare outputs), delta-statement fusion (does it ride
   the P1-P3 incremental statements), portability across future
   backends (the rust return), linecount.
Q4 EXPRESSION-LIFT CONSEQUENCE: with typed columns landed, write the
   assertion set for lifting the phase-C comparison/arith/:= refusals
   into the incremental emitter (what must be true per statement
   family; the "1" vs 1 and bigint-bind regressions become named
   checks). NOT an implementation, an assertion set the lift arc
   executes against.
Q5 sprf_sym_intern / sprf_sym: these are the v5 intern-dictionary
   functions. Name their relationship to the types-lab content_id()
   ruling (surrogate mate, dense ints); a graft that conflicts with
   that ruling is refused in the verdict.

## Grades

Lab suite exit 0, PASS-only stdout, twice. Parity corpus: real strings
pulled from this repo's paths/symbols (hermetic copy inside the lab
dir), rust-UDF outputs captured ONCE via a scratch v5 db (never touch
~/.local/state/sprefa; SPREFA_CONFIG=/nonexistent + scratch --db, the
house hermetic pattern) and byte-compared against each graft shape.
No-drift: conformance + roundtrip untouched and green.

## Deliverable

Verdict: per-function-class graft table (sql-native | udf | ts-deopt |
emit-time) with criteria visible, the Q4 assertion set, Q2 driver
verdict with receipts, named slots for anything unresolved.
