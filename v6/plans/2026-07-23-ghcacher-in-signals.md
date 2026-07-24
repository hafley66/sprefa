# ghcacher in signals (2026-07-23)

Sources read: signals/src/3_Endpoint.ts, 4_Query.ts, 2_Signal.ts, 0_types.ts.
Translating: examples/gh-cache.dl.

## Correspondence

createQuery = the v5 effect cache.

| gh-cache.dl | 4_Query.ts |
|---|---|
| pending_effect digest (head,kind,args) | endpoint.key(input) -> queryCache Map (:150-153) |
| INSERT-OR-IGNORE | cache.get(key) hit |
| clock(300, b) salts args | bucket in query input; new bucket = new key |
| @async response lands later | materialize() -> reduceState |
| resp_latest max(b) | switchMap cancel (:179) |
| resp history table | old cache entries until cacheTime gc (bounded) |
| ? effect_log | QueryState + $.$ meta stream |
| (no error lane in dl) | QueryState.error / isError |

## Sketch

```ts
import {
  combineLatest, distinctUntilChanged, filter, interval, map, merge,
  scan, startWith, switchMap,
} from "rxjs"
import { Endpoint, Signal } from "@hafley-rxjs/signals"
import type { EndpointTransport, Query } from "@hafley-rxjs/signals"

// dl: sh fetch(...) = `gh api {ep} ...`. Shell text -> transport value.
export const ghTransport: EndpointTransport = async ({ url, method, headers }) => {
  const wire = await fetch(url, { method, headers })
  return {
    status: wire.status,
    headers: { etag: wire.headers.get("etag") ?? "" },
    body: wire.status === 200 ? await wire.json() : undefined,
  }
}

// dl: sh fetch(ep, prev) -> (status, tag, body)
export type PollInput = { ep: string; prev: string; bucket: number }
export type PollOutput = { status: number; tag: string; body?: unknown }

export const ghConditional = new Endpoint<PollInput, PollOutput>(
  {
    request: ({ ep, prev }) => ({
      url: `https://api.github.com/${ep}`,
      method: "GET",
      headers: { "If-None-Match": prev },
    }),
    decode: (response) => ({
      status: response.status,
      tag: String(response.headers?.etag ?? ""),
      body: response.status === 200 ? response.body : undefined,
    }),
    // pending_effect digest verbatim. Same triple = one wire call.
    key: ({ ep, prev, bucket }) => [ep, prev, bucket].join(" "),
  },
  ghTransport,
)

export const watch = Signal<string[]>(["repos/cli/cli"])   // rel watch(ep)
export const etags = Signal<Record<string, string>>({})    // rel etag(ep, tag)

// clock(300, b): time as value, not callback.
export const bucket = Signal(
  interval(300_000).pipe(map((i) => i + 1), startWith(0)),
  0,
)

// poll(ep, prev, b) <- watch(ep), etag(ep, prev), clock(300, b).
// One query per endpoint row. Map = the relation (friction 2).
const polls = new Map<string, Query<PollInput, PollOutput>>()
export const pollOf = (ep: string) => {
  if (!polls.has(ep)) {
    polls.set(
      ep,
      ghConditional.createQuery(
        combineLatest([etags.$, bucket.$]).pipe(
          map(([tags, b]) => ({ ep, prev: tags[ep] ?? "", bucket: b })),
        ),
        // bucket owns cadence; each key fires once ever.
        { staleTime: Infinity, cacheTime: 3_600_000 },
      ),
    )
  }
  return polls.get(ep)!
}

// etag(ep, tag) <- @next etag_next(ep, tag).
// THE one write. rx has no legal cycle; edge goes through Signal set.
// 304: tag == prev, distinctUntilChanged holds key stable until bucket moves.
export const wireEtag = (ep: string) =>
  pollOf(ep).$.pipe(
    map((state) => state.data),
    filter((data): data is PollOutput =>
      data !== undefined && data.status === 200 && data.tag !== ""),
    map((data) => data.tag),
    distinctUntilChanged(),
  ).subscribe((tag) => etags.$.setImmer((draft) => { draft[ep] = tag }))

// stars(ep, n) <- resp_current(ep, _, body), jsonp(body, "stargazers_count", n).
// jsonp = pure map at rule position (audit hole 7 lives here).
export const entity = (ep: string, kind: string, pick: (body: any) => unknown) =>
  pollOf(ep).$.pipe(
    map((state) => state.data),
    filter((data): data is PollOutput =>
      data !== undefined && data.status === 200 && data.body !== undefined),
    map((data) => ({ ep, kind, val: String(pick(data.body)) })),
    distinctUntilChanged((a, b) => a.val === b.val),
  )

export const stars    = (ep: string) => entity(ep, "stars", (b) => b.stargazers_count)
export const fullName = (ep: string) => entity(ep, "full_name", (b) => b.full_name)

// change_log: merge + scan + membership = datalog set semantics by hand.
// 304 emits nothing -> scan never runs -> appends nothing (parity-critical).
export type ChangeRow = { ep: string; kind: string; val: string }

export const changeLog = Signal(
  watch.$.pipe(
    switchMap((eps) => merge(...eps.flatMap((ep) => [stars(ep), fullName(ep)]))),
    scan((log, row: ChangeRow) => {
      const rowId = [row.ep, row.kind, row.val].join(" ")
      return log.has(rowId) ? log : new Map(log).set(rowId, row)
    }, new Map<string, ChangeRow>()),
  ),
  new Map<string, ChangeRow>(),
)

// ? change_log = changeLog.$() / .use() / .$.pipe(...) for SSE tail.
export const main = () => {
  const feedbackSubs = watch.$().map(wireEtag)   // imperative rim, explicit
  return { changeLog, dispose: () => feedbackSubs.forEach((s) => s.unsubscribe()) }
}
```

## Friction

1. **Cycle needs a blessed write.** poll reads etags; etags set from poll response.
   dl: `@next`, marked syntax, tick-delayed. Here: subscribe -> Signal set,
   invisible to types, delay unspecified. Language must make feedback edges
   first-class. Tick = cache-key advance; no global clock survived.
2. **Relation = Map beside the graph.** watch(ep) fanout needs polls Map.
   4_Query.ts does the same internally (queryCache, endpointCaches WeakMap).
   Store owns identity; rx is notation. Thesis re-derived from this package.
3. **Two freshness algebras.** staleTime = pull-time (defer check :166-171).
   clock bucket = push-time re-fire. No refetchInterval, so: staleTime Infinity
   + bucket in input. Cost: history = live cache entries, gc'd by cacheTime.
   dl keeps resp forever + max(b). Same word "cache", two retention policies.
4. **Set semantics manual.** change_log dedup = scan + Map.has. In dl it is the
   definition of a relation. Forget one distinctUntilChanged -> double feed.
5. **QueryState = the missing error lane** (both reviews flagged it).
   materialize() reifies next/error/complete; reduceState folds to COLUMNS
   (status, error, isStale, updatedAt). Port target: v6 host-rel response
   envelope gets QueryState-shaped columns. Failure = row, not teardown.
6. **latest-wins two ways.** switchMap cancels, no trace. dl accumulates + max(b),
   auditable, costs storage. Cancel has no dl word at all.
7. **decode vs derive.** decode = transport normalization only (status/tag/body).
   Entities = rule-position maps. Keeps lowering 1-rule-to-1-pipe.

## Better in translation

- 304-keeps-etag: free from distinctUntilChanged (dl needed a rule + comment).
- Shell body + $prev quoting hazard gone; transport typed, swappable.
- Error lane exists (dl has none).
- resp_latest/resp_current/max(b) -> switchMap.

## Worse

- Feedback edge unmarked (@next -> naked subscribe).
- Dedup hand-maintained.
- History gc-bounded, not durable.
- main() imperative rim; watch changes leak subs unless disposed.
