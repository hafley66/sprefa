# ghcacher

[The contract](#the-contract) · [What dl8 has](#what-dl8-has) · [The sketch](#the-sketch) · [Gaps](#gaps) · [Receipts](#receipts)

Not built on dl8. The v6 golden is the contract; the sketch is a program that compiles and runs today, and the gap list is every place it falls short.

## The contract

`v6/tsv2/goldens/ghcacher_tick_golden`: one watched endpoint, a clock bucket, an etag carried from each answer into the next request. Program: `0_ghcacher_clock_golden.dl6:8-42`; schedule: `1_schedule.json`.

| tick | arrives | holds after the tick | construct it needs |
|---|---|---|---|
| 1 | `watch(repo)`, `etag_event(repo, "")`, `interval(300, 1)` | `current_etag(repo, "")`, `current_clock(300, 1)`, `poll(repo, "", 1)` | `key(1)` latch with `<+` (`.dl6:10-11`, `:24-25`) |
| 2 | fetch answer for `(repo, "", 1)`: 200, `tag-v1`, 17 | `resp`, `fresh_hit`, `cache_view(repo, tag-v1, 17)` | a host call keyed by `(ep, prev, bucket)` (`.dl6:15-17`) |
| 3 | `etag_event(repo, tag-v1)`, `interval(300, 2)` | `poll(repo, tag-v1, 2)` replaces bucket 1; `fresh_hit` retracts; `cache_view` holds | replacement and retraction |
| 4 | fetch answer for `(repo, tag-v1, 2)`: 200, `tag-v2`, 18 | `cache_view(repo, tag-v2, 18)` replaces `tag-v1` | keyed latch |
| 5 | a late answer for bucket 1 | no public change | a response for a demand no longer live is ignored |

Rows from `v6/tsv2/goldens/ghcacher_tick_golden/README.md:13-19`. The 304 leg: `ghcacher_304_golden/README.md:17-24`, `cache_view` keeps the last successful row through every not-modified answer. The clone leg: `ghcacher_checkout_golden/README.md`.

## What dl8 has

| piece | where |
|---|---|
| a clock | `timer`, Continuing, `src/_9_runtime/_3_executors/timer.rs` |
| one GET per url, status on error | `fetch_json`, Once, `fetch_json.rs:42-59` |
| rows that survive a restart | `--db`, [The store](../12_store.md) |
| a Rust client with etags, outside dl8 | `hafley-rs/crates/ghcache/src/gh.rs:65-66` `with_etag`, `:90` `is_not_modified`, `:92-93` `etag`; it runs the `gh` binary, `:175` |

## The sketch

```dl7
{{#include ../probes/15_ghcacher.dl7}}
```

```ts
import { combineLatest, interval, of, type Observable } from "rxjs";
import { ajax } from "rxjs/ajax";
import { catchError, filter, map, mergeMap, shareReplay, switchMap } from "rxjs/operators";

type Answer = { url: string; ok: true; body: string } | { url: string; ok: false; status: number };

// (timer 100 ?Tick): ticks from 1.
const timer = (periodMs: number) => interval(periodMs).pipe(map((index) => index + 1));

// fetch_json, Once: one request per url, replayed to every later reader.
const fetchJson = (url: string): Observable<Answer> =>
  ajax({ url, responseType: "text" }).pipe(
    map((response): Answer => ({ url, ok: true, body: response.response as string })),
    catchError((error) => of<Answer>({ url, ok: false, status: error.status ?? 0 })),
  );

export const ghcacher = (watch: Observable<string>) => {
  const poll = watch.pipe(switchMap((url) => timer(100).pipe(map((tick) => ({ url, tick })))));
  const answer = watch.pipe(mergeMap(fetchJson), shareReplay(1));
  const joined = combineLatest([poll, answer]).pipe(filter(([p, a]) => p.url === a.url));
  const response = joined.pipe(
    filter(([, a]) => a.ok),
    map(([p, a]) => ({ url: p.url, tick: p.tick, body: (a as { body: string }).body })),
  );
  const failed = joined.pipe(
    filter(([, a]) => !a.ok),
    map(([p, a]) => ({ url: p.url, tick: p.tick, status: (a as { status: number }).status })),
  );
  return { poll, response, failed };
};
```

```console
$ bash book/show.sh run book/src/probes/15_ghcacher.dl7 --serve timer,fetch_json --max-ticks 3 | grep '^(effect \|^(Failed \|^ticks'
(effect timer ref(application(timer, [100 none])))
(effect fetch_json ref(application(fetch_json, ["http://127.0.0.1:9/repos/cli/cli" none])))
(Failed "http://127.0.0.1:9/repos/cli/cli" 1 0)
(Failed "http://127.0.0.1:9/repos/cli/cli" 2 0)
ticks 3
```

A single `effect fetch_json` row serves every poll: the fetch ran once, the shape `shareReplay` gives `answer`.

## Gaps

| gap | the v6 contract needs | dl8 today | line |
|---|---|---|---|
| request per bucket | a new request when `(ep, prev, bucket)` changes | the application holds only the bound columns of `fetch_json`; the executor reads position 0 and writes a two-column row | `fetch_json.rs:74-78`, `:88-92`; `src/_9_runtime/_2_reconcile.rs:140-143` |
| request headers | `If-None-Match: <prev>` | `agent.get(url).call()` sends none | `fetch_json.rs:43` |
| response headers | the `ETag` of the answer | not read; no column carries it | `fetch_json.rs:42-59` |
| 304 | a 304 leaves `cache_view` alone | a 304 is an error row with status 304 and message `http status 304`, reachable only with request headers | `fetch_json.rs:52-54` |
| keyed latch, `key(1)` with `<+` | `current_etag`, `current_clock`, `cache_view` replace per key | `<+` rewrites to `<-` | `macrotime/0_standard.dl7:100-102`; [Not built yet](../16_not_built.md) |
| retraction | `poll` and `fresh_hit` rows leave when the bucket moves | tables are append-only | `src/_6_eval/_3_table.rs:1-3` |
| `pre/1`, latest | the previous tick's etag feeds the next request | not built | `plans/v8/2026-09-14-v8-design-review.fable.md:127` |
| schedule-fed replay | the golden replays `1_schedule.json` hermetically | not built in dl8 | [Not built yet](../16_not_built.md) |

## Receipts

| claim | path | command |
|---|---|---|
| the v6 contract holds on v6 | `v6/tsv2/goldens/ghcacher_tick_golden/6_gate.sh` | `bash v6/tsv2/goldens/ghcacher_tick_golden/6_gate.sh` |
| the port target named in the reconciler brief | `plans/v8/2026-09-14-v8-reconciler.brief.md:37` | `sed -n 37p ../plans/v8/2026-09-14-v8-reconciler.brief.md` |
| the sketch compiles | `book/src/probes/15_ghcacher.dl7` | `cargo test --test _22_book probes_compile_as_their_page_says` |
