# TypeScript lowering review (tsv2 emitted code, runtime, serve, cli)

Lane `lane/ts-lowering-review`, base `22c0c9f71ca6b16e848c53f8980f4b0c6e3d6ecd`.
Read-only review. Nothing outside this file was edited. Every probe ran hermetic
(`SPREFA_CONFIG=/nonexistent/x.toml DL_NO_DAEMON=1`, `:memory:` db, port 0 read
back from `server.address()`); no daemon and no `~/.local/state` was touched.

Scope read: `v6/tsv2/gen_emitted/*.ts` (137 modules, 45,869 lines),
`v6/tsv2/runtime/` (2,552 lines), `v6/tsv2/serve/` (1,933 lines),
`v6/tsv2/cli/` (404 lines), `v6/prolog/compile/emit_ts.pl` (1,993 lines).

Baseline health at this sha, run by me:

| gate | result |
| --- | --- |
| `just one-subscribe` | exit 0, `dl/src` 1/1, `tsv2/serve` 1/1 |
| `npm test` (tsv2) | 99 tests, 98 pass, 1 skip, exit 0 |
| `just memory-soak` | MEMORY SOAK HOLDS, exit 0 |
| `npx tsgo --noEmit` | **exit 1, 4 errors** |

---

## PROVED findings, ranked

### 1. CRITICAL: one tick fault permanently kills the engine and the served process

`LiveEngine.ticks$` is a single `concatMap` lane over one `Subject`
(`serve/3_engine.ts:104,113-124`). `runBatch`'s error arm reports the failure to
the submitter and then **re-throws into that shared lane**:

```ts
// serve/3_engine.ts:176-179
catchError((failure: unknown) => {
  queued.subscriber.error(failure);
  return throwError(() => failure);
}),
```

An error inside `concatMap` terminates the outer observable. `ticks$` is dead
from that moment; the `tap({ finalize })` at `:119-121` flips `running` false, so
every later `submit` fails at `:130` with "tsv2 engine is not running". There is
no recovery path. `runProgram$` merges `ticks$` into the app graph
(`serve/4_http.ts:164`), so the error propagates to `serveTsv2`'s single
subscriber and `serve/main.ts` exits.

Direct engine-level receipt (probe against `LiveEngine` with a subscribed
`ticks$`, no HTTP involved):

```
1 good  : OK 1 tick(s)
2 bad   : THREW arrival.row.map is not a function
   ticks$ errored: true | completed: false
3 good  : THREW tsv2 engine is not running: nothing subscribes ticks$
```

This is not specific to bad input. Any fault from `program.tick` (a SQLite
constraint violation, a disk-full, a decode fault in a generated module) is a
permanent kill. `serve/4_http.ts:315-316` states the opposite law in its own
comment ("one bad request cannot end the app"); `routeRequest$`'s `catchError`
at `:346` does hold for faults that stay above the engine, and cannot hold for
faults that reach it.

Nothing in the suite covers this: no test submits a batch that faults inside a
tick. `serve-endurance` and `serve-leak-soak` both exercise only well-formed
traffic.

### 2. CRITICAL: the HTTP arrival boundary validates lengths, never shapes

`serve/4_http.ts:220` trusts the wire:

```ts
const batch = (JSON.parse(text) as { readonly batch?: readonly IArrivalRow[] }).batch ?? [];
```

`batchProblem` (`:200-210`) then checks three things: rel is an arrival target,
sign is `add`/`del`, and `arrival.row.length` equals the declared width. It never
checks that `row` is an array, that its elements are `IRowValue`, or that
`arrival` is an object.

The emitted second line of defence, `validateArrivals` (identical in 136 of 137
modules, `emit_ts.pl:503`), only branches on `"bool"` and `"float"`. Every
`text`, `int`, and `ref` column falls through with a bare `return value`.

Probes, each against a fresh server, program
`rel note(name: text, body: text). rel echoed(...). echoed(n,b) <- note(n,b).`:

| payload | reply | server after |
| --- | --- | --- |
| `row: "ab"` (2-char string, 2-column rel) | 500 `arrival.row.map is not a function` | **DEAD, ECONNREFUSED** |
| `row: [{a:1},[2,3]]` | **200**, tick log `{"tick":1,"deltas":{}}` | alive, corrupted |
| `batch: 5` | 500 `batch is not iterable` | alive |
| body `{{{` | 500 (JSON parse text) | alive |
| `batch: [null]` | 500 `Cannot read properties of null` | alive |

The first row is finding 1's trigger. The second row is worse in kind: it
returns 200, the tick log reports **zero deltas**, and the row is nevertheless
in the table with a NULL in a `TEXT NOT NULL` column, where it persists and
pollutes every later read:

```
B post: 200 {"ticks":[{"tick":1,"line":"{\"tick\":1,\"deltas\":{}}"}]}
note  : {"rows":[[null,"[2,3]"]]}
echoed: {"rows":[]}
   ... after a later well-formed arrival:
note  : {"rows":[["x","y"],[null,"[2,3]"]]}
```

The tick log is the cross-target grading contract. A path that stores a row and
prints an empty delta line breaks that contract silently. Three of the five
probes also leak a raw JS `TypeError` message as the 500 body.

### 3. HIGH: `{col}` host template splice is unescaped shell injection

`serve/1_hosts.ts:143-147` splices a row value into the command text by string
replacement, and `:157` runs it with `spawn(commandLine, [], { shell: true })`.
No quoting, no escaping, no opt-in safe mode.

Probe: host `sh look(name: text) -> (out: text) = \`printf '%s\n' '{name}'\``,
one ordinary arrival whose value closes the quote the template put it in.

```
value posted : "x'; touch /var/.../INJECTED; echo '"
marker file created by the host shell: true
seen rows    : {"rows":[["x'; touch /var/.../INJECTED; echo '"]]}
```

The host inputs of every real program are file paths and globs read off disk.
`scripts/crawl-bench.sh:128` uses exactly this shape (`ls-files -- '{glob}'`),
and the probe shows the single quotes are not a defence. `$col` env expansion
(the other spelling, same file) is safe; `{col}` is not, and the emitter accepts
both without distinction.

### 4. HIGH: host subprocesses run one at a time, by construction

`serve/1_hosts.ts:441-445` composes invocations with `concatMap` inside
`concatMap`:

```ts
concatMap((batch) =>
  from(groupInvocations(batch)).pipe(
    concatMap((invocation) => this.runInvocation(invocation)),
  ),
),
```

Measured with a host that sleeps 200ms, on an M2 Pro:

| demands | wall | serialized would be | concurrent would be | effective concurrency |
| ---: | ---: | ---: | ---: | ---: |
| 4 | 871ms | 800ms | 200ms | 0.92 |
| 8 | 1723ms | 1600ms | 200ms | 0.93 |
| 16 | 3381ms | 3200ms | 200ms | 0.95 |

Concurrency is 1, flat. There is no knob. This is the structural explanation for
the repo's own `CRAWL-BENCH.md` number: 779 files at **40.68 files/s** against
v5's **3,540.93 files/s measured in the same run** (87x), and 7,244 files/s
historical (178x). Every one of those 779 files is a separate `sprefa-extract`
subprocess taken strictly in turn on a 12-core machine.

`groupInvocations` (`:393-412`) already does the applicative collapse for
`sprefa_extract` witnesses that share a command; it cannot help here because
distinct paths are distinct commands.

### 5. HIGH: the emitted ordered-occurrence (`pre`) family is N+1, and has no count test

Two emitted execution families, measured on the same probe (fresh `:memory:`,
one tick, `stmt_counter` delta):

| program | family | 1 arrival | 5 | 25 | 100 |
| --- | --- | ---: | ---: | ---: | ---: |
| `comparison_filters_rows` | incremental | 31 | 31 | 31 | 31 |
| `batched_increments_both_count` | ordered/pre | 15 | 23 | 63 | **213** |
| `counter_fold_matches_hand_computation` | ordered/pre | 30 | 38 | 78 | **228** |

Both ordered programs are exactly `constant + 2n`. The cause is emitted verbatim
by `emit_ts.pl:1360-1372`: `processOrderedOccurrences` reduces the occurrence
list into a sequential `concatMap` chain, and `applyOrderedOccurrence`
(`emit_ts.pl:1333`) runs one `seam.runner.execute` per occurrence. 49 of 137
modules also carry the narrower per-arrival shape
`forkJoin(triggerRows.map((arrival) => seam.runner.execute(...)))`
(`emit_ts.pl:949`, `:1011`).

On top of the per-row loop, `snapshotOrderedPre`
(`gen_emitted/batched_increments_both_count.ts:214`, called unconditionally at
`:388`) does `DELETE FROM "__pre_counter"; INSERT ... SELECT * FROM "counter"`
every tick: a **full copy of the whole relation per tick**, independent of
arrivals.

The standing law is that formerly-quadratic paths get COUNT or EXPLAIN
assertions rather than end-state equality. `tests/orderedPre.test.ts` asserts
neither: it exercises the `stageOrderedFrontiers` runtime helper for row content
only. The seven files carrying `EXPLAIN QUERY PLAN` assertions
(`aggregateScope`, `7_value-plane`, `edgeGuard`, `relationDepth`,
`departureFrontier`, `levelFreeze`, `structPlane`) and the three carrying
statement-count assertions (`watchCounts`, `retentionCount`, `serveLeak`) all
sit on the incremental family. The law is held exactly where someone already
looked and nowhere else, and the one place it is unmet is the one place the
shape is per-row.

ARCH row `pre_occurrence_loop` already names the shape as owed work. What is new
here is the measured curve and the absent gate.

### 6. HIGH: the package does not typecheck, and no gate runs the compiler

```
$ npx tsgo --noEmit -p tsconfig.json ; echo $?
tests/relationDepth.test.ts(205,28): error TS2345: ...
tests/relationDepth.test.ts(230,28): error TS2345: ...
tests/relationDepth.test.ts(255,28): error TS2345: ...
tests/relationDepth.test.ts(...): error TS2345: ...
1
```

Four errors, all the same cause: a local structural parameter type writes
`insertSql: string` where `IIncrementalLevelStatement.insertSql` is
`string | null` (`runtime/types.ts:182`, null exactly when `aggregateSql` is
present).

`package.json` has a `typecheck` script. `v6/justfile`'s `green` and `green-all`
do not call it, and no other recipe does (`grep -n "tsgo\|tsc\|typecheck"` over
both justfiles: nothing). `npm test` runs `node --test
--experimental-transform-types`, which strips types without checking them.

This is worse than four stale errors. Several standing laws in this repo exist
*because* the compiler checks them ("TypeScript cannot conformance-check a
standalone function against anything"). A compiler that never runs in the gate
cannot enforce any of them. `tsconfig.json` is otherwise exemplary (`strict`,
`noUncheckedIndexedAccess`, `noImplicitOverride`), which makes the missing gate
the whole of the gap.

### 7. MEDIUM: eight free `export function`s carrying real contracts, none in the header

The law: important functions bind to an `I`-prefixed header interface, because a
bare `export function` can drift from its documented signature silently.

| site | what it is |
| --- | --- |
| `runtime/diff.ts:32` `multisetDiff` | the boundary-diff algorithm every emitted module calls |
| `runtime/rows.ts:37` `selectRows` | the read seam, used by `LiveEngine.rows` and emitted code |
| `runtime/rows.ts:22` `rowValueFromSql` | driver-seam value decode |
| `runtime/1_incremental.ts:147` `stageOrderedFrontiers` | called by 11 emitted modules |
| `serve/3_engine.ts:215` `bootServedProgram` | the whole boot contract |
| `serve/1_hosts.ts:620` `witnessRows` | endurance receipt read |
| `serve/2_binds.ts:188` `watchRootOf` | glob-to-root resolution |
| `serve/2_binds.ts:410` `bindPlansFor` | executor split |

`grep -c` for each name in `runtime/types.ts`: **0 for all eight**, plus
`RowDiff` (the return type of `multisetDiff`) is also absent.

`stageOrderedFrontiers` is the sharpest: it sits in the same file as
`export const IncrementalRuntime: IIncrementalRuntime` and does the same class of
work, and `IIncrementalRuntime` (`runtime/types.ts:212-269`) lists 12 members and
not this one. The ten interface-bound namespace objects and five
`class ... implements` in the same trees show the pattern is understood; these
eight are the leak.

### 8. MEDIUM: the emitter re-declares header types into all 137 modules

Every emitted module carries its own copies of types `runtime/types.ts` already
declares exactly once:

```ts
// gen_emitted/comparison_filters_rows.ts:42-53, emitted by emit_ts.pl:326-336
interface IHostColumnPlan { ... }        // header: types.ts:422
interface IHostPlanData { ... }          // header: IHostPlan, types.ts:434
interface IBindPlanData { ... }          // header: IBindPlan, types.ts:458
interface IQueryPlanData { ... }         // header: IQueryPlan, types.ts:465
interface IBootStatement { sql: string; params: readonly IRowValue[] }  // header: types.ts:349
type IGenProgramWithBoot = IGenProgram & { ... }  // header: IServedProgram, types.ts:477
```

The header law says each name is declared once. Six are declared 138 times. The
emitted `IBootStatement` has already drifted: the header's `sql` is `readonly`,
the emitted one is not.

Byte-identical duplication across `gen_emitted/`, measured by brace-matched
block extraction and md5:

| block | copies | lines each | duplicated lines |
| --- | ---: | ---: | ---: |
| `validateArrivals` | 136 | 19 | 2,565 |
| `triggerOccurrences` | 49 | 18 | 864 |
| `applyArrivals` | 137 | 4 | 544 |
| `IBootStatement` | 137 | 4 | 544 |
| `applyOrderedOccurrence` | 11 | 39 | 390 |
| `orderedPreWriteStatement` | 11 | 17 | 170 |
| `processOrderedOccurrences` | 11 | 12 | 120 |
| `readOrderedCarry` | 11 | 11 | 110 |
| the six type declarations above | 137 each | 1 | 816 |
| others (`quoteOrderedIdentifier`, `advanceTick`, …) | | | ~443 |
| **total** | | | **6,566 of 45,869 = 14.3%** |

That undercounts: `runTick`, `runNaiveTick`, `runIncrementalTick`,
`recomputeLevels`, `readSnapshot`, `buildDeltas` and `arrivalStatement` appear in
all 137 modules and differ only by table names, so they are near-duplicates the
md5 test does not catch.

### 9. MEDIUM: emitted code builds SQL text at runtime, per row

`emit_ts.pl:1264-1284` emits an identifier quoter and a statement builder into
the generated file:

```ts
function quoteOrderedIdentifier(identifier: string): string { ... }
function orderedPreWriteStatement(write: IOrderedWrite): SqlStatement | null {
  const table = quoteOrderedIdentifier("__pre_" + arm.headRel);
  const columns = arm.headColumns.map(quoteOrderedIdentifier);
  const placeholders = columns.map(() => "?").join(", ");
  ...
  return { sql: "INSERT INTO " + table + " (" + columns.join(", ") + ") ...", args: bindArgs(row) };
}
```

The result is a function of `arm` alone. `arm` is compile-time data
(`ORDERED_EDGE_ARMS` is a literal array in the same file, and already carries
`projectSql` and `writeSql` as emitted constants). So the module re-derives, on
every written row of every tick, a string the emitter could have written once
into the arm record as `preWriteSql`. String concatenation in a loop, in
generated code, over a value that never varies.

This is exactly the axis the P0 emitter lab graded
(`plans/2026-07-28-emitter-p0-lab-verdict.md`: four statement families, inline
versus one-shared-helper, verdicts MIXED / HELPER / MIXED / HELPER). That lab
graded *which SQL statement families* to inline. It did not grade the split of
*algorithm versus data* in the surrounding TypeScript. The extension the lab did
not cover: `quoteOrderedIdentifier` + `orderedPreWriteStatement` +
`applyOrderedOccurrence` + `processOrderedOccurrences` + `readOrderedCarry` are
556 lines of byte-identical algorithm emitted 11 times whose only per-program
input is three data arrays that are already emitted separately. That is a
runtime helper wearing generated-code clothes, and the P0 verdict's own criterion
(inline when the SQL text is program-specific, share when it is not) says so.

The mirror image also exists, and is smaller: `runtime/1_incremental.ts:26-47`
carries `quoteIdentifier`, `bindArgs`, `resultRows`, `valuesSql` while the
emitter emits its own `bindArgs` into all 137 modules. Two implementations of the
same bigint-coercion rule, in the two places where a divergence would be silent.

### 10. MEDIUM: unbounded per-process witness set

`serve/1_hosts.ts:417` `private readonly claimed = new Set<string>()`. It is
added to at `:504` and there is no `delete` anywhere in the file (`grep -n
"claimed"` returns exactly lines 19, 417, 503, 504). One entry per
`(host, witness_digest)` for the process lifetime. A long crawl or a watcher
feeding extraction over days grows it without bound.

`just memory-soak` cannot see this: the soak program declares no hosts. Its
receipt (rss second-quarter 188MB, final-quarter 143MB, page count 10 flat, 37
statements per tick flat) is real and holds, and it is silent about the host
plane.

### 11. MEDIUM: N+1 writes in the host settle path

The N+1 law says never a per-row write; collect the set and write once. The
incremental runtime obeys it (`seam.runner.batch` at `1_incremental.ts:139, 178,
351, 503, 1044`). The host runner does not:

- `serve/1_hosts.ts:584-588`: `from(demands).pipe(concatMap((d) =>
  WitnessCache.claim(...)), toArray())` is one `INSERT` per demand, serialized.
- `serve/1_hosts.ts:609`: `concatMap((projection) =>
  this.settleProjection(projection, ...))` is one `INSERT ... ON CONFLICT` per
  projection, serialized.

For a grouped `sprefa_extract` invocation covering N witnesses, that is 2N
round trips where `seam.runner.batch` is already imported and used one directory
away.

### 12. MEDIUM: the import gate does not scan `cli/`

`scripts/check-imports.sh` loops over `gen/*.ts`, `gen_emitted/*.ts`, `serve/*.ts`
and inspects `runtime/*.ts` import lines. `cli/*.ts` is not scanned at all, so
`bop.ts` could import `@libsql/client` or `../../dl/` and the gate stays green.
`bop.ts` is a shipped app (`package.json` `"bin": { "bop": "./cli/bop.ts" }`),
not a script. The gate's own comments record two prior sabotage probes that
found holes of exactly this shape; this is the third.

Same shape in the one-subscribe ratchet: `v6/tools/one-subscribe.sh:49-50` checks
`dl/src` and `tsv2/serve`. `cli/bop.ts` holds two `.subscribe()` calls (`:132`,
`:180`) in one file. The file's header states the exemption honestly and the
reasoning (two standalone processes) is sound, but the count is unratcheted, so a
third one lands silently.

### 13. LOW-MEDIUM: five `toArray()`-then-discard sites

The stated symptom of crossing the sync/async line is a pipeline that ends by
throwing its values away. Present at `runtime/2_boot.ts:40`,
`serve/3_engine.ts:223`, `serve/1_hosts.ts:471`, `:588`, `:606`.
`serve/4_http.ts:226` is the legitimate one (the array is the response body).

`serve/3_engine.ts:217-224` is the clearest instance: `from(statements).pipe(
concatMap(execute), toArray(), concatMap(() => BootRunner.run(...)))`. Every
`rowsAffected` the seam was widened to carry is collected and dropped. These are
sequencing, not computation, and the repo's own history (8 redundant `SELECT
count(*)` scans per conformance run) is what the widening was for.

### 14. LOW: type-quality residue

Genuinely good: **zero `any`** in `runtime/`, `serve/`, `cli/`, `gen_emitted/`
(the five grep hits are a relation literally named `any_diag`). 24 non-null
assertions total across all four trees, all index reads under
`noUncheckedIndexedAccess`.

The lies that remain:

- `cli/bop.ts:315` `Readable.fromWeb(response.body as never)` is a cast to
  `never` to paper a Web-versus-Node stream mismatch. `as never` accepts
  anything; if the shape is wrong this fails at runtime with no compiler help.
- `cli/bop.ts:274` `JSON.parse(text) as { readonly rows: readonly IRow[] }`
  followed immediately by `for (const row of parsed.rows)`: an unexpected body
  gives an unhandled rejection inside an async `.then` callback, not a message.
  Already filed; the same unchecked shape recurs at `:236`, `:265`, `:290`.
- `cli/bop.ts` `Number(options.port)` on every verb: `--port abc` yields NaN with
  no named error.
- The remaining casts in `runtime/` and `serve/` are at the two seams where the
  API genuinely hands over `unknown` (`row[column] as IRowValue` at the SQLite
  driver, `message as TickEvent` at `diagnostics_channel`). Those are correct.

### 15. DOC: `v6/tsv2/SCALE.md` carries pre-P1 numbers

The table records tsv2 s1/100k at **177,093ms** and s2/100k at **183,068ms**, and
its preamble says "Both engines recomputed the Datalog result per tick". The
ledger records the P1 incremental emitter landing those cells at 2.1s and 1.1s
(84x and 165x). A reader reaching SCALE.md today reads the naive numbers as
tsv2's current standing.

---

## SUSPECTED

- **Finding 1 generalizes to every fault class.** Proved for an input-shape
  fault. The code path (`concatMap` + rethrow) is class-independent, so a SQLite
  `SQLITE_BUSY`, a `database or disk is full`, or a decode fault in a generated
  module should behave identically. Not separately probed.
- **The host `claimed` set is one of several unbounded per-process structures.**
  Only `claimed` was grepped to exhaustion. `GlobWatch` state in
  `serve/2_binds.ts` holds a digest map per glob whose growth was not measured.
- **`snapshotOrderedPre`'s full-table copy makes the pre family quadratic in the
  relation, not just linear in arrivals.** The per-tick copy is proved
  (unconditional call at `gen_emitted/...:388`); the cost curve against relation
  size was not measured, because the fixtures are small.
- **`parseWhitespace` (`serve/1_hosts.ts:293-304`) has a third ambiguity beyond
  the two its comment names.** The comment states the grid-versus-per-column
  case honestly. A host whose output contains a line with the right field count
  and another with the wrong one falls to the third branch and pads with `""`,
  which coerces to a stored empty string rather than a named refusal. Not
  probed.

---

## What is good and should be protected

- **`runtime/types.ts` is the best file in the package.** 754 lines, no bodies,
  every interface carrying the receipt for why it has the shape it has
  (`IBootRunner.run`'s bigint note cites `tests/bootBind.test.ts` by name;
  `IServeStats` cites the empirical dbstat check against @libsql 0.17.4;
  `IIncrementalRelationPlan.departureFrontierTableName` explains why it is
  optional). The pinned-five-fields contract on `IGenProgram` is doing real work.
- **Zero `any`, and `strict` + `noUncheckedIndexedAccess` on.** The 24 non-null
  assertions are the price of that config, correctly paid.
- **The driver seam is genuinely singular.** No `@libsql` import exists anywhere
  in `runtime/`, `serve/`, `cli/`, or `gen_emitted/` except the store's own
  re-export through `scratchStore.ts`. The 0_trace seam gap named in the brief is
  a `v6/dl` gap; tsv2 has no equivalent. `SqlRunner` is one `defer(() =>
  from(db.execute(...)))` and nothing else.
- **Zero `console.*`** across all four trees. Diagnostics go through
  `diagnostics_channel` + pino, off by default, one JSONL line per tick, with
  `hasSubscribers` checked at every publish site. `serve/0_trace.ts:65` folds
  effects, binds and watches into the tick line rather than emitting per event:
  the N+1 law applied to telemetry, which is rare.
- **The incremental emitter family's flat statement counts are real.** 31 per
  tick at 1, 5, 25 and 100 arrivals, measured, unprompted by any test I ran.
- **The emitted DDL is well formed for the planner.** Key tables are
  `WITHOUT ROWID` with the PK on the declared key columns; every `__delta_*` gets
  an index on `_sign` and every `__frontier_*` an index on `_phase`, emitted
  beside the table. The `__pre_*` tables are `WITHOUT ROWID` on the key too.
- **The failure comments are unusually honest and specific.** `serve/1_hosts.ts`
  names `bug host_grid_answer_folded` and describes the "right at one file, right
  at three, wrong at two" symptom. `serve/4_http.ts:367-379` names
  `serve_lifecycle_idb_read_race` and states the old ordering that caused it.
  `scripts/check-imports.sh` records two sabotage probes that found holes in
  itself. This is the repo's real asset and it should not be traded for brevity.
- **Test hygiene.** Ephemeral ports everywhere (`reservePort` binds 0 and reads
  it back; `serveHelpers.ts:59-74` records why the previous hardcoded constants
  were wrong), `:memory:` databases, injected `SchedulerLike` and `IWatchSource`
  so no test sleeps on a wall clock or a real filesystem.
- **`just memory-soak` is a real gate with a real sabotage receipt.** It fails
  red under `keep_all` (page count 17 to 33 against a ceiling of 19), which is
  what makes its green meaningful.

---

## The single thing to fix first

**Finding 1.** Stop `LiveEngine.runBatch` re-throwing into the shared `ticks$`
lane. The submitter already gets the error through `queued.subscriber.error`; the
lane should absorb it and keep turning. One line changes from
`return throwError(() => failure)` to `return EMPTY`, plus a decision about
whether the app graph should see a `{ kind: "fault" }` event, plus a fail-first
test that submits a faulting batch and asserts the next good batch still ticks.

It ranks first not because it is the largest but because it is the amplifier:
every other fault in this document (a malformed row, a constraint violation, a
future emitter bug) is currently a process kill rather than a 500, and no gate
in `green-all` would notice.

Second, because it is nearly free: wire `pnpm typecheck` into `just green`
(finding 6), so the four standing errors get fixed and the laws that depend on
the compiler start being enforced again.
