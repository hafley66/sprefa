# STORE-ADOPTION FINDINGS (sonnet, tsv2 x sprefa-store engine)

Directed by: Chris Hafley. Implemented by: sonnet agent (v6/tsv2 investigation
+ prototype), 2026-07-28. First action `git merge --ff-only c986f487`
succeeded (fast-forward from 20d7d33a, v6/ tree present).

Question: does the tsv2-generated tick engine (naive DELETE + recompute per
level rule, full-table snapshot reads, JS-side `multisetDiff`) have a path
onto the labbed store engine at `v6/sprefa-store/js/src/engine/` (cascade
Z-set weights, reconcile salsa-in-SQL, support tracking) instead of leaving
that machinery unused. Read-only research plus new files only; no edits to
`v6/sprefa-store/js/src/**`, `v6/prolog/compile/**`, or `v6/dl/src/**`.

## (a) The store engine's public surface, and how 3_runtime.ts actually drives it

Two DIFFERENT packages both live under `v6/sprefa-store/js/src/` and both get
called "the store engine" loosely. They are not the same machinery:

- **`engine/` (cascade + reconcile)** — a generic **liveness propagator** over
  opaque dense integer keys. `ICascadeApi`/`IRelStore`
  (`v6/sprefa-store/js/src/engine/types.ts:414-440`, `:167-209`) expose
  `insert_rows`, `insert_deps`, `assert`, `retract`, `retract_scc`,
  `retract_dred`, `alive`, `alive_keys`. `IReconcileApi` (`types.ts:477-501`)
  is salsa-in-SQL: `seed`/`mark_changed`/`dirty`/`verify`/`propagate` over
  digests. Read the actual bodies in `engine/engine.ts:258-334` (`retract`)
  and `:442-` (`assert`): every one of these functions takes a **seed set of
  already-known `(tag, id)` keys** and a **pre-existing `cx_dep` edge table**
  and does nothing but propagate a weight delta along edges someone else put
  there (`engine.ts:280-315` — hop from `${ns.frontier}` across `${ns.dep}`,
  decrement `${ns.row}.weight`, repeat until the frontier empties). There is
  no join, no predicate, no column value anywhere in this file — `cx_row` is
  `(key INTEGER PRIMARY KEY, weight INTEGER)`, `cx_dep` is
  `(parent_key, child_key)` (`engine.ts:203-222`). It cannot derive that
  `demanded(Target, SessionId)` follows from `open_feed(SessionId, Target)`;
  it can only tell you, once told which rows depend on which, whether a row
  is still supported.

- **`lower/lowerSql.ts` (`DatalogEvaluator`/`RecursiveStratum`)** — the actual
  join/derivation engine: a stratified, semi-naive SQL compiler over a
  `Program`/`Rule` AST (`lower/ast.ts`, `lower/rulegraph.ts`'s `stratify`/
  `scc`). `DatalogEvaluator.run()` (`lowerSql.ts:104-124`) does `DELETE FROM`
  every rule-headed IDB table (`clearStatements`, `:80-84`), then for each
  stratum either one `INSERT OR IGNORE ... SELECT ... FROM <body join>`
  per rule (`acyclicStatements`, `:86-95`) or a genuine semi-naive
  delta/next/promote loop for a recursive stratum
  (`RecursiveStratum.round`/`run`, `:418-450`, using `ALTER TABLE ... RENAME`
  to swap `_dl_delta_*`/`_dl_next_*`). This is real incremental-*within-a-
  stratum* evaluation (a recursive fixpoint doesn't rejoin the whole table
  every round), but it is **NOT incremental across ticks**: every call to
  `run()` clears every derived table and rebuilds from the current EDB state.

`v6/dl/src/3_runtime.ts`'s `applyDerivedTxn` (`:775-843`) is the tick's
fixpoint stage. It calls `evalProgramSql` (imported `3_runtime.ts:47`, i.e.
`lower/lowerSql.ts`, not `engine/`) at `:787-793` to recompute every derived
table from scratch when any EDB rel moved, THEN:
- `refreshFactPlane` (`:757-769`) mirrors the SETTLED table state into
  `cx_row` as `weight=1` upserts, keyed by `tag * stride + <surrogate>` — a
  one-way snapshot copy, not a computation.
- `diffAgainstTables` (`:679-694`) computes the tick's insert/retract sets
  by **JS-side `differenceWith` (lodash) against an in-memory
  `derivedTableMirror`** (`diffDerivedRel`, `:669-677`) — the exact same
  shape as tsv2's `multisetDiff`, just lodash instead of a JSON-key `Map`.
- `state.relStore.retract_dred` is called exactly once, in
  `retractThroughSupport` (`:1061-1079`), an on-demand "what died when I
  retract these EDB rows" explain query — driven by `supportEdges`, which is
  populated by `lowerSql.ts`'s `supportPlan` (`lowerSql.ts:162-226`, called
  from inside `evalProgramSql`'s `run()`, `:117-123`) doing its own
  `INSERT INTO cx_dep SELECT <join> ...` pass over the settled model.

Boot wiring: `RelStore.attach(db)` (`3_runtime.ts:979`) is called only to
name `relStore.ns().dep` for `supportEdges.table` (`:980-985`) and to hand
`ns().row` to `refreshFactPlane` — i.e. it is used as a **table-namespace
provider**, not as the tick's compute path. `engine.ts:32-36`'s own header
says this precisely: *"v6/dl's tick loop does NOT reach this namespace's
retract/assert. It runs its fixpoint through lower/lowerSql.ts... The cascade
retract and assert entry points are called only from
js/tests/engine/golden.test.ts and src/labs/stress.ts."*

**Conclusion for (a): the premise that 3_runtime.ts "rides" the count-IVM
cascade machinery for its core recompute does not hold.** It rides
`lowerSql.ts` (full DELETE+recompute per tick, semi-naive only inside one
stratum's recursive rounds) for derivation, and JS `differenceWith` for the
tick diff — the same complexity shape tsv2 already has. `engine/`'s cascade
is bolted on downstream, used only for a support-explain feature no tsv2
fixture or grading path exercises.

Already-real reuse (not aspirational): tsv2 already imports the store's
connection constructor and driver seam — `open_db`
(`v6/tsv2/runtime/scratchStore.ts:16,22-24`, importing
`sprefa-store-engine/src/engine/lib.ts:46-48`) and `SqlRunner`
(`scratchStore.ts:17,23`, `v6/tsv2/runtime/types.ts:20` re-exporting
`ISqlRunner`). `gen/demand_laziness_effect_rows.ts` calls
`seam.runner.batch`/`executeMultiple` (`:78,92`) through exactly this seam.
This IS store adoption, already landed; the import gate
(`v6/tsv2/scripts/check-imports.sh:27-33`) enforces it stays that way.

## (b) tsv2 statement-family -> store-call mapping

| tsv2 family (gen/*.ts) | Store call it could map to | Verdict |
|---|---|---|
| DDL (`CREATE TABLE ...`, hand-written, e.g. `demand_laziness_effect_rows.ts:40-44`) | `IRelTableApi.create_rel_table` (`spine.ts:227-250`) | Mechanically equivalent generator, but hides the literal SQL behind a function call — conflicts with the tsv2 reorientation law ("prolog EMITS literal TypeScript ... the real SQLite statements ... visible in the generated file", plans/2026-07-27-tsv2-compile-target-header.md). Also not identical output: `create_rel_table`'s no-surrogate path appends `WITHOUT ROWID` (`spine.ts:248`); current tsv2 DDL omits it. Not adopted; noted as a possible separate perf tweak (add `WITHOUT ROWID` to tsv2's own DDL text), unrelated to store adoption. |
| Arrival apply (`INSERT OR IGNORE`/`DELETE` against the arrival rel, `demand_laziness_effect_rows.ts:70-79`) | `ICascadeApi.insert_rows` / plain SQL | Arrivals are exact known rows (add/del signed by the caller) — no propagation needed, cascade buys nothing over the plain SQL already there. |
| Edge/keyed-replace write (`switch_as_keyed_replace.ts:122-149`, `ON CONFLICT(session_id) DO UPDATE`) | none needed | Already fully expressible as plain SQLite `ON CONFLICT` upsert against the rel's own PK. Store engine has no "keyed replace" primitive narrower than full-row identity; there is nothing to adopt here. |
| Level rule recompute (`DELETE FROM demanded; INSERT INTO demanded SELECT ... FROM open_feed`, `demand_laziness_effect_rows.ts:85-93`) | `lower/lowerSql.ts` `acyclicStatements`/`mergeStatement`/`RecursiveStratum` | The right SHAPE to imitate once tsv2 needs recursion (SCOREBOARD backlog has zero recursive fixtures today — top gap is unmarked edge triggers, 48 fixtures, not recursion). Calling `DatalogEvaluator` at runtime means interpreting a `Program`/`Rule` AST inside the generated program — exactly the "no AST/parser/lowering in TS on this path" the reorientation bans. Not adopted as a runtime call; its statement shapes are legitimate reference for what the prolog emitter should literally print once a recursive fixture exists. |
| Tick delta (`multisetDiff(before, after)`, `runtime/diff.ts:32-59`) | `ICascadeApi.assert`/`retract` + reverse key resolution | See gap (c) below — store has no facility to turn a changed key back into row VALUES; `3_runtime.ts`'s own `resolveFactKeys` (`:722-752`) has to do a per-rel SELECT to do this, which is at least as much work as the diff it would replace. |
| Support/explain (no tsv2 equivalent exists) | `IRelStore.retract_dred` + `supportEdges` | tsv2 has no "why did fact X die" feature and no fixture grades one; nothing to map onto. |

## (c) Gaps: where the store cannot express a tsv2 semantic

1. **No join/derivation capability at all.** `engine/engine.ts`'s cascade
   operates purely over pre-supplied `(tag,id)` keys and pre-supplied
   `cx_dep` edges (`engine.ts:169-256`, `:258-334`). It has no notion of a
   rule body, a variable, or a column. Every one of tsv2's "level rule" and
   "edge rule" semantics IS a join — cascade cannot compute one. Evidence:
   `insert_deps`/`assert`/`retract` all take the edge list or seed list as an
   argument (`types.ts:176-187,420-439`); nothing computes it.
2. **No key-to-row-value resolution.** Even if cascade correctly tracked
   which dense keys are alive after a change, turning a key back into a
   printable row (what the tick log needs) requires a per-rel `SELECT ...
   WHERE <surrogate> IN (...)` join back to the physical table — see
   `3_runtime.ts`'s `resolveFactKeys` (`:722-752`) and `selectSurrogates`
   (`:703-720`), both hand-written against `relbase_*`/`rel_*` tables, not
   store API. This resolution cost is comparable to (or more than) the
   full-table `multisetDiff` it would replace.
3. **Log-rel multiset occurrence diffs.** `cx_row.weight` is a single
   liveness/support counter keyed by ONE dense surrogate id per row
   (`engine.ts:203-208`: `key INTEGER PRIMARY KEY, weight INTEGER`). tsv2's
   Log rels need "N distinct arrival stamps for the same row value, each a
   separate occurrence" (`runtime/types.ts:91-95`,`IRelDelta.add`/`del` as
   full row lists, not counts; `runtime/diff.ts:1-14`'s own doc: "Log rels
   are append-only... 'count in next minus count in prev' ... without any
   separate stamp column"). Cascade's key model has no per-arrival stamp
   dimension; minting a fresh surrogate id per arrival is possible in
   principle but is new plumbing the store doesn't provide today.
4. **Keyed replace.** No store primitive narrower than full-row identity
   exists (`IRelStore`/`ICascadeApi` surface, `types.ts:167-209,414-440`) —
   already fully solved by plain SQL `ON CONFLICT` (see (b) table); not a
   real gap, just an absent feature that isn't needed.
5. **Boot/schema mismatch.** `IStore`'s 9 spine tables (`types.ts:259-296`,
   `spine.ts` entity types) are a FIXED code-graph schema (nodes/edges/spans/
   files/repos/revs) — unrelated to a tsv2 program's own arbitrary rel
   tables. Only the domain-agnostic `cx_row`/`cx_dep` pair
   (`IRelStore`/cascade) is generic enough to even consider reusing, and (1)
   above shows that reuse buys nothing without also reinventing the join
   layer. `runtime/scratchStore.ts:9-12`'s own header already records this
   choice ("`Store.open`/`create_all_tables` ... deliberately NOT reused").
6. **Minor, unrelated to adoption but worth flagging**: `engine/types.ts:37`
   documents the connection as `intMode:"bigint"`; the actual constructor
   (`engine/lib.ts:46-48`, `createClient({ url })`) sets no `intMode` at all,
   so the real default (`"number"`) applies. tsv2 independently verified
   this the hard way (`runtime/rows.ts:12-20`'s comment: "verified
   empirically, not assumed"). The store header comment is stale; harmless
   here because tsv2 never reads a bigint-typed column, but a future reader
   trusting `types.ts:37` would get it wrong.

## (d) Recommended adoption shape, ranked

1. **Keep what's already landed: `open_db` + `SqlRunner` reuse.** Real,
   working, zero gap (`scratchStore.ts:16-17,22-24`). No action needed.
2. **Do not adopt `engine/` cascade/reconcile for tsv2's tick fixpoint or
   diff.** It is structurally the wrong tool: it requires the join and the
   dependency graph to already be known (gap 1), and even a correct
   integration would still need the diff-shaped work it was meant to
   replace (gap 2). `v6/dl`'s own runtime does not use it this way either —
   it is downstream decoration for a feature (support-explain) tsv2 has no
   equivalent of. Forcing adoption here would add bookkeeping (register
   keys, register edges, mint surrogate ids, resolve keys back to rows)
   without removing the `multisetDiff`/full-recompute it was supposed to
   replace — pure overhead, no asymptotic or expressiveness win, for 109
   fixtures that are all small.
3. **Mine `lower/lowerSql.ts`'s statement SHAPES, don't call it.** Once a
   recursive tsv2 fixture exists (none do today — SCOREBOARD's top gap is
   unmarked edge triggers, not recursion), the prolog emitter's own literal
   SQL for a recursive stratum should look like `RecursiveStratum`'s
   delta/next/promote pattern (`lowerSql.ts:343-451`) — emitted as text by
   the prolog compiler itself, never invoked as a runtime `DatalogEvaluator`
   call, which would violate the "no AST/lowering in TS" reorientation law.
   This is a future reference, not an action item now.
4. **`reconcile`'s digest/early-cutoff machinery (`IReconcileApi`) is a
   plausible FUTURE win, not now.** It changes WHEN a recompute runs (skip
   on unchanged digest), never WHAT gets computed, so it cannot change a
   byte-identical tick-log grading result — nothing to prototype against
   the current grading loop. Worth revisiting only if/when tsv2 needs the
   v5 bench-parity-scale target (ledger item 6, unassigned), which is a
   different, unstarted track.
5. **Store-side additions (report-only, per the task's ownership split):**
   if the store package ever wanted to serve tsv2, the missing piece is a
   join-aware incremental engine — something that derives NEW rows from a
   delta to the body (real IVM over rule bodies), which neither `engine/`
   nor `lower/lowerSql.ts` provide today (`lowerSql.ts` recomputes each
   stratum from scratch on every call; `engine/` has no join at all). That
   is new engine-side work, not a wiring problem, and is out of scope for
   this agent (ownership boundary: no edits to
   `v6/sprefa-store/js/src/**`).

## Prototype: not attempted, blocked by (c)/(d) evidence above

The task asked for a prototype routing `demand_laziness_effect_rows`'s tick
through the store engine path, IF feasible without editing sprefa-store src.
It is not feasible in any way that demonstrates real adoption:

- The join (`demanded <- open_feed`, `effect_call <- demanded`) has to run as
  SQL regardless — cascade cannot compute it (gap c.1). That SQL is already
  what `recomputeLevels` does (`gen/demand_laziness_effect_rows.ts:85-93`);
  nothing changes there.
- The only way to route the DIFF/DELTA half through cascade would be to (i)
  register `open_feed`/`demanded`/`effect_call` rows as `cx_row` keys and the
  join's support edges as `cx_dep` rows every tick (itself a full-table
  `INSERT INTO cx_dep SELECT <join>` pass, same cost class as the recompute
  it decorates — this is literally what `lowerSql.ts`'s `supportPlan` already
  does for `v6/dl`), (ii) call `assert`/`retract` with correctly-computed
  seed sets (which requires knowing what changed — the very diff being
  replaced), and (iii) resolve the resulting alive/dead keys back to row
  values with a per-rel `SELECT ... WHERE surrogate IN (...)` (gap c.2). Net
  result: strictly more code and more statements than the current
  `multisetDiff`, with no correctness or performance improvement
  demonstrable on a 3-table, 2-rule fixture, and no plausible way to show a
  win without inventing new incremental-join machinery this agent is not
  authorized to add to the store package.
- Building that new machinery inside `v6/tsv2/` instead (to avoid touching
  store src) would mean re-implementing a real incremental join engine from
  scratch in a "small prototype" — exactly the shape the standing
  build-vs-buy law asks to be justified in writing before any bespoke line
  lands, and there is no library or existing sprefa component that closes
  this gap today.

No new runtime/gen files were written for this task; the honest output is
this findings doc. If a future fixture needs support-explain (tsv2 grows a
"why did X die" feature) or genuine tick-to-tick incrementality at scale
(the v5 bench-parity track), that is the point to revisit `engine/`'s
cascade and `IReconcileApi` respectively — not before.

## Validation run (baseline, unchanged by this task)

- `cd v6/tsv2 && pnpm test` — 6/6 pass (`multisetDiff` x3,
  `demand_laziness_effect_rows` oracle + perturbed schedule,
  `switch_as_keyed_replace` oracle incl. drain tick).
- `cd v6/tsv2 && bash scripts/check-imports.sh` — `import gate: OK (2 gen
  files, 6 runtime files)`.
- `cd v6/tsv2 && bash scripts/sweep.sh` — stage 1: `SWEEP total=110
  compiled=31 unsupported=79 crash=0`; stage 3: `RUN total=31 identical=28
  wrong=0 run_error=2 no_oracle_log=1` (the 2 run_errors and 1 no-oracle-log
  are expected engine-rejection fixtures, not regressions). `wrong=0`
  confirms conformance is green at 110 fixtures, unchanged by this
  read-only investigation.
