# The tsv2 compiler pipeline, stage by stage

The dl-to-TypeScript (and later rust) compiler in this directory, written down
the way it actually runs. Refs are `file:predicate/arity` anchors rather than
raw line numbers because the code is under active widening (phase C2) and
predicate names are the stable, mechanically checkable handle. INTENDED
UPGRADE (user, 2026-07-28): these refs become CHECKED refs, the pattern used
for the architecture diagrams here and in ~/projects/instant and
~/projects/kneeman -- a rail asserts every `file:pred/arity` in this doc
resolves against the source, so the doc cannot silently rot. Until that rail
lands, the anchors are hand-maintained; state as of the reconciliation-complete
commit (898ebcce).

## Shape of the whole thing

```
fixture term          plan/6              lowered/8               file
prog(Decls, Rules) -> analysis owned  ->  SQL text + structure -> gen TS
                      by compile.pl       (target-neutral)        (one backend)

  stage 1 read        stage 2 analyze     stage 4 lower           stage 5 emit
                      stage 3 order
                                          stage 6 grade: tick-log diff vs oracle
```

Rule of the design: **prolog makes every decision; the emitted file holds the
decisions.** The generated .ts contains no cleverness -- it is the compiler's
reasoning serialized readably. That is also why a rust backend is only a second
printer: same `lowered/8` term, different rendering (compile.pl header, the
backend-pluggable note).

## Stage 1 -- read the program (compile.pl)

- `compile.pl:read_fixture_term/4` reads the fixture with raw `read_term` +
  `variable_names`, NEVER consult. Surface variable names (`Target`,
  `SessionId`) survive as live variable objects, and same-spelled variables in
  one clause are the same object -- column naming (stage 2) is mined from that
  identity. Consult loses it the moment the reader returns.
- `compile.pl:find_fixture/4` replays each file's own `:- op(...)` directives
  so `<-`/`<+`/`:=` parse, exactly what consult would have done.
- `compile.pl:program_plan/2` builds `plan(Name, Prog, RelPlans,
  ArrivalTargets, RuleOrder, EdgeRules)` once; lower and emit are pure
  functions of it.

## Stage 2 -- analysis (analyze.pl)

- `analyze.pl:rel_kind/3`: log if declared, else set; `keyed` implies set.
  Mirrors the reference engine's unexported predicate.
- `analyze.pl:declared_refs/2`: unions kind/keyed/keep decls so a rel with
  zero rule readers still gets a table + arrival handling (phase C sweep
  found EDB-only fixtures with an empty Rules list).
- `analyze.pl:body_ref_uses/2`: every body atom becomes
  `use(Ref, Args, pos|neg, marked|unmarked)`; one tuple shape feeds
  stratification, column mining, and lowering. `not/1` flips sign; `only/1`
  marks.
- `analyze.pl:rel_columns/4`: per argument position, the first occurrence
  whose argument is `==`-identical to a surface binding names the column
  (snake_case). TRAP recorded in the source: findall/bagof copy_term their
  template and sever variable identity; the walk backtracks over the original
  term instead.
- `analyze.pl:check_supported_subset/1`: the gate. Anything the lowering
  cannot do honestly throws `unsupported_construct(NamedThing)`. History
  matters here: comparisons, `:=` binds, and head arithmetic used to compile
  SILENTLY WRONG (filters vanished; arithmetic stored as an unevaluated json1
  tree) -- the phase C sweep converted them to named refusals. Refusal over
  guessing is the gate's law.

## Stage 3 -- ordering (strat.pl)

Two orderings answering different questions:
- `strat.pl:stratum_groups/2` reproduces the reference engine's own
  stratification (relax gap algorithm: head >= body, strictly greater under
  negation) for parity reporting.
- `strat.pl:sql_rule_order/2` answers what the engine never asks: generated
  SQL runs each rule ONCE per tick, so within a stratum group rules get a
  Kahn topological sub-order over positive edges. A genuine positive cycle
  throws `unsupported_construct(recursive_stratum(...))` rather than emitting
  a wrong single-pass order.

## Stage 4 -- lowering (lower.pl), target-neutral by construction

Output term: `lowered(Name, Ddl, ArrivalStatements, EdgeStatements,
LevelStatements, DeltaStatements, RelPlans, ArrivalTargets)` -- SQL text plus
plain structure, zero host-language idiom (the rust directive lives here).

- DDL, `lower.pl:rel_ddl/3`: log rels = plain rowid tables (duplicate rows
  are distinct occurrences); set rels = WITHOUT ROWID with PK over ALL
  columns -- EXCEPT an edge-headed keyed rel, whose PK is the key columns
  alone because its upsert's `ON CONFLICT(key)` must name a real constraint
  (found as a live SQLite error during reconciliation, not by analysis).
- Arrivals, `lower.pl:arrival_statement/2`: INSERT/DELETE templates with `?`
  placeholders. Keyed decls do NOT alter arrivals -- the engine's
  absorb-arrivals path never consults keys; only edge writes do.
- Edge rules, `lower.pl:edge_statement/3`: per rule, ProjectSql (one arrival
  row bound to `?1..?N` in trigger-arg order, projected into the head row
  shape) + UpsertSql (`INSERT ... ON CONFLICT(key) DO UPDATE SET col =
  excluded.col`). Keyed-replace and last-write-wins resolve in the backend
  from the arrivals array. HISTORY: round 1 threaded a tick number through
  everything; the real runtime seam provides none (the runtime owns tick
  numbering), which forced this whole shape -- recorded in the file header as
  the round-2 finding.
- Level rules, `lower.pl:level_statement_groups/3`: per head, DELETE once
  then one INSERT-SELECT per clause; adjacent same-head clauses grouped so a
  second clause does not wipe the first (sweep-found bug).
- Deltas, `lower.pl:delta_statement/2`: one read-every-row SELECT per rel;
  the runtime diffs before/after snapshots with its own multisetDiff (reused,
  never reimplemented) -- one algorithm covers set diff and log
  occurrence-count diff. `lower.pl:canonical_column_expr/2` renders a stored
  json1 compound back to canonical term text (`route_data(settings)`) at read
  time, per the tick-log envelope pin; storage encoding stays the compiler's
  own business.
- Boot, `lower.pl:boot_statements/3`: parameterized INSERTs for Initial rows,
  emitted as the `boot` field beyond the five pinned IGenProgram names; the
  reconciliation runner executes it after DDL, before tick 1.

## Stage 5 -- emission (emit_ts.pl)

The one backend. Walks `lowered/8` and prints a flat, machine-regular module:
DDL strings, arrival templates, the level SQL in execution order, the
snapshot SELECTs, a four-step rx tick (snapshot -> arrivals -> levels ->
snapshot+diff), and `export const program: IGenProgramWithBoot` conforming to
the coordinator-pinned seam (v6/tsv2/runtime/types.ts). A future emit_rust.pl
plugs in via `compile.pl:compile_fixture/4`'s explicit emitter argument.

## Stage 6 -- grading

- `../conformance/ticklog.pl` prints the reference engine's per-tick deltas in
  the shared JSONL envelope.
- `v6/tsv2/scripts/run-emitted.ts` runs an emitted module on the phase A
  runtime (DDL, boot, then the tickLoop fold) printing the same envelope.
- `diff` of the two outputs IS the grade; byte-identity or nothing.
- `v6/tsv2/scripts/sweep.sh` runs that loop over the whole fixture corpus and
  writes SCOREBOARD.md (buckets: IDENTICAL / UNSUPPORTED named / WRONG).

## Worked example (smallest fixture, demand_laziness_effect_rows)

```
input   prog([keyed(open_feed/2, [1])],
             [(demanded(Target, SessionId) <- open_feed(SessionId, Target)),
              (effect_call(Target) <- demanded(Target, _))])

stage 2 columns: open_feed = [session_id, target] (mined from the surface
        variable names); arrival target: open_feed (never a rule head)
stage 3 order: demanded before effect_call (Kahn, positive edge)
stage 4 level SQL:
        INSERT OR IGNORE INTO "demanded" ("target", "session_id")
        SELECT b0."target", b0."session_id" FROM "open_feed" b0
stage 5 out/demand_laziness_effect_rows.ts: ddl block, ARRIVAL_STATEMENTS,
        recomputeLevels (the SQL above, visible), buildDeltas via
        multisetDiff, runTick pipe, exported program object
stage 6 5 log lines, byte-identical to the oracle, including a perturbed
        schedule run proving it computes rather than replays
```
