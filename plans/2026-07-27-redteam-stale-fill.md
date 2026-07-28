# RED TEAM: the stale-fill trichotomy

Target: `plans/2026-07-27-switch-flow.md` section 8 + ambiguity 4, whose position
is "admit the row, scope the view". Feeds `rulings.pl`'s PROVISIONAL
`stale_fill_policy` row.

Probes: `v6/prolog/labs/redteam_stale.pl`, **21 checks, all PASS**, swipl 10.0.2.
Run `swipl -q -l v6/prolog/labs/redteam_stale.pl -g go -g halt`; `-g report`
prints every scenario's store and world ledger under all 3 policies x 2 salt modes.

The probe model is a scale copy of the SHIPPING runtime, not of the Prolog forest:
`effect_cache` keyed (Target, Salt) with pending/done, fire-once by cache-row
membership (`v6/dl/src/1_hosts.ts:450-454`), boot replay = DELETE pending then
re-present every live demand row (`1_hosts.ts:414-428`), demand rows durable and
deltas not. It adds one component switch_flow never had: a **world ledger**, one
`call` per request that left the machine and was billed.

## VERDICT TABLE

| # | claim under attack | verdict |
|---|---|---|
| B1 | "abort and drop are observationally identical" | **BROKEN.** Identical in the store, 8-to-1 apart in peak concurrency on the same schedule. |
| B1b | "abort-on-teardown costs ZERO primitives" | **BROKEN.** It is the only policy whose close step reads in-flight state, and the shipping runtime has none of the three parts it needs. |
| B2 | orphan-as-a-row needs no retention primitive | **BROKEN.** Ruling q10 puts `keep` on Log rels only; the lab's own orphan destination is a Set rel, so nothing bounds it. |
| B3 | orphan-as-a-row is a well-defined reading | **DENTED, badly.** It is the only reading that is not crash-deterministic; making it durable collapses it into the longer-lived-demand reading. |
| B3b | goal-endurance grades the readings | **HOLDS trivially, and is therefore useless here.** All three readings pass phases 0-2 unchanged. |
| B4 | "drop is the only reading that ADDS a primitive" | **HOLDS on the letter, BROKEN on the framing.** Under content salts all three readings produce the SAME store; the trichotomy only exists once salts are per-instance. |
| B5 | orphan-as-a-row is not a dead-letter queue | **HOLDS on shape, and that is the problem.** One destination, so not AMQP; also no durability, so weaker than a DLQ on the axis that makes a DLQ worth having. |
| (a) | "orphan-as-a-row needs ONE FEWER primitive than drop" | **HOLDS as stated, and is the wrong metric.** Primitive count is 1 apart; world-call count is up to 12x apart in the opposite direction. |

## B1. The store is not the only observer

`b1_abort_and_drop_agree_in_the_store` reproduces the lab's claim honestly: on
`[open, close, arrive]` both readings end with an empty store, under both salt
modes. Then the second observer separates them immediately.

`b1_typeahead_peak_concurrency_is_eight_to_one`, eight keystrokes each replacing
the previous scope, one response:

| reading | store | world calls | peak in-flight |
|---|---|---|---|
| abort | `[row(query(8), hits(8))]` | 8 | **1** |
| drop | `[row(query(8), hits(8))]` | 8 | **8** |
| orphan | `[row(query(8), hits(8))]` | 8 | **8** |

`b1_the_store_is_blind_to_the_eight_to_one_gap` grades the row sets equal across
that gap. This is the v5 gh-cache shape exactly: `examples/gh-cache.dl:23-28`
builds its whole rate story on request ids that do not advance, and a 304 lands
zero rows, so an eight-fold call increase is invisible in the db.

**Abort is not free either, which the lab also missed.**
`b1_abort_thrashes_a_flapping_demand_that_drop_would_have_cached`: twelve reticks
of one endpoint, opened and closed each time. Cancelling on demand-support-zero
also deletes the in-flight lock (`rulings.pl:132-133`, "its pending cache row
deleted"), so a demand that flaps faster than the effect completes re-issues every
time. Content salts, identical schedule: drop spends **1** call, abort spends
**12** with 12 aborts, both stores empty. On `identical_reopen` under content
salts abort spends 2 calls where drop and orphan spend 1, and all three end with
the same row.

So the world-cost axis does not order the readings. It orders them by whether the
demand's departure is FINAL (abort wins, up to 8x concurrency) or a FLAP (drop
wins, up to 12x calls). That is a debounce question, and it does not belong to any
of the three readings.

**"abort costs ZERO primitives" is false against the shipping runtime.**
`b1_only_abort_reads_inflight_at_close_time`: after `[open, close]`, in-flight is
0 under abort and 1 under drop and orphan. Abort is the only policy whose close
step reads in-flight state at all. In `v6/dl/src/1_hosts.ts` that means three
additions, none of which exist:

1. `HostRunner.start` filters `event.inserts.length > 0` (line 387). Nothing in
   the runtime reacts to a demand-row DELETE.
2. There is no map from `full_digest` to a cancel handle. `runEffectOnce` holds
   the run in a local `for await` (line 469).
3. `HostDef.run(req): AsyncIterable<Resp>` (`0_types.ts:167`) takes no
   `AbortSignal`. `spawnCollect` already returns `() => child.kill()` (line 224),
   but `firstValueFrom` inside an `async *run` only unsubscribes on complete or
   error, so that teardown is unreachable from a demand deletion.

`dispose()`'s own comment states today's behavior in the affirmative: "any run
already past its `host.run()` drain settles harmlessly ... no cancellation
machinery this arc" (line 430-432). **The runtime ships orphan-as-a-row already,
by omission.**

## B2. Retention of orphans

`b2_orphan_rows_grow_one_per_dead_scope` + `b2_orphan_growth_is_linear_and_reader_independent`: a firehose of N distinct
short-lived scopes, every response landing orphaned, gives store sizes
`[1, 2, 8, 32]` for N in `[1, 2, 8, 32]` under orphan and 0 under abort and drop.
Nobody ever opened a scope that reads any of those rows. Growth is linear in scope
deaths and independent of readers.

`b2_q10_keep_cannot_reach_the_orphan_destination` reads the real
`v6/prolog/conformance/rulings.pl` and grades the implication:

- `ruling(q10_retention, keep_clause_required_on_log, user, _)` is USER-FINAL, and
  q10's text says `keep` "ranges over Log rels only".
- switch_flow's own orphan destination is `kind(cache_row/2, set)`
  (`content_feed_program`, recovered at `ac2aafdc`).
- A Set rel carries no `keep` clause, so under the ruling as written there is no
  retention expression that reaches an orphan row.

Bounding orphans therefore needs either `keep` on Set rels (a q10 change) or a Log
destination (a semantics change: stamps, multiset deltas, occurrence identity).
Both are primitives. The lab's own caveat ("bounded only by the rel's `keep`
clause") names a clause the ruling does not give it. Against the 39x db/corpus
ratio standing defect, an unbounded-by-default row source is a poor trade for one
saved column.

## B3. Exactly-once interaction

`b3_all_three_readings_pass_endurance_phase_one` and `..._phase_two`: on
`[open, crash, boot, arrive]` and a third boot, every reading lands one row with
two world calls and no re-fire. **goal-endurance.sh does not discriminate the
readings**, because its scope never dies. Extending it on paper means inserting a
scope death mid-nap, which is what `bare_orphan_crashed` is.

`b3_orphan_as_a_row_is_not_crash_deterministic`, content salts:

| schedule | orphan store |
|---|---|
| `[open, close, arrive]` | `[row(feed, body_one)]` |
| `[open, close, crash, boot, arrive]` | `[]` |

`b3_abort_and_drop_are_crash_deterministic` grades both readings empty in both
runs. The asymmetry has one cause: the durable trigger for a response is the
demand row, and orphan-as-a-row has just deleted it, so boot replay finds nothing
to replay. An orphan that lands before a crash is durable; an orphan in flight at
crash time is silently lost. Identical schedules, different stores, decided by
crash timing.

`b3_a_longer_lived_demand_restores_crash_determinism_for_every_policy` closes it.
Add a second, never-closed scope (`open(cache_rule, feed)`) and the crashed store
matches the quiet one under all three policies. So orphan-as-a-row is either
crash-lossy, or it is `rulings.pl:136-138`'s own reformulation: **demand from a
longer-lived scope**, which is an ordinary program rule plus abort, and not a fill
policy at all.

**Store-exactly-once is not world-exactly-once, independent of this ruling.**
`b3_store_exactly_once_is_not_world_exactly_once`: `runtime.commit`
(`1_hosts.ts:497`) and `UPDATE effect_cache SET state='done'` (`1_hosts.ts:501`)
are two separate awaits against two separate handles with no transaction spanning
them. A kill -9 inside that window leaves a `pending` row that boot replay
DELETES, so the demand re-fires against a cache miss. Store: 1 row (Set dedupe).
World: 2 calls. Every policy, same number. Flagging as a suspected defect read off
the source; it wants its own reproduction before it is called landed.

## B4. The detectability proof cuts the other way

`b4_under_content_salts_all_three_readings_agree`. On the identical reopen
(`[open s1, close s1, open s2, arrive(first_response)]`) with content-addressed
salts, all three readings end at `[row(feed, first_body)]`. Drop has nothing to
drop, because the arriving body is a valid answer to the live demand. Abort
cancelled and re-fired, and the re-fire's answer is byte-identical.

`b4_under_instance_salts_the_readings_split_two_ways`. The same schedule with
per-instance salts splits into `{abort, drop} = []` and `{orphan} = [row]`.

**So the trichotomy has no store-observable content until salts are per-instance.
It is a consequence of the salt ruling, and not an independent question.**

And the primitive that per-instance salts add is not a column. It is a world-call
multiplier, because per-instance identity means two subscribers to one target
never share:

| probe | content salts | instance salts |
|---|---|---|
| `b4_instance_salts_double_the_world_calls_on_a_shared_target` | 1 call, 0 aborts | 2 calls, 1 abort |
| `b4_content_addressing_is_the_gh_cache_rate_law` (12 reticks) | **1 call** | **12 calls** |

The second row is the 720-vs-12 defect restated one layer down.
`examples/gh-cache.dl:45-56` states the mechanism verbatim: "an already-fired id
is never re-fired ... between boundaries the SAME (ep, etag, bucket) hashes to the
same id, so nothing re-hits GitHub." Per-instance salts delete that property by
construction, and the store shows nothing.

Ruling `stale_fill_policy` before salt minting inverts the dependency.

## B5. Prior art, and the dead-letter question

| source | contract | which reading it backs |
|---|---|---|
| rxjs `switchMap` | "stops emitting items from the earlier-emitted inner Observable"; source comment "Cancel the previous inner subscription". Purely subscription-layer. | none by itself |
| rxjs `fromFetch` | explicitly builds an `AbortController` and calls `controller.abort()` in the teardown; an author-rolled `new Observable(sub => { fetch(...) })` does NOT abort. Its `abortable` flag disarms once the Response is emitted (no selector) so a late unsubscribe does not abort a delivered response. | abort, **opt-in per source** |
| Go `context` | signal only: `Done()` closes, `Err()` explains. Advisory. The stdlib's own `AfterFunc` example must call `conn.SetReadDeadline` by hand to interrupt a blocked read. | abort is cooperative, never automatic |
| nginx `proxy_ignore_client_abort` | **default `off`**: nginx closes the upstream connection when a client disconnects. Setting it `on` is the explicit opt-in for "keep filling the cache after the client left". | **abort is the industry default; orphan is the flag** |
| nginx `proxy_cache_lock` | many demanders, one origin fetch; `proxy_cache_lock_timeout` bounds the wait. Separately configurable from client-abort. | coalescing and cancellation are two knobs |
| `x/sync/singleflight` | no context parameter at all; the shared call runs to completion regardless of any caller. Forks (`janos`, `resenje.org`, Tailscale) all add **cancel only when the LAST waiter cancels**. | **refcounted abort**, converged on independently |
| Java `StructuredTaskScope` | shutdown "interrupts all unfinished threads"; a subtask finishing concurrently with shutdown has `handleComplete` **not invoked**. Result silently discarded. | drop, as a hard API contract |
| Trio | "Raising `Cancelled` means that the operation did not happen." | drop, as a semantic guarantee |
| Kotlin `NonCancellable` | exists so a block finishes despite parent cancellation; documented for **cleanup**, not for delivering a result to a dead requester. | orphan, but repurposed |
| RFC 5861 §3 | "caches MAY serve the response ... after it becomes stale"; "SHOULD attempt to revalidate it while still serving stale responses". **Silent on whether the triggering requester is still present.** | orphan, by construction |
| RFC 9111 | no clause keyed on a cancelled or aborted request; storage conditions are about the response received. | neither forbids nor blesses |
| RabbitMQ DLX | dead-lettering triggers: reject/nack with `requeue=false`, per-message TTL, length overflow, `delivery-limit`. None is "the consumer went away". A DLX is a routing indirection to a real second destination with its own consumers. | not this problem at all |

**Is orphan-as-a-row a dead-letter queue?** Three properties define a DLQ: a second
destination, its own consumer, and durability. `b5_orphan_lands_in_the_same_rel_as_an_answered_fill`
grades that an answered fill and an orphan produce the identical store
`[row(feed, body_one)]`. One destination, so the user's "we're not AMQP" instinct
is right about the shape. `b5_the_orphan_carries_no_delivery_guarantee` grades the
third property failing: the orphan is lost across kill -9 while the call was still
billed. Orphan-as-a-row is best-effort delivery to a destination nobody named,
weaker than a DLQ on the one axis that makes a DLQ worth having.

The residual dent the research surfaces: because the orphan lands in the primary
rel, **a reader cannot distinguish "demanded and served fresh" from "stale orphan
fill" unless the schema encodes it.** That is a freshness column, which is the
`written_at` column ruling `r_equal_row_write` already parks.

## THE ACTUAL DECISION SPACE

Three independent axes plus one that falls out:

```
  A. salt minting        content-addressed  |  per-instance
  B. cancellation        abort on support-zero  |  let it run
  C. completed-unclaimed land it  |  discard it       (exists only when A = per-instance)
  D. bound               what caps the rows C admits  (exists only when C = land)
```

Axis B is decidable on world cost alone and has nothing to do with axis C: the
switch_flow lab folded them together because it only measured the store.
Axis C is empty under A = content-addressed, so A strictly precedes it.
Axis D exists only if C admits rows, and q10 does not currently reach it.

## DECISION SHAPE: what to ask the user, in order

**Four questions, strictly ordered. Do not present them as one.**

**Q1 (blocks everything). Salt minting: content-addressed or per-instance?**
Evidence to hand him: content-addressed is what the TS runtime already ships
(`effect_cache.full_digest`), what `gh-cache.dl` builds its whole rate story on,
and what makes demand refcounting automatic. Per-instance costs 12x the calls on
the retick probe and 2x on a shared target, and it is the ONLY thing that makes
"stale" a definable predicate. If he picks content-addressed, Q3 disappears and
`stale_fill_policy` can be closed as "not applicable" rather than ruled.

**Q2 (independent of Q1, decidable on cost). Does demand-support-zero abort the
in-flight effect?** Frame it as a cost question; the store cannot see the
answer either way. Evidence: 8-to-1 peak concurrency on the typeahead probe
in favor of abort; 12-to-1 world calls on the flapping-demand probe AGAINST it;
nginx defaults to abort and makes non-abort the explicit flag; the singleflight
forks converged on abort-when-the-LAST-waiter-leaves, which is exactly
content-addressed support-zero. Expected shape of the answer: abort on
support-zero, with a grace window, which is a debounce and belongs on the
scheduler rather than in the language.

**Q3 (exists only if Q1 = per-instance). A response for a dead instance: land it
or discard it?** Bring three facts: it is crash-lossy either way, so "land it" is
a promise the runtime cannot keep; `StructuredTaskScope` and Trio both spell
discard as a hard contract; and the SWR case he actually wants is
`open(cache_rule, feed)`, a longer-lived demand, which
`b3_a_longer_lived_demand_restores_crash_determinism_for_every_policy` shows works
under every policy. His own reformulation in `rulings.pl:136-138` already said
this; the switch_flow lab did not test it, and it survives the crash probe.

**Q4 (exists only if Q3 = land). What bounds the landed rows?** Either `keep` gets
extended to Set rels (a q10 amendment) or the destination becomes a Log rel with
its own occurrence semantics. Do not let this one be a follow-up.

**What to tell him about the lab's position.** "Admit the row, scope the view" is
right about the SHAPE, and it is the behavior the TS runtime already has by
omission. It is wrong about the COST, because it counted primitives in a model
that could not see money, concurrency, or crash timing. On the three axes that can
see them, it loses.

## STANDING FACTS, whichever way the rulings go

- The switch_flow equivalence check runs the with-orphan and no-orphan schedules
  under the per-instance program only (`switch_flow.pl:1289`, `orphan_probe_run`
  calls `instance_feed_program`). It grades that drop drops, and it never compares
  abort against orphan. The claim it supports is narrower than the sentence it is
  cited for.
- `goal-endurance.sh` cannot grade this ruling as written: no scope dies in it.
  Adding a scope death mid-nap costs one `DELETE /edb/want` between `plant bravo`
  and `stop_server`, plus a "value must NOT arrive" assertion. That is the
  smallest change that makes the script discriminating.
- The `commit` / `effect_cache` ack window (`1_hosts.ts:497` vs `:501`) is a
  suspected world-exactly-once hole under kill -9. It is orthogonal to this
  ruling, it wants its own reproduction, and if it reproduces it is a
  failure-modes entry.
- `HostDef.run` has no `AbortSignal` parameter. Any abort ruling changes that
  interface, so it is a header change in `0_types.ts`, not a `1_hosts.ts` patch.
