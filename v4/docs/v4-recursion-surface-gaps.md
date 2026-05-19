# Recursion at the sprf surface — WIRED (was "3 stacked gaps")

The fixpoint ENGINE was always proven; the sprf LANGUAGE now has a
wire to a working least-fixed-point through the FullSql run path.
Original finding 2026-05-18 on `f2ef0acc`; wired on branch
`feat/recursion-fixpoint-wire`.

## The original "3 gaps incl. trigger" model was a confound

The first RED spec and probe used `FROM`/`TO` as the rule's columns.
The fuser emitted UNQUOTED column identifiers, so the generated SQL
was `... r0.X AS FROM ...` — a SQLite syntax error. `run_fused_sql`
failed, fell back to the (inert, decl≠run) legacy path, and `reach`
came up empty. That empty table was misread as a TRIGGER gap
("declared `rule(){}` overloads never run"). Re-running the same
program with `SRC`/`DST` columns showed the base overload populating
correctly: a declared `rule(){body}` overload with no bare caller
DOES execute at the run path today. There is no trigger gap.

## The real gaps (validated by probe + run-loop instrumentation)

- **Gap 0 — unquoted identifiers.** `fuser.rs` emitted bare column
  idents in SELECT/AS/INSERT/CREATE. Reserved-word columns
  (`FROM`, `TO`, `ORDER`, …) produced invalid SQL. Fix: `qid()`
  double-quotes every emitted column ident; `build_live_facts_insert`
  unwraps quotes when deriving the live `<col>_id` FK names.

- **Gap 2.5 — self-`_facts` excluded from source-view rewrite.** The
  recursive step reads its own `{rule}_facts`.
  `source_tables_from_fused_sql(sql, target_facts)` excluded the
  target, so the self-read pointed at the raw FK table (`<col>_id`,
  no string columns) and the step SQL failed → the step contributed
  zero rows (not even one hop). Fix: `FusedRule.recursive`
  (set when a body RuleQuery reads `{rule}_facts`); `run_fused_sql`
  does NOT exclude the target for recursive rules, so a `{rule}`
  source view is created and the self-read is rewritten through it.

- **Gap 2 — single pass, no fixpoint loop.** `app.rs` ran each
  `FullSql` rule once. Fix: `run_fused_sql` iterates the recursive
  primary INSERT within the transaction until a pass inserts zero
  rows; `INSERT OR IGNORE` + deterministic blake3 `_id` makes each
  pass monotone/idempotent, so it converges. `SPREFA_RECURSION_CAP`
  (default 1000) is a hard round cap → loud abort, never a hang.

- **Gap 3 — pipe-write CROSS JOIN.** Still present and still
  deferred. The top-level pipe-write form
  (`reach?(X?,MID?) > edge?(MID?,Y?) > reach(X,Y);`) emits `ON 1=1`
  after the first join (`fuse_full_sql`, "best-effort"). The
  rule-OVERLOAD form's `ON` is correct, so recursion via overloads
  is unaffected. Not needed for Prolog-style clauses.

## Status

GREEN. `v4/tests/recursion_fixpoint_target.rs`:
- `recursive_rule_transitively_closes_to_fixed_point` (no longer
  `#[ignore]`) — canonical two-overload `reach` drives the full
  transitive closure `a-b a-c a-d b-c b-d c-d` on the raw fact
  table through the real run path.
- `probe_base_overload_decl_runs_at_run_path` — pins the base
  overload + the reserved-word (`FROM`/`TO`) class as a regression
  guard.

Engine layer (`v4/src/fixpoint.rs`, `tests/retraction_ph6.rs`)
remains the Rust-side proof; the surface no longer depends on it for
the FullSql overload path (this wire loops the SQL directly).

## Retraction gaps — WIRED 2026-05-18 (both specs GREEN)

Retracting a base `edge` fact and watching `reach` follow needed TWO
things wired. Both are now implemented; both specs are un-`#[ignore]`'d
and pass in the full gate (488/0/1).

**Part 1 — owner-scoped reconcile for stream/literal-fed rule writes.**
A `FactWrite` whose input cursors carry no
`mounted_query::SUPPORT_CURSOR_ID` (a literal/stream head) now
SELF-supports each produced row in the existing DRed ledger: one
`(row_id, owner_table, row_id)` triple — `SUPPORT_CURSOR_ID ==
SUPPORT_ROW_ID` is the self-support signature, and keeping the cursor
id equal to the sink `row_id` is what lets `cascade_retract` descend
transitively (the Part 2 link). A per-owner snapshot
(`factwrite_owner_snapshot`, keyed by `(table, assignment shape)` so
distinct write sites are distinct owners) holds the prior run's
`row_id`s tagged with a per-`run()` epoch
(`RuntimeGraph::run_epoch`); rows from a STRICTLY older epoch no
longer produced → `cascade_retract`. Within-run multi-writes
accumulate (same epoch, never self-retract). Soundness gate: the
reconcile runs ONLY when the write re-derives its FULL extent each
run — `FactWrite::full_extent`, set at lower time in `walk_pipe` (a
pipe touching any external/incremental source op — fs/read/ast/glob/
… — is NOT full extent, so an fs-driven warm-sliced re-run never
retracts an unchanged-file row). Spec:
`tests/surface_owner_retract_target.rs`.

**Part 2 — DRed for the recursive FullSql loop.** A recursive rule is
a pure derived relation: its overloads lower to SEPARATE fused rules
(a non-recursive base seeds `<rule>_facts`, the recursive step closes
over it). Before ANY overload runs, `run()` clears every recursive
rule's `<rule>_facts` + its `support_edges` lineage once
(SQLite-only path), then the existing in-tx fixpoint loop recomputes
the least fixed point over the now-current sources (Part 1 already
self-reconciled the base `edge` writes). The clear is at run start,
NOT per fused rule — a per-rule clear wiped the base seed a sibling
overload had just written. Spec: `tests/retract_recursion_demo.rs`
(rides on Part 1, also asserts the recursive re-closure).

The engine already proved the cascade possible
(`retraction_ph6.rs::recursive_transitive_closure_terminates_and_retracts`)
at the Rust store level; this wire reaches it from the surface.
