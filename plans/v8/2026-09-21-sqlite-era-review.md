# The sqlite layers across the sprefa eras

Read-only review. No behavior changes. Every claim carries a `path:line`, a
fixture, or a command. A claim taken from a comment or a plan page is labelled
as such. Numbers live in tables with their source.

Plain-words twin: `2026-09-21-sqlite-era-review.visual.human.unga.md`.

## 1. Layers by kind, then by era

Each row is one sqlite layer. The last column is the maintenance entry point.

### 1.1 Derived-product maintenance engines

| era | layer | what it stores | what maintains it | entry |
|---|---|---|---|---|
| v5 | derived tables in the engine db | one SQLite table per derived relation, plus `_delta_*` scratch tables | wipe per dependency component, then a semi-naive fixpoint to convergence | `v5/src/engine/derive.rs:334` (`rebuild_derived`), `:1673` (`rebuild_derived_seminaive`) |
| v6 | tsv2 runtime | one level table per head, `__delta_*`, `__frontier_*`, `__new_*`, `__support_next_*`, `__ping_*`/`__pong_*`, `__cone_*` | signed delta insert and refCount reconcile; delete-and-rederive for a recursive head | `v6/tsv2/runtime/1_incremental.ts:575` (refCount reconcile), `:715` (`maintain_head_in_place`) |
| v6 | `sprefa-engine-rs` tick engine | the same plan as one JSON document (`ProgramJson`) | refCount reconcile and whole-head recompute | `v6/sprefa-engine-rs/src/incremental.rs:2923`; the DRed plan is data only (`types.rs:485`) |
| v6 | `sprefa-store` cascade | `cx_row` (weight), `cx_dep`, `cx_frontier`, `cx_next`, `cx_hits`, `cx_cone`, `cx_refcount`, `cx_delta` | counting cascade, SCC cascade, DRed loop, DRed CTE, signed delta | `v6/sprefa-store/src/engine.rs:491` (`assert`), `:559` (`retract_dred`), `:717` (`retract_dred_cte`), `:812` (`retract_signed_delta`) |
| v8 | eval engine over rusqlite | the dictionary (`sym`, `term`, `term_arg`), one seed table per product, one material table per nonlinear product, one `sqlite_ivm` view `program` | the view maintains its own products; Rust rounds fill the material tables; a retraction empties the material tables and reruns the rounds | `src/_6_eval/_7_sqlite_eval.rs:116` (`reset_rounds`), `:130` (`run_rounds`) |
| live | `sqlite_ivm` extension | one arrangement per operator, one member table per fixpoint, source triggers | signed deltas per operator; delete-and-rederive per recursive stratum | `~/projects/sqlite_ivm/src/1a_relational.rs:728` (`fixpoint`), `1_maintenance.rs` |

### 1.2 Source-fact stores

| era | layer | what it stores | what maintains it | entry |
|---|---|---|---|---|
| v5 | call family in the storage db | `_call_owner`, `_call_raw_site`, `_call_def`, keyed by owner id and per-owner digests | owner-scoped affected-key reproject, replacing one owner's sites as a unit | `v5/src/storage/call.rs:248` (`apply_sqlite_call_owner_delta`) |
| v8 | runtime row store | the dictionary mirror plus one row table per product, with an arena watermark | append and commit past the watermark; no derived maintenance | `src/_9_runtime/_1_sqlite.rs:659` (`load_arena`), `:718` (`commit_arena`) |

### 1.3 Emitted plans and view declarations

| era | layer | what it emits | who maintains | entry |
|---|---|---|---|---|
| v6 | `lower.pl` level plan | `DELETE FROM <head>` plus inserts per head group, a refCount statement list, and a DRed plan for a recursive head | tsv2 runtime | `v6/prolog/lower.pl:4534` (group), `:5391` (`level_dred_plan`) |
| v7 | sqlite query emitter | `CREATE VIRTUAL TABLE <name> USING sqlite_ivm(<select>)` | the extension | `v7/src/3_emit/1c_sqlite_query_emitter.pl:521` |
| v8 | sqlite reify plan | one view statement plus the round statements for the materialized products | the extension, plus the Rust rounds | `src/_5_reify/_7_sqlite.rs:2203` (`view`), `:2155` (`round_statements`) |

### 1.4 Build-fixpoint floors and benches

| era | layer | what it stores | what maintains it | entry |
|---|---|---|---|---|
| v6 | `sqlite_baseline` | one `reachable` table, build only | recursive CTE, or a semi-naive wavefront, or a rowid-range wavefront | `v6/labs/exec_shootout/sqlite_baseline/src/engines.rs:58`, `:108`, `:182` |
| v6 | `sqlite_raw` | one `reachable` table, build only, in JS | one prepared semi-naive wavefront statement per round | `v6/labs/exec_shootout/sqlite_raw/REPORT.md` |
| v6 | declared-plan `sqlite_ivm` of that era | hidden result storage per installed result | whole-result recompute on every source row | `v6/labs/exec_shootout/postgres_pglite_ivm/29_sqlite_ivm_public.md` |
| v6 | `dl6` bench | `reachable`, `ping`, `pong`, `cone`, `delta_edge` over real rule columns | the store's `assert` and `retract_dred` | `v6/labs/exec_shootout/dl6/dred.mjs:1`, `dredopt.mjs` |

### 1.5 Test-only fixtures

| era | layer | what it stores | what maintains it | entry |
|---|---|---|---|---|
| v5 | incremental-view fixture | private in-memory `projected`, `joined`, `out` tables | targeted scoped deletes over a touched-candidate set, batched | `v5/src/engine/deltaflow.rs:116` (`apply_generation`) |

The v5 fixture declares itself outside production: "This module deliberately
uses a private in-memory SQLite schema. It does not participate in production
execution" (`v5/src/engine/deltaflow.rs:3-5`). Its delete statements keep a
scoped `WHERE` so a whole-table wipe is distinguishable
(`deltaflow.rs:17-21`), and its writes batch into one statement per chunk
(`deltaflow.rs:429`).

## 2. Maintenance shape by era

v5 wipes the derived tables per dependency component and refills them from the
base tables. The wipe is per component, not one upfront delete
(`v5/src/engine/derive.rs:408-419`). The tick calls the rebuild whenever a
derived component can have moved (`v5/src/engine/tick.rs:1039`, `:1162`), so
derived maintenance is a full recompute of the affected component, not a delta.
Source facts are handled differently: one owner's call sites are replaced as a
unit keyed by digests (`v5/src/storage/call.rs:248`).

v6 tsv2 splits by head kind. A plain head has a refCount reconcile: reseed the
counts from the base tables, subtract the difference into the head, delete what
fell to zero, insert what became derivable
(`v6/tsv2/runtime/1_incremental.ts:575-654`). An aggregate head has a
group-scoped recompute (`v6/prolog/lower.pl:4534-4556`, a comment). A recursive
head with a negative level body takes the delete-and-rederive half, gated by
`retraction_guard_sql` so an additive tick never touches the DRed statements
(`1_incremental.ts:715-717`, `:877-880`).

v6 `sprefa-engine-rs` carries the same plan as JSON. The refCount and recompute
arms are ported (`v6/sprefa-engine-rs/src/incremental.rs:2923`). The DRed plan
type exists (`types.rs:485-507`) and the bench plans written from the emitter
carry `dred_sql: null` (`v6/sprefa-engine-rs/bench/sf_join_per.rs:7`), so the
Rust port never runs a DRed walk.

v6 `sprefa-store` is the one sqlite home of that era. It holds a dependency
graph in tables and offers four retraction shapes beside the forward add
(`v6/sprefa-store/FINDINGS-AND-GAPS.md` section 1, a summary page). Counting
(`retract`) keeps a phantom cycle alive; DRed is cycle-correct because it
over-deletes the cone and rederives rows anchored to a survivor
(`engine.rs:484-489`, a comment).

v8 lowers every rule set to one `sqlite_ivm` view
(`src/_5_reify/_7_sqlite.rs:2203-2211`). A linear recursive component becomes a
recursive CTE inside that view, capped by `RECURSION_DEPTH_LIMIT`
(`_7_sqlite.rs:190`, `:1732`). A nonlinear component, one whose rule body reads
its own recursive product at more than one position, is instead materialized
into a plain table (`_7_sqlite.rs:532-556`) and filled by Rust-sequenced
`INSERT OR IGNORE` rounds (`_6_eval/_7_sqlite_eval.rs:171`). The view reads the
material table, since a view cannot read another view
(`plans/v8/2026-09-18-eval-on-sqlite-ivm.design.md`, fork F3).

The extension maintains its own products from signed row deltas per operator
and runs delete-and-rederive inside a recursive stratum
(`~/projects/sqlite_ivm/README.md`, "State and incremental work"; code at
`src/1a_relational.rs:725-900`).

## 3. Delete-and-rederive, per implementation

| implementation | loop | shape | bound | termination |
|---|---|---|---|---|
| `sprefa-store` `retract_dred` | `v6/sprefa-store/src/engine.rs:559` | set-at-a-time per round, temp frontier ping-pong, one transaction | none, rounds equal the cone depth | `SELECT count(*) FROM <next>` is zero in each pass (`engine.rs:616`, `:687`) |
| `sprefa-store` `retract_dred_cte` | `engine.rs:717` | set-at-a-time, recursive CTEs inside SQLite | none | the CTE exhausts (`engine.rs:792`) |
| `sprefa-store` `retract_signed_delta` | `engine.rs:812` | set-at-a-time per round over a `(round,key,diff)` table | none | no delta row at the current round (`engine.rs:878`) |
| tsv2 `maintain_head_in_place` | `v6/tsv2/runtime/1_incremental.ts:715`, over-delete at `:799`, revive at `:820` | set-at-a-time per round, `__ping`/`__pong` role swap, `__cone` | the cone cap is `floor(head_count / 4)` (`:796`); each walk is capped by the expand plan round cap, read in `bounded_wave` (`:557-575`) | a hop returns zero, or the cone passes the cap (`:801`) and the driver falls back to recompute (`:848`, `:880`) |
| `dl6` bench two-pass | `v6/labs/exec_shootout/dl6/dred.mjs:1` | set-at-a-time two-pass over real rule columns | none | checksum matches the recompute oracle |
| `sqlite_ivm` `fixpoint` | `~/projects/sqlite_ivm/src/1a_relational.rs:728` | set-at-a-time, semi-naive rounds whose delta is a rowid range of the member table | none stated; a fixpoint nested inside another step is not buildable | `max_rowid` stops growing (`1a_relational.rs:788`) |
| v8 materialized components | `src/_6_eval/_7_sqlite_eval.rs:116` | set-at-a-time `INSERT OR IGNORE` rounds over plain tables | `NONLINEAR_ROUND_LIMIT` (`:44`) | a round changes no row (`:148`), or the limit raises a diagnostic (`:153`) |

v5 has no delete-and-rederive. Its derived maintenance is wipe plus refill
(`v5/src/engine/derive.rs:334`). Its targeted-delete design stays in the
test-only fixture (`v5/src/engine/deltaflow.rs:3-5`).

The v8 delete path is not DRed. On a retraction of a seed it empties every
material table and reruns the rounds (`src/_6_eval/_7_sqlite_eval.rs:113-127`,
`:1130`). The code comment gives the reason: sqlite_ivm's delete path and the
plain-table rounds "do not yet agree with a fresh view (`DL8_EVAL_CHECK=1`)"
(`:1095-1096`, a comment). The `DL8_EVAL_CHECK` comparison prints fresh-only
and cached-only rows (`:1156-1175`).

## 4. What was measured

| era | receipt | file | numbers, quoted |
|---|---|---|---|
| v5 | semi-naive versus naive counter at a 40-edge chain | `v5/tests/it/seminaive.rs:350-375` | naive `2 * n` = 80 full re-runs against a budget of 20; semi-naive 0; "semi-naive issues ZERO full-input re-runs (deltas only)" |
| v6 | incremental tick versus refCount recompute | `v6/labs/exec_shootout/dl6/FACTS.dredland.md` section 1 | insert one edge 2,019 ms to 42 ms; delete one edge 3,907 ms to 56 ms; delete a structural edge 3,878 ms to 82 ms; empty drain 1,926 ms to 1 ms |
| v6 | build tick, DRed landing | `v6/labs/exec_shootout/dl6/FACTS.dredland.md` section 2 | grid_10000 2,110 ms to 2,141 ms; layered 19,954 to 20,386; chain 32,112 to 33,338 |
| v6 | DRed phase zero over a closed graph | `plans/2026-08-06-dred-emit-lab-header.md` section 2 | insert one edge 40 ms versus 2,179 ms; delete one structural edge 60 versus 2,195; delete 100 jumps 7,153 versus 2,218 (recompute wins) |
| v6 | DRed variants and the cone bail | `plans/2026-08-06-dred-emit-lab-header.md` section 3 | two-pass 8,179 ms; variant B 7,440 TAKE; variant D 36,209 REJECT; variant BC 36,663 REJECT; variant E (mid-walk bail at cone past a quarter of the head) 3,190 TAKE; full rebuild 2,190 |
| v6 | pure SQLite build floor, Rust | `v6/labs/exec_shootout/sqlite_baseline/BASELINE.md` | grid_10000 tuned_wave 1,065 ms at 16.2 MB versus dl6 1,265 ms at 715 MB; layered tuned_range 10,254 versus 11,721; chain 11,224 versus 20,850 |
| v6 | pure SQLite build floor, JS | `v6/labs/exec_shootout/sqlite_raw/REPORT.md` | grid_10000 992 ms versus dl6 1,998 ms; chain 9,212 versus 30,670 |
| v6 | retraction matrix, all four shapes | `v6/sprefa-store/PERF-REPORT.md` | DAG 960k: sqlite-count 429.6 ms in 23 statements; sqlite-dred-loop 1,774.6 in 53; sqlite-dred-cte 2,582.7 in 6; sqlite-signed-delta-v2 1,135.6 in 3; dd 174.6. CYC 960k: counting returns 830,478 survivors and is marked `NO`; DRed returns 815,240 and matches the oracle |
| v6 | declared-plan ivm, whole-result recompute | `v6/labs/exec_shootout/postgres_pglite_ivm/29_sqlite_ivm_public.md` | 12,000 rows at batch size 1,000: SQLite plan full-refresh 8,354.893 ms, pg_ivm 23.646, DD 1.175 |
| v6 | retraction tick bench, per-tick cost | `v6/labs/exec_shootout/tick_bench/TICKS.md` | chain 10000: mono 55.514 ms median, mercury-semi-naive 214.253; grid 10000: 44.049 versus 143.607; "Its delta-proportional number is not built yet" |
| v6 | sqlite-native competitive suite | sprefa commit `e2052d5ae`, `TASKS/7_sqlite_competitive.REPORT.md` (not at HEAD) | median cumulative mutation plus query ms: DD 0.467, SWI 0.521, SQLite full query 2.544, Take 2 6.301, Take 1 7.398, PG full query 6.366, pg_ivm 18.044; "All eleven core circuits include actual incremental maintenance, with negation, cyclic retraction" |
| v7 | prepared-statement cache | `v7/receipts/0_sqlite_query.md` | UPDATE median 283.269 ms at cache 16 to 58.394 ms at cache 128; "median difference: -79.4%" |
| v8 | none | `tests/_27_eval_sqlite.rs:10` | no wall-time receipt exists; the only numbers are oracle floor counts (FLOOR 32) and the commit-title sequence 4/32, 7/32, 9/32, 10/32, 14/32, 17/32, 22/32 |
| live | sqlite_ivm extension | `~/projects/sqlite_ivm/bench/46_feature_acceptance.md`, `bench/54_shootout.md` | version 0.2.0: 44 cases and 7,656 checked states passed against an independent SQLite connection, DD graphs, and PostgreSQL; pg_ivm 1.15 matches in 16 of 17 admitted cases |

Eras with no receipt: v8. The sqlite engine landed with oracle parity counts
only. No timing was taken for a retraction tick, and no bench directory exists
(`ls bench` fails at the repo root).

## 5. v8 today

The eval engine's established state, verified in this review:

| fact | site |
|---|---|
| a retraction of any seed empties every material table and reruns the rounds | `src/_6_eval/_7_sqlite_eval.rs:113-127`, `:1130` |
| the delete path does not agree with a fresh view under `DL8_EVAL_CHECK` | `:1094-1096` (comment), comparison at `:1156-1175` |
| the material rounds are capped at `NONLINEAR_ROUND_LIMIT` | `:44`, `:130-160` |
| the recursive CTE path is capped at `RECURSION_DEPTH_LIMIT` | `src/_5_reify/_7_sqlite.rs:190`, `:1732` |
| the insert delta is tested to read like a fresh view | `src/_6_eval/_7_sqlite_eval.rs` test `a_seed_delta_reads_like_a_fresh_view` (`:1279-1333`) |
| the in-memory evaluator table is append-only, with no remove or replace | `src/_6_eval/_3_table.rs:27-50` |
| the planned `Table::replace` and `retract_and_rederive` never landed: no `tests/_19_retraction.rs`, no `fixtures/retraction`, no `bench/retraction` | `ls tests`, `ls fixtures`, `ls bench` |

The retraction lane is written and unlanded. `plans/v8/2026-09-14-v8-retraction.brief.md`
names `Table::replace`, `Table::remove`, and `retract_and_rederive` as its
deliverables, with a per-stratum delete-and-rederive and a measurement harness
at three sizes. None of those files exist at HEAD.

The v8 evaluation design chose the materialized path deliberately:
`plans/v8/2026-09-18-eval-on-sqlite-ivm.design.md` fork F4 selects option (b), a
budgeted Rust-sequenced `INSERT INTO t SELECT ... FROM t ...` round loop on
plain tables, over a diagnostic. The same page states why a material table is
needed at all: a view cannot read a view, so upstream products must be ordinary
tables. That is the design page's own words.

## 6. Verdict

An earlier era solved something v8 has not.

v6 implements cycle-safe incremental retraction for recursive products, in
code that runs, and v8 does not. The v6 shapes:

| shape | code | what v8 lacks |
|---|---|---|
| emitted DRed for a recursive head, with a cone cap and a recompute fallback | `v6/prolog/lower.pl:5391`, `v6/tsv2/runtime/1_incremental.ts:715-885` | v8 materializes a nonlinear product and empties it on any retraction |
| four retraction algorithms over a table graph, DRed among them | `v6/sprefa-store/src/engine.rs:559`, `:717`, `:812` | v8's eval engine has one retraction shape, a full rebuild |
| DRed inside the extension v8 already loads, for a recursive stratum | `~/projects/sqlite_ivm/src/1a_relational.rs:728` | v8 routes a nonlinear recursive product around the view into a plain table, so the extension's DRed never sees it |

The v8 gap is narrower than "no incremental retraction anywhere". A linear
recursive component lives in the view as a recursive CTE, and the extension
maintains that CTE with delete-and-rederive on a seed retraction
(`src/_6_eval/_7_sqlite_eval.rs:586-596` deletes from the seed tables that the
view reads). The gap is the nonlinear component, plus the in-memory evaluator
which has no retraction at all (`src/_6_eval/_3_table.rs:27-50`).

Why the earlier code was dropped, from the record: the eval-on-sqlite design
routed nonlinear recursion to plain tables on the grounds that SQLite rejects a
second reference to the recursive table in one step
(`plans/v8/2026-09-18-eval-on-sqlite-ivm.design.md`, fork F4). That choice made
the material tables ordinary sources, and ordinary sources have no retraction
rule. The retraction lane that would have added one
(`plans/v8/2026-09-14-v8-retraction.brief.md`) was written and never landed. So
the loss is a path not taken rather than a removal.

Recovery path, one paragraph. Port the v6 shape onto the material tables.
A material table is the cone of a nonlinear component, so the same two passes
apply: over-delete the rows the retracted seed justified, then rederive the
rows still anchored to a surviving seed. In v8 that means adding a delete arm to
the round statements, which today are `INSERT OR IGNORE` only
(`src/_5_reify/_7_sqlite.rs:539`, `:552`), and a cone table beside the material
table. The bound is already written down: the v6 lab bails to a full rebuild
when the cone passes a quarter of the head
(`plans/2026-08-06-dred-emit-lab-header.md` section 3), and
`NONLINEAR_ROUND_LIMIT` (`src/_6_eval/_7_sqlite_eval.rs:44`) already caps a
runaway walk. The extension cannot do this part, because the material table is
a source it maintains from the outside; the walk belongs in the round
statements, which Rust sequences today. The check is the existing
`DL8_EVAL_CHECK` comparison: with the delete arm, the cached read must equal a
fresh view (`src/_6_eval/_7_sqlite_eval.rs:1156-1175`), which is exactly the
test that fails today and forces `reset_rounds`.