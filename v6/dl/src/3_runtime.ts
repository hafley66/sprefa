/**
 * 3_runtime.ts — DlRuntime: attach db, run DDL, lower the program, run the tick loop,
 * apply derived diffs, publish deltas$.
 *
 * Contract (plan M2, tasks.d.ts): `DlRuntime.boot({dbPath, bridge})`; `commit(batch)`
 * is THE single write site (one call = one tick); `rows(rel)`; `deltas$`. The loop is
 * one visible rx graph of named exported operators (the pipe IS the marble diagram):
 *   commits$ |> concatMap(applyEdbTxn) |> tap(injectSources)
 *            |> map(collectDerivedSets) |> map(diffAgainstTables)
 *            |> concatMap(applyDerivedTxn) |> tap(clearScratchRels)
 *            |> mergeMap(events) |> share()
 * concatMap IS the lock; share() = one graph, many readers. No hidden state outside
 * SQLite. Sources = BehaviorSubject per EDB rel seeded from current tables.
 *
 * Owned by package M2 (schema+runtime). Placeholder until that package lands.
 */
export {};
