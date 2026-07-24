# v6 in literal rxjs — the whole deal, rx words only (F10 working spec, 2026-07-23)

> Vocabulary law (owner): rx words ARE the vocab. No aliases, no sigils, no invented
> nouns. Banned: the v5 rel-boundary noun ("host rel" is the phrase), @-anything.
> Everything below is runnable-shaped TypeScript over rxjs 7.

## The axis law

There is exactly ONE stream-time axis: **tick**, the engine's monotone batch counter.
Git time (`repo_revs`) is DATA — columns you join through, never a clock the engine
bumps. "As of sha X" is a query over rows; "previous value" is `pairwise()` over tick.

```ts
type Tick = number;              // engine time. The only stream axis.
type Row  = readonly unknown[];  // world time (git sha, rev_id) lives INSIDE rows.
```

## The one `.next()` site

Correct FRP calls `.next()` only where the non-reactive world pushes in. For v6 that
is the ingest boundary (fs watcher, extractor jsonl, http request arrival) and
nowhere else:

```ts
const changes$ = new Subject<ChangedCell[]>();       // THE .next() site. The only one.

const tick$: Observable<Tick> = changes$.pipe(
  buffer(interval(POLL_MS)),                          // coalesce a batch
  filter((batch) => batch.length > 0),
  concatMap(async (batch, i) => {                     // concatMap: ticks are ordered, never overlap
    await markChanged(batch.flat(), i);               // SQLite: bump changed_at
    return i;
  }),
  share(),                                            // one tick train, many riders
);
```

## A rel, by kind, in rx words

```ts
// EDB (facts): re-read from SQLite when the tick says so. Rows live on disk;
// the stream carries the signal + the bounded read.
const edge$: Observable<Row[]> = tick$.pipe(
  switchMap(() => from(sql`SELECT * FROM rel_edge`)),
  shareReplay({ bufferSize: 1, refCount: true }),     // materialized = shared replay;
);                                                    // refCount = the subscription law

// derived (a rule): combineLatest is the join, map carries equi-join/select/project.
const watchLabel$: Observable<Row[]> = combineLatest([watch$, label$]).pipe(
  map(([watchRows, labelRows]) => equiJoin(watchRows, labelRows)),
);

// union (two rules, one head): combineLatest + dedup (set semantics).
const reachable$ = combineLatest([ruleA$, ruleB$]).pipe(map((sets) => dedupSorted(sets.flat())));

// negation: an anti-join INSIDE map. Element-level, so no operator exists or is needed.
const missing$ = combineLatest([all$, seen$]).pipe(
  map(([allRows, seenRows]) => antiJoin(allRows, seenRows)),
);

// lazy: exactly the same pipe, minus shareReplay. Cold; re-subscribe re-runs.

// host rel (the effect seam; asynchrony is the host's implementation detail):
type HostRel = (trigger$: Observable<Row[]>) => Observable<Row[]>;
const httpGet: HostRel = (trigger$) =>
  trigger$.pipe(switchMap((rows) => from(fetchAll(rows))));   // or concatMap: ordered
// switchMap = cancel-stale. concatMap = queued. A property OF THE HOST REL, never per-rule.

// clock: a host rel with no inputs.
const poll$: Observable<Tick> = interval(POLL_SECS * 1000).pipe(share());
```

## The carry (what @next was), in rx words

```ts
// "etag at t vs etag at t+1" is a 2-tuple. rx word: pairwise. Storage word: replay: 2.
const etagPair$: Observable<[Row[], Row[]]> = etag$.pipe(
  startWith([] as Row[]),                             // the seed
  pairwise(),                                         // [previous, current] per emission
);
const changed$ = etagPair$.pipe(
  map(([previousRows, currentRows]) => diffKeys(previousRows, currentRows)),
  filter((delta) => delta.length > 0),
);
```

Datalog-native spelling of the same tuple: tick as a column — `etag(ep, tag, tick)` —
and "previous" is a self-join on `tick - 1` (v5's argmax-by-gen, renamed). The
bitemporal store (`fact(key, tt_from, tt_to)`) already keeps these intervals, so
`pairwise` is the special case `tick IN (now, now - 1)` and history depth is a
retention rule, not a new mechanism.

## Early cutoff (what verify was), in rx words

```ts
// salsa's early cutoff IS distinctUntilChanged on a digest.
const cutoff = <T extends Row[]>() =>
  distinctUntilChanged<T>((previousRows, currentRows) => digestOf(previousRows) === digestOf(currentRows));

const stars$ = starsRaw$.pipe(cutoff());              // a 304 upstream = silence downstream, free
```

## Feedback (a rule reading its own past), in rx words

The rx graph is ACYCLIC within a tick (stratification guarantees it). Every cycle is
one of exactly two things:

1. **within-stratum recursion** — the fixpoint: a plain `while` inside `map` for
   bounded sets (`expand` only ever earns its keep across an async hop), or the SQL
   delta loop for heavy sets. Never a subject knot.
2. **cross-tick feedback** — goes THROUGH THE STORE. The table is the knot: tick t
   writes, tick t+1 reads (`pairwise` over the tick axis). No BehaviorSubject in the
   language; if an implementation needs a knot object internally, that is plumbing,
   not semantics.

```ts
// gh-cache request loop, acyclic per tick, knotted through the store:
const request$ = poll$.pipe(withLatestFrom(etagFromStore$));  // SAMPLE, don't join —
// withLatestFrom reads the latest without reacting to it (v5 never had this word).
const response$ = httpGet(request$.pipe(map(buildRequestRows)));
// response$ -> ingest -> store -> etagFromStore$ next tick. The cycle crosses ticks on disk.
```

## The query surface, in rx words

```ts
const answer = await firstValueFrom(watchLabel$);     // ? query = take(1) demand
const live = watchLabel$.subscribe(render);           // a standing subscription
// process lifetime = sum of held subscriptions. refCount everywhere makes it literal.
```

## The vocabulary table (dl concept -> rx word, complete)

| dl concept | rx word | notes |
|---|---|---|
| fact arrival | `subject.next(rows)` | the ONE site (ingest boundary) |
| derived rule | `combineLatest` + `map` | join/select/project inside map |
| multi-rule union | `combineLatest` + `map(dedup)` | set semantics |
| negation | anti-join inside `map` | element-level; no operator |
| bounded recursion | `while` inside `map` | expand only over async hops |
| heavy recursion | SQL delta loop | rows never in JS heap |
| carry / previous value | `startWith` + `pairwise` | = `replay: 2`; = tick-column self-join |
| early cutoff | `distinctUntilChanged(digestEq)` | salsa verify, literally |
| materialized | `shareReplay({bufferSize: 1, refCount: true})` | table = the replay buffer |
| lazy | cold `Observable` | re-subscribe re-runs |
| demand / cold-by-default | `refCount` | the subscription law |
| effect (host rel) | `switchMap` / `concatMap` into `from()` | property of the host rel |
| clock | `interval` + `share` | a host rel with no inputs |
| sample-without-reacting | `withLatestFrom` | new expressive power v5 lacked |
| one-shot query | `firstValueFrom` / `take(1)` | subscribe, answer, unsubscribe |
| ordered ticks | `concatMap` on the tick train | ticks never overlap |

## The two honest gaps (where rx has no word)

1. **Glitch-free consistent propagation.** `combineLatest` double-fires diamonds with
   transiently inconsistent views. rx has no transactional update. That gap IS the
   reconcile plane: `buffer(tick$)` + the ascending-id topo sweep (`propagate`) is
   the missing "atomic batch" operator, implemented over SQLite. Effects therefore
   subscribe to the tick train, never to raw interior pipes.
2. **Durable state.** rx replay buffers die with the process. The store is the replay
   buffer that survives: `shareReplay` whose buffer is a SQLite table. This is the
   whole project thesis in one sentence.

Everything else in the language is a plain rx word. If a proposed feature cannot be
written in this file's vocabulary plus those two named gaps, it does not go in the
language.

---

## ADVERSARIAL REVIEW VERDICT (2026-07-23, two independent Fable reviewers, probe receipts)

Both reviewers, independently, with runnable probes against rxjs 7.8.2: **the closing
law leaks as written.** The dataflow rows all survived (join/union/negation/fixpoint/
heavy-recursion-in-SQL/cancel-stale, verified against lower.ts and the labs). Every
failure clusters in one zone: **operator state is subscription-local and process-local**
(`pairwise`, `distinctUntilChanged`, `shareReplay`'s buffer, `concatMap`'s index,
`interval`'s counter, `withLatestFrom`'s latch), so under the refCount lifetime law that
state evaporates at exactly the demand boundaries the language serves. Convergent
findings: non-monotone tick ids after refcount-zero reset (corrupts `changed_at`
ordering durably); fact loss at the `.next()` site during idle windows; one effect
rejection kills the shared train (error/retry is an unnamed gap); unbounded `concatMap`
queue under sustained pressure (admission is an unnamed gap); quiescent queries hang
(demand-pull of current state is an unnamed gap); post-churn `pairwise` reseeds and
reports the whole relation as new (the etag re-fetch storm); torn diamond reads through
`firstValueFrom` on interior pipes; `withLatestFrom` cold-start drops deadlock the
gh-cache loop; completion/fairness unnamed; the store schema is currently monotemporal
(the git-time axis has no landing column); `propagate`'s frontier-skip has no rx word
(distinctUntilChanged is only the cutoff half). One shipped-code bug found and FIXED
same day: `TemporalStore.attach` was missing `FROM fact` (TS port drop; Rust
engine.rs:1382 correct).

**Resolution (the corrected reading of this file):** the store is the engine; rx words
are the READ-SIDE NOTATION. Concretely: the tick counter is store-owned (a monotone
SQLite counter bumped inside the ingest commit), never an operator index; the ingest
write path is imperative (watcher/extractor/http -> store write -> counter bump), and
the rx graph begins DOWNSTREAM of the commit, so tick-reset/ingest-loss/mid-tick-
teardown vanish as a class; carry and cutoff are store spellings (tick-column self-join;
rx_memo verify) with `pairwise`/`distinctUntilChanged` as their per-subscription
notations only; queries and demand read the store, never interior pipes; effect
responses re-enter through the idempotent ingest (identical rows = zero changed cells =
no tick — the E2 property), which is what terminates effect feedback loops. The gap
list is therefore: (1) transactional propagation, (2) durable state, (3) error/retry
boundaries, (4) admission/backpressure, (5) demand-pull + completion semantics — all
owned by the engine plane, none by rx vocabulary.
