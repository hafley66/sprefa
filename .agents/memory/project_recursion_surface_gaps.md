---
name: project-recursion-surface-gaps
description: "sprf recursion: fixpoint engine proven but Rust-only; surface = 3 stacked gaps; pinned RED spec on branch, not merged"
metadata: 
  node_type: memory
  type: project
  originSessionId: b5f0ade9-540e-4fda-9f7a-284766ab6419
---

Finding 2026-05-18 (main `f2ef0acc`). sprf-surface recursion does
NOT transitively close. The fixpoint ENGINE works and is proven:
`v4/src/fixpoint.rs` semi-naive + `stratify` + DRed retraction;
`tests/retraction_ph6.rs::recursive_transitive_closure_terminates_and_retracts`
drives self-dep `reach` to correct full closure (in the 484/0
suite). But it is reachable ONLY from Rust (hand-built `EvalRule`);
zero `v4/src/*` callers of `eval_stratum`/`eval_all` outside
`fixpoint.rs`; no lowering builds an `EvalRule`; CLI/run never calls
it.

CORRECTION 2026-05-18 (probe + run-loop instrumentation,
branch `feat/recursion-fixpoint-wire`): the original "3 gaps
incl. trigger" model was DISTORTED by a reserved-word confound.
The RED spec + first probe used `FROM`/`TO` as rule columns →
fuser emits `r0.X AS FROM` (unquoted) → SQLite syntax error →
`run_fused_sql` fails → inert legacy fallback → `reach=[]`. That
`reach=[]` was a FALSE trigger signal. Re-run with `SRC`/`DST`:
base overload `rule(:reach,SRC?,DST?){edge?(SRC?,DST?)}`
populates correctly via FullSql. There is **NO trigger gap**: a
declared `rule(){body}` overload with no bare caller DOES run at
the run path today (walk.rs:75-96 pushes it; app.rs:1643 runs
FullSql). Opus reviewer's code-read was right; my empirical
"decl never runs" was wrong.

TRUE gap model (validated):
- Gap 0 (NEW, trivial): `fuser.rs` emits UNQUOTED column idents;
  reserved-word cols (`FROM`/`TO`/`ORDER`/…) break generated SQL.
  Quote all emitted idents.
- Gap 2.5 (BLOCKING): recursive step SQL is correct
  (`FROM reach_facts r0 JOIN edge_facts r1 ON r0.DST=r1.X`) but
  `source_tables_from_fused_sql(combined, target_facts)` EXCLUDES
  the target's own `{rule}_facts` (app.rs:852 + exclude) → the
  step's self-read is never rewritten to the live FK view → step
  SQL fails → step contributes ZERO rows (recursive RED still
  `reach=[a-b,b-c,c-d]`, base only, NOT even one hop).
- Gap 2 (downstream): `app.rs:1643` runs each FullSql rule ONCE;
  no loop to quiescence. Only observable after Gap 2.5.
- Gap 3 (deferred, N/A here): pipe-write CROSS JOIN bites only
  the separate-pipe form; the rule-OVERLOAD form's `ON` is
  correct.
Revised order: Gap 0 → Gap 2.5 → Gap 2. Task #8 RESUMED (impl).

WIRED + GREEN 2026-05-18 (branch `feat/recursion-fixpoint-wire`,
NOT yet merged; base = main HEAD `f2ef0acc`, not stale). Impl:
- `fuser.rs::qid()` double-quotes every emitted column ident
  (SELECT/AS/INSERT col-list/CREATE TABLE/streamed prepared).
- `FusedRule.recursive` + `is_recursive()`; set in `fuse_full_sql`
  when `joined_tables` contains `{rule}_facts`.
- `app.rs::run_fused_sql`: recursive ⇒ source-table discovery does
  NOT exclude target (self-view `{rule}` created + self-read
  rewritten through it); primary INSERT loops in-tx to a fixed
  point; `SPREFA_RECURSION_CAP` (default 1000) hard round cap.
- `app.rs::build_live_facts_insert`: unwrap `"..."` from the parsed
  col list before deriving live `<col>_id` FK names.
RED spec un-`#[ignore]`'d + GREEN; added
`probe_base_overload_decl_runs_at_run_path` as the FROM/TO
reserved-word regression guard. Full v4 gate 486/0/1 (was 484/0;
+2 = the 2 new tests; 1 ignored = pre-existing branch1 git
fixture). Gap 3 (pipe-write CROSS JOIN) still deferred, not needed
for prolog-style overload clauses.

RETRACTION 2026-05-18 (main `f0422142`, both pinned RED+ignored,
no behavior change). Retract a base `edge` fact → `reach` does
NOT follow. Demo: re-run script with a source line removed →
NOTHING changed (`edge` itself didn't retract). TWO independent
gaps:
- Part 1 (PRE-EXISTING, orthogonal to recursion): stream/literal
  pipe → rule write lowers to plain `FactWrite`.
  `FactWrite::render_batch` records DRed support rows only when
  the input cursor carries `mounted_query::SUPPORT_CURSOR_ID`
  (fact.rs:101-104); literal head never sets it →
  `store.insert_batch` (fact.rs:125) insert-only, no owner
  reconcile. Surface owner-retraction wired ONLY for
  `fs>read>re` hits-owner + mounted SQL-query support paths.
  Spec: `tests/surface_owner_retract_target.rs`.
- Part 2 (the recursion wire): `run_fused_sql` recursive
  INSERT-OR-IGNORE loop has no link to `mounted_query_support`/
  `cascade_retract`; materialize-forward. Spec:
  `tests/retract_recursion_demo.rs` (rides on Part 1).
Engine CAN cascade (`retraction_ph6.rs::recursive_transitive_
closure_terminates_and_retracts`) but Rust-store level only.

RETRACTION WIRED + MERGED 2026-05-18 (main `3f4163f3`, ff from
`f0422142`, single squash commit; both specs un-`#[ignore]`'d +
GREEN; full v4 gate 488/0/1; v4-bench linux quick unaffected —
hot path fs/ast ⇒ full_extent=false ⇒ reconcile short-circuit).
- Part 1: stream/literal FactWrite (no upstream SUPPORT_CURSOR_ID)
  SELF-supports each row (triple `(row_id,table,row_id)`; cursor_id
  == sink row_id so `cascade_retract` descends transitively = the
  Part 2 link). Per-owner snapshot `factwrite_owner_snapshot` keyed
  by `(table, assignment-shape)` (distinct write sites = distinct
  owners → fixes the bang multi-call regression), rows tagged with
  a per-`run()` epoch `RuntimeGraph::run_epoch`/`begin_run_epoch`
  (clock SourceId `sprf://run-epoch`). Only STRICTLY-older-epoch
  rows retract; same-epoch accumulate (within-run multi-write
  safe). Gated on `FactWrite::full_extent` set at lower time in
  `walk_pipe` (pipe touching any EXTERNAL_SOURCE_OPS fs/read/ast/
  glob/repo/path/lsp/http/… ⇒ NOT full ⇒ skip reconcile → fixes
  the fs mounted-read-pipeline regression: warm-sliced re-run must
  not retract unchanged-file rows).
- Part 2: recursive overloads lower to SEPARATE fused rules (base
  seeds `<rule>_facts`, recursive step closes over it). `run()`
  clears every recursive rule's `<rule>_facts` + `support_edges`
  lineage ONCE before any overload (SQLite path), then the existing
  in-tx fixpoint recomputes the LFP over current sources. KEY
  GOTCHA: clear MUST be run-start, not per-fused-rule — a per-rule
  clear wiped the base seed a sibling overload had just written
  (`reach=[]` on run1).
Files: fact.rs (self-support + owner_key + full_extent gate),
mounted_query.rs (reconcile_owner_table + epoch snapshot),
runtime_graph.rs (run_epoch), walk.rs+ctx.rs (pipe_full_extent),
sql.rs (with_full_extent tag), app.rs (Part 2 clear + epoch bump).
Doc: v4-recursion-surface-gaps.md "Retraction gaps — WIRED".
Latent (documented, untested): multi-batch SINGLE owner — batch2
would treat batch1 as prior within an epoch only if epochs
differed; same-epoch accumulate covers it. Cleaner future: replace
runtime-derived extent with this lower-time tag everywhere.

User ruling: "stop, report only" — no impl, scope deferred. Pinned
on branch `feat/recursion-fixpoint-wire` commit `d1afdbb2` (NOT
merged to main): `#[ignore]`'d RED spec
`v4/tests/recursion_fixpoint_target.rs` + writeup
`v4/docs/v4-recursion-surface-gaps.md`. Task #8 PARKED. Resume =
un-ignore + drive all 3 gaps. Worktree removed; branch persists.

RECURSIVE-OWNER-SUBSCRIBE WIRED + MERGED 2026-05-18 (main
`37bb93a5`, rebased onto `36f15733` then ff; gate 492/0/1 —
+recursive_owner_subscribe.rs, +3 from main's dots-chained leg).
app.rs run() pre-loop Part-2 block replaced: each recursive
FullSql rule gets a stable OwnerNode (`sprf://ast/rule/{n}#fused`)
SUBSCRIBE'd to its external source tables; self + co-recursive
(rec_names) excluded = structural wake-cycle guard (owner never
subscribes to a source it/an SCC sibling produces). Negative
cycle ⇒ stratify() Err ⇒ runtime_diags.emit(to_diag()
`lower/unstratifiable-negation`); data-cycle unchanged
(SPREFA_RECURSION_CAP). Run-scoped incremental skip
(skip_recompute set + pipes-loop `continue`) code-complete but
GATED OFF behind env `SPREFA_REC_INCREMENTAL`: warm_changed_paths
is a process-wide CORPUS signal, NOT per-rule source attribution
— observed Some([]) even after an edited .edges corpus file, so
skipping by default = unsound stale closure. Default = recompute
(proven Part-2). FOLLOW-UP to flip default-on: per-rule source-set
attribution oracle. Note: explicit `path>read` + fs-corpus in a
non-git tempdir don't refresh `edge` across fresh-process runs
(existing fs-incremental limit) → verification test uses a
LITERAL-block edge (full_extent ⇒ Part-1 reconcile, deterministic
each run). retract_recursion_demo + surface_owner_retract_target
stay green. Plan file `~/.claude/plans/recursive-owner-
subscribe.md` fully executed.

Related: [[project-dots-types-nesting]] (the binding/rule-model
work that landed just before this on the same main leg).
