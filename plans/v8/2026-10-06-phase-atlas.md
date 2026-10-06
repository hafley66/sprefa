# Phase atlas, 2026-10-06 (main b16f9bb00, dd default, one process engine default)

Redraw of `2026-10-05-phase-atlas.md`. Counts: hello world on dd unless marked.
`// INSTALL` = new engine. `// DELTA` = work proportional to the change. `// FULL` = work
proportional to everything.

## Today

```ts
// once per process
engine = install(compose(                                   // INSTALL x1: 1,727 IR nodes, parts=3
  macroLibraryComptime,                                     //   p0: compiles macrotime/*.dl7
  macrotimeCone,                                            //   p2: the expander
  stdlibComptime,                                           //   p3: prelude rules
))

compile$ = path$.pipe(
  map(read),                                                // Rust parse, 11 ms
  concatMap(unit => engine.p2.settle(+syntaxRows(unit))),   // DELTA: 48 rows for hello
  expand(wave => wave.claims.length === 0 ? EMPTY
    : engine.p2.settle(rustRewrite(wave))),                 // DELTA settle; rewrite in Rust
                                                            //   (DL8_MACROTIME=engine: rewrite as rules, 7x work)
  last(),
  map(lowerUnits), map(check),                              // FULL: Rust batch passes
  concatMap(prog => engine.p3.settle(+programRows(prog))),  // DELTA on the stdlib part
  concatMap(round => round.programRules.length === 0
    ? of(round)                                             // hello: 0 program rules, no install
    : programEngine(round.programRules)                     // INSTALL: small compiled part per program
        .settle()),                                         //   (DL8_RULES=interpret: rows, 5x work)
  expand(r => r.stable ? EMPTY : nextRound(r)),             // closure diffs; lowering reused when inputs equal
  last(),
  map(finish),
  mergeMap(writeFiles$),
)
compile$.subscribe()                                        // bin/dl8.rs
```

## Target

```ts
engine = install(compose(macroLibrary, macrotime, lowerRules, checkRules, stdlib))   // INSTALL x1 per process

edits$ = merge(fsWatch$, lspDidChange$)                      // (path, content) changes
edits$.pipe(
  map(edit => [-rows(edit.old), +rows(edit.new)]),           // signed source rows
  concatMap(rows => engine.settle(rows)),                    // macrotime -> lower -> check -> comptime, all DELTA
  concatMap(delta => delta.programRules.changed
    ? engine.extend(delta.programRules) : of(delta)),        // page 4: extend/retire, no reinstall
  map(delta => delta.only('fs.file', 'diagnostic')),
).subscribe(publish)                                         // files, LSP diagnostics
```

## Gaps between them

| gap | today | needed |
|---|---|---|
| lower + check | FULL Rust passes | delta steps: rules, or Rust over deltas with kept state |
| program rules | 1 small install per program with rules | extend/retire on the process engine (page 4), or interpreter after its row work is cut |
| macrotime rewrite | Rust between waves | rules version exists (DL8_MACROTIME=engine) at 7x work; rerun-after-loop fix pending |
| input | one compile per process | file/LSP edits as signed rows into the live engine |
