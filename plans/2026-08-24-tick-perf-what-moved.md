# Tick perf: what moved, what we cannot explain, what next

## What moved (PRs #428-#441)

| change | before -> after | why it matters |
|---|---|---|
| recount only when a source shrinks | 5,630 -> 3,268 calls per ghcache fold | recount is refcount GC; it ran on every additive tick for nothing |
| skip levels with an empty input frontier | 9.02 -> 6.02 stmts per rel (wide_64) | most rels do not move on a given tick |
| coalesce lowers to one LEFT JOIN | 256 arms -> 13 arms on page_response | 2^N delta arms was the single worst emitted rule |
| ghcache polls what the account offers | 1,422 404s/day -> 3 | hafley66 is a user, the org endpoint never existed |
| not_an_org is a fact from history | 2.8 h wait after restart -> instant | a fact fed only by a live edge is lost on restart |

## Where a tick's time goes now (PR #446, #451; quiet machine)

| block | ms | note |
|---|---:|---|
| DDL at process start | 98 | once per boot, 640 temp tables |
| 14 ticks, SQLite | 86 | level_insert 31, recount 18, publish 9, probe 9, stage 7 |
| 14 ticks, Rust | ~15 | 14-18% of tick wall |

## What we cannot explain yet

- Numbers move 6-25% with machine load; no run pins its load. Two runs of the same sha disagree.
- Tick SQLite grew 78 -> 86 ms since 89e3074ee. Guess: `probe` calls 14 -> 28 from the recount gate. Not proven.
- No tool diffs two shas per verb. Every comparison so far was by hand from PR bodies.

## Next

1. A fold benchmark that writes per-verb, per-tick rows plus a load stamp into the db, and `bench diff <sha> <sha>`. Everything else waits on this.
2. Run it on 89e3074ee vs main to explain the 8 ms.
3. DDL 98 ms at boot: check whether CREATE on an existing one-db is skippable.

## Parked, with a number

- decode-in-Rust: max win 6.7 ms per 14 ticks. Not worth a lane.
- one-db-retention: body cache 49 MB + `__str` 28 MB grow forever; needs a `<~` delete edge, your design call.

## Lab PR pile (nothing merged)

#443/#449 firehose, #444/#448 adapters-dir panic, #445/#450 branches cadence key: same fix twice each, both green. #446/#451: the measurements above.
