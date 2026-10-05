# Phase atlas, 2026-10-05 (after pages 1-2, unit rewind, plain_inner)

sprefa main `0d8dfcf17`, sqlite_ivm main `1074daa`. Redraw of the 2026-10-04 atlas
(transcript, "TODAY" / "TARGET"). `// INSTALL` marks a new engine. `// FULL` marks work
proportional to all rows; `// DELTA` marks work proportional to the change.

## Spine today

```ts
compile$ = path$.pipe(
  // READ  (_8_driver/_0_read.rs; lib.rs:137)                       dd 12 ms   sqlite 12 ms
  switchMap(path => combineLatest([prelude$, readFile$(path), importChain$(path)])),
  map(units => units.flat()),

  // MACROTIME phase                                                dd 57 ms   sqlite 274 ms
  //   1. macro library: a full compile of macrotime/*.dl7, identical every process
  concatMap(units => compileUnits(macrotimeText).pipe(      // _8_driver/_2_macro.rs:18
    // INSTALL #1  ~1057 nodes                                      dd 6.3 ms  sqlite 52.6 ms
    map(slice_macro_program),
    //   2. one MacroSession for every unit (page 2)
    // INSTALL #2  ~1069 nodes, then mark                           dd 5.9 ms  sqlite 52.7 ms
    mergeMap(macroProgram => from(units).pipe(
      concatMap(unit => session.enter(unit).pipe(          // _4_expand.rs:220
        // sqlite: rewind to mark (savepoint); dd: retract last unit at -1 in this settle
        map(graph => syntaxSeeds(graph)),                   // DELTA: unit rows +1
        expand(wave => engine.settle(wave.changes).pipe(    // DELTA (page 1)
          map(applied => available.apply(applied)),         // DELTA
          map(() => macro_results(available)),              // claims, outputs
          takeWhile(r => r.claims.length > 0),
          map(r => rewrite_active_graph(graph, r)),         // Rust, recursive Pass::node   <- page 3
          map(rw => signed(rw.removed, -1).concat(signed(rw.added, +1))),
        )),
        last(), map(materialize))),
      toArray())))),

  // LOWER, CHECK: Rust passes                                      dd 5 ms    sqlite 5 ms
  map(lowerUnits), map(mergeBasements), map(check),

  // COMPTIME phase                                                 dd 78 ms   sqlite 473 ms
  map(checked => ({ rules: checked.rules, frozen: empty, session: new IrSession() })),
  expand(st => of(st).pipe(
    map(st => roundSeeds(base, st.frozen)),                 // FULL: base + edge_snapshot + intern_snapshot + answers
    map(seeds => ir_lower.lower(rules ++ st.generated, seeds)),   // FULL lowering every round
    // INSTALL #3 when fingerprint changes; ~2170 nodes           dd 8.6 ms  sqlite 80.2 ms
    //   reinstall between rounds: 3 of 122 programs (sqlite 107-123 ms, dd 9-10 ms each)
    concatMap(lowered => session.run(lowered)),             // diff vs last_seeds -> DELTA settle
    map(closure => snapshot(closure)),                      // FULL snapshot + decode
    mergeMap(closure => answerEffects$(closure)),           // Once executors -> frozen answers
    map(closure => ({
      edges: colon_rows(closure),                           // FULL scan
      requests: intern_rows(closure),                       // FULL scan
      generated: assemble_generated_program(closure),       // FULL scan; new rules -> new program <- page 4
    })),
    takeWhile(next => !stable(next, st)),                   // 84 programs: 1 round; 36: 2; 1: 3
  )),
  last(),
  map(finish),                                              // _5_finish: diagnostics, runtime program
  map(c => c.rows.filter(isFsFile)), mergeMap(writeFiles$), // dl8 build sink
)
compile$.subscribe()                                        // bin/dl8.rs
```

## Where the time goes (104 programs that compile, `dl8 compile`, sums)

Programs: every `.dl7` under `fixtures/`, `examples/`, `book/src/probes/`, `std/` (122; 18 exit
non-zero by design or need arguments, excluded). Debug profile with deps at opt-level 3.

| phase | dd s | sqlite s | gap s | sqlite installs s | sqlite non-install s |
|---|---|---|---|---|---|
| read | 1.27 | 1.27 | 0 | 0 | 1.27 |
| macrotime | 5.89 | 28.48 | 22.6 | 11.0 (#1 5.5 + #2 5.5) | 17.5 |
| lower + check | 0.59 | 0.60 | 0 | 0 | 0.60 |
| comptime | 8.15 | 49.16 | 41.0 | 9.5 (#3) | 39.6 |
| total | 15.9 | 79.5 | 63.6 | 21.0 | 58.5 |

Same settle counts on both engines (349 settles, 149 applies, 245 snapshots, 398 intern reads),
so sqlite pays more per settle.

Suite (`it` binary, `--test-threads=1`, `--report-time`): 164 tests, sqlite 246.4 s, dd 124.0 s.
The gap is spread: largest single test gap 14.8 s (`every_committed_case_matches_v7`); the top 25
tests hold 71 s of the 122 s gap. Most tests spawn `dl8` per fixture, so per-process fixed cost
(installs #1-#3, cold statement prepare) multiplies.

## Levers, ranked by measured sqlite seconds

| # | lever | targets | sqlite s at stake |
|---|---|---|---|
| 1 | comptime settle cost on sqlite (passes, statements, snapshot); start with the `plain_inner` receipt: run another closure pass only when a fresh term's functor has a reader | comptime non-install | up to 39.6 |
| 2 | macro library compile is identical every process: persist its compiled `MacroProgram` (content-addressed by macrotime source hash) | install #1 + its comptime settle | 5.5 + its settle |
| 3 | macrotime unit settles on sqlite (non-recursive `_i`/`_d` inserts, per-row source writes; retraction receipt) | macrotime non-install | up to 17.5 |
| 4 | install cost per CREATE on sqlite (#2, #3) | installs | 15.0 |
| 5 | comptime reinstall between rounds (page 4, P4a) | 3 programs | 0.34 |
| 6 | Rust FULL steps per round (lower, snapshot, colon_rows, assemble) | both engines | not split out |

Page 4 (engine extend) is a feature for import-graph HMR; measured speed gain on this corpus is
0.34 s sqlite.
