# REPORT — deaf file watcher (lane/t3-watcher-flash)

## Files touched

- `v6/tsv2/serve/2_binds.ts` — the fix (imports `repeat`/`retry`; both applied to
  `WatchBindRunner`'s `liveWindows`).
- `v6/tsv2/tests/watchSourceLiveness.test.ts` — NEW. Real-fs regression guard.
- `v6/tsv2/tests/watchSourceCompletion.test.ts` — NEW. Deterministic red→green
  receipt for the completing-source failure class.
- `REPORT.md` — this file.

All changes left uncommitted.

## Root cause

The production `IWatchSource` is `NodeWatchSource` (`serve/2_binds.ts:209`,
wired as the default `config.watchSource ?? new NodeWatchSource()` at
`serve/4_http.ts:175`). It turns OS events into the observable via
`from(fs.promises.watch(root, {recursive, signal}))` inside a `defer`.

The digest-diff bookkeeping in `GlobWatch.batchFor` (`2_binds.ts:351`) is
correct. The defect is that the watch stream is FINITE, not that the digest
compare is wrong. In `WatchBindRunner`'s constructor the live pipeline is

```ts
const liveWindows = options.source.watch(watchRootOf(options.root, glob)).pipe(
  bufferTime(coalesceMs, scheduler),
  filter((paths) => paths.length > 0),
  map((paths) => ({ kind: "paths", paths })),
);
return merge(liveWindows, boot).pipe(scan(...), concatMap(...), ...);
```

`boot` completes after its one emission. If `liveWindows` ever COMPLETES —
which is exactly what node's macOS `fsPromises.watch` recursive async iterator
did in the field, emitting its first delivery and then terminating — then
`merge(liveWindows, boot)` completes once both sources are done, the entire
per-glob observable completes, and `firings$` completes. There is no recovery
path until a program swap. Result: the FIRST digest-changing edit is delivered,
then the bind is permanently deaf to that file and to everything, for the rest
of the run. This is the "subscription completing" failure class, not a watcher
teardown or an AbortSignal fault (the AbortController only ever aborts on our
own `finalize`/unsubscribe).

### Why I could not catch this with the real OS watch

I reproduced every faithful scenario of the served path in THIS worktree (node
v24.15.0, macOS) and the OS `fsPromises.watch` iterator does not terminate
early here:

- raw `fs.promises.watch` recursion: continuous delivery across a burst + quiet
  edit (7 events) and 8 spaced edits;
- in-process served engine, same-process edits: N edits → N batches;
- atomic-save (write temp + rename): 8/8;
- external-process edits: 8/8;
- git-repo root with 6 edits: 6/6;
- 3001-file tracked repo: 4/4;
- real backgrounded `serve/main.ts` subprocess driven over HTTP: boot + N/N;
- file+subdir created after load, then edits: 4/4;
- 300-write burst then a quiet edit: burst coalesces to 1 batch, quiet edit still
  lands (2/2);
- the canonical `scripts/extraction-live.sh` (all 9 phases) HOLDS.

So the live source is well-behaved in this environment. The terminating-iterator
failure is environmental (macOS FSEvents / a given machine's system state), but
the CODE was structurally wrong: it depended on the OS iterator never ending,
and it had no recovery when it did. The contract in the `2_binds.ts` header —
"the watch stream is INFINITE until program swap or server shutdown" — was
unstated in the code and unenforced.

## Fix (one file, `2_binds.ts`)

Append `repeat()` and `retry()` to `liveWindows`, after the `map`:

```ts
const liveWindows = options.source.watch(watchRootOf(options.root, glob)).pipe(
  bufferTime(options.coalesceMs, options.scheduler),
  filter((paths) => paths.length > 0),
  map((paths) => ({ kind: "paths" as const, paths })),
  repeat(),   // live source COMPLETED (OS iterator ended early): re-establish
  retry(),    // live source ERRORED (non-abort): re-establish
);
```

`NodeWatchSource.watch` returns `defer(...)`, so each re-subscription runs a
fresh `defer`, creating a new AbortController + new `fs.promises.watch`; the
prior subscription's `finalize(() => controller.abort())` closes the old OS
watch, so re-establishing does not leak an FD. The real teardown is still the
program swap's `switchMap` unsubscription, which propagates through the whole
pipe and aborts the controller.

`repeat`/`retry` are inert on a live source that never completes or errors
(it simply re-subscribes on termination), so all existing behavior is
unchanged; they only restore the infinite contract when a source terminates
prematurely.

## Fail-first test (red, then green)

`tests/watchSourceCompletion.test.ts` emulates the terminating-iterator failure
with a COLD injected source (`defer`) that emits one path then completes. It
asserts a SECOND novel edit still produces an arrival batch. The source is cold
because rxjs `repeat` re-subscribes to the same source instance, and a hot
completed `Subject` stays completed (infinite loop); `NodeWatchSource` is cold,
so the test mirrors production.

RED (before the fix, `2_binds.ts` stock):

```
✖ watch bind: the live stream survives a completing source; a later edit still lands
  AssertionError [ERR_ASSERTION]: second edit after completion must still land, got 1
  1 !== 2
```

GREEN (after the fix):

```
✔ watch bind: the live stream survives a completing source; a later edit still lands
tests 1  pass 1  fail 0
```

`tests/watchSourceLiveness.test.ts` is the real-fs guard the task asked to write
first: `N successive distinct edits to one file produce N delta batches`, through
the full served engine with the PRODUCTION `NodeWatchSource` against a temp dir.
It passes in this environment (see above). It is the regression net if the live
stream ever regresses to finite.

## Validation (all run, tails pasted below)

`tsv2-test` recipe (`cd v6/tsv2 && npm test`, node
`--test --experimental-transform-types --test-concurrency=6`):

```
ℹ tests 130   ℹ pass 129   ℹ fail 0   ℹ skipped 1
```

(129 pass = prior 127 + the two new tests; 0 failures, 1 pre-existing skip.)

New test files, individually (red→green for the completion test shown above;
liveness green):

```
✔ watch bind: the live stream survives a completing source; a later edit still lands
✔ real fs watch source: N successive distinct edits to one file produce N arrival batches
```

`serve-leak-soak` (`bash v6/tsv2/scripts/leak-soak.sh`) — handles flat across
20 program swaps; the `repeat`/`retry` re-establishment leaks nothing:

```
✔ receipt (c): 20 program-swap cycles leave no handle, timer, or subscription behind
```

Also re-ran the canonical harness `scripts/extraction-live.sh` after the fix:
`EXTRACTION LIVE HOLDS` (all 9 phases PASS). And `npx tsgo --noEmit -p
tsconfig.json`: exit 0.

## Commands that ran >10s (per dispatch rule)

- `cargo build --release --features cli --bin extract` (the first `extraction-live`
  leg): minutes; needed to run the real harness.
- `bash v6/tsv2/scripts/extraction-live.sh`: ~48s each run.
- `npm test` (full tsv2 suite): ~7s wall.
- `bash v6/tsv2/scripts/leak-soak.sh`: ~5s wall.

## Environment

- Assigned worktree only; no commits; changes uncommitted.
- node v24.15.0 (only version present), macOS, darwin.
- `v6/tsv2/node_modules` and `v6/sprefa-store/js/node_modules` were missing and
  were `pnpm install`ed first (per the dispatch rule).
