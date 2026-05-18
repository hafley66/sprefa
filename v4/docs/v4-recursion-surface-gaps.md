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

## Retraction gaps (two independent, both pinned RED)

Retracting a base `edge` fact and watching `reach` follow needs TWO
things wired. Demoed 2026-05-18: re-running the script with a source
line removed changed nothing — `edge` did not retract, so `reach`
never got the chance to cascade.

**Part 1 — surface owner-reconcile for stream-fed rule writes
(pre-existing, orthogonal to recursion).** `edge(X,Y)` lowers to a
plain `FactWrite` (`sql.rs` -> `fact.rs`).
`FactWrite::render_batch` records DRed support rows ONLY for input
cursors carrying `mounted_query::SUPPORT_CURSOR_ID`
(`fact.rs:101-104`); a literal/stream head never sets it, so
`store.insert_batch` (`fact.rs:125`) is insert-only with no
generation/owner reconcile. Owner-scoped retraction is wired only
for the `fs>read>re` hits-owner path and the mounted SQL-query
support path. Pinned: `tests/surface_owner_retract_target.rs`
(`rerun_with_dropped_source_row_retracts_owner_fact`, `#[ignore]`).

**Part 2 — DRed link from the recursive FullSql loop (this wire).**
`run_fused_sql`'s recursive loop is `INSERT OR IGNORE` only, with no
link to `mounted_query_support` / `cascade_retract` (the
`support_edges` table it writes is disconnected from the real DRed).
Recursive rules are materialize-forward. Pinned:
`tests/retract_recursion_demo.rs`
(`retract_base_edge_cascades_recursive_reach`, `#[ignore]`) — rides
on Part 1 and additionally asserts the recursive re-closure.

The engine proves the cascade is possible
(`retraction_ph6.rs::recursive_transitive_closure_terminates_and_retracts`,
`cascade_retract` over the `reach` closure) but only at the Rust
store level. Both specs assert the SOUND target and are `#[ignore]`'d
so the gate stays green while the gaps are explicit.
