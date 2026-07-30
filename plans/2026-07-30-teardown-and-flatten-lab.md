# Teardown and flatten: the effect plane is deaf to retraction

Lab branch `lane/teardown-flatten`, base `a4629623`. Zero production edits.
Runnable evidence: `labs/teardown-flatten/receipts.sh` (`TEARDOWN LAB HOLDS`,
exit 0), which drives `labs/teardown-flatten/flatten-probe.ts`. The prior lane's
`v6/tsv2/rxoracle` harness was re-run first and holds 8/8 as declared.

## The one-paragraph answer

Door 1 works, and it is narrower than it looks, because **door 1 is already the
ruled position and the ruling names the unbuilt site itself**. `effect_abort`
(`v6/prolog/conformance/rulings.pl:158-168`) says demand-row deletion IS the
abort signal and closes with "Lowering owed: AbortSignal through HostDef.run +
cancel-handle map + pending cache-row delete on abort (none exist today,
1_hosts.ts:387 filters inserts only)". `subscription_kernel` (same file,
:170-186) says "switchMap = keyed replace on an ordinary program rel;
flattening policy = the scope row primary key shape; teardown = ordinary IVM
retraction". Both were ruled 2026-07-27. Neither was built. This lab's
contribution is not the design, it is the measurement that the design is
already correct and already unimplemented, plus one result the rulings do not
contain: **three of the four flatteners need no new syntax and no ordinal, and
the fourth needs a relation made visible rather than a construct added.**

## What was measured

`R1`. A superseded demand row produces a `del` on `__host_demand_fetch_body`,
on the tick of the supersession, carrying the retracted route's own witness
digest. Verbatim from the served tick stream, tick 3:

    "__host_demand_fetch_body":{
      "add":[["identity|fetch_body|route_id:text=r2","witness|fetch_body|route_id:text=r2","r2"]],
      "del":[["identity|fetch_body|route_id:text=r1","witness|fetch_body|route_id:text=r1","r1"]]}

`R2`. Nothing reads it. The superseded `r1` ran to completion (`start r1 / done
r1 / start r2 / done r2`), its answer landed on tick 5, two ticks after its
demand retracted, and it is stored durably (`__host_response_fetch_body` holds
2 rows). The program's own relation is nevertheless **correct**: `body` holds
exactly one row, `r2`. Identity is not the problem and never was.

`R5`. The `concatMap` serialization is a separate cost. Two jobs with no shared
key, no retraction anywhere, no supersession possible: still serialized (`start
j1 / done j1 / start j2 / done j2`), wall time the sum of the two rather than
the max.

## The four flatteners

Measured by `labs/teardown-flatten/flatten-probe.ts`, which reads the same
`GET /ticks` stream, spawns the same shape of child, and changes one rx
operator. It imports `rxjs` and node builtins and nothing from this repository,
the leg-A discipline rxoracle already holds. `concat` reproducing the shipped
runner's ledger exactly is what makes the other three credible.

| flattener | measured ledger | expressible as rules today | what is actually missing |
|---|---|---|---|
| `concat` | `start r1 / done r1 / start r2 / done r2` | **yes**, and it is what ships | nothing at the program level |
| `merge` | `start r1 / start r2 / done r1 / done r2` | **program-identical to concat** | a concurrency number, which is not a program property |
| `switch` | `start r1 / start r2 / torn down r1 / done r2` | **yes** (`key(1)` head, the flagship program, byte-unmodified) | the runtime must read `del` |
| `exhaust` | `start s1-r1 / done s1-r1` | **no** | an in-flight relation; the rows exist and are invisible |

Three results in that table are worth stating on their own.

**`merge` and `concat` are the same program.** Nothing in the source text
distinguishes them. The difference is entirely how many inners the runner
allows at once, and a program that could specify that would be specifying a
property of the machine, not of the computation. This splits the flattening
axis in two: *which inners get torn down* is the program's business and is
expressed by retraction; *how many run at once* is the runner's and probably
should never be a program property at all. The user's framing put flattening on
one axis; the measurement says it is two, and only one of them belongs in the
language.

**`switch` needs no key at the effect plane.** The probe's switch does not
group by any slot. It groups by the *witness*, which is the demand row's own
identity, and flattens each group with `switchMap` over its own sign: an `add`
starts the inner, a `del` for that same witness switches to `EMPTY`, and
switching away is what unsubscribes and kills the child. The program never says
"these two demands compete". The rule that retracted the row already applied
whatever discipline the program declared, so `del` arrives carrying that
decision per row. **Teardown-on-`del` is strictly more general than
`switchMap`**: it expresses switch (a `key(1)` head), a scope closing, and a
`not(...)` guard going true, all as one lowering, with no slot column and no
new construct.

**`exhaust` is the only genuine gap, and it is a visibility gap.** Exhaust
means "while a slot is busy, drop new work for that slot", which requires
knowing a slot is busy. Those rows exist: `__host_witness` carries
`state = 'pending'` for exactly the in-flight witnesses
(`v6/tsv2/serve/1_hosts.ts:76-129`). They are runtime-private and no program can
join them. The probe derived a slot from the first input column to grade the
operator at all, and that derivation is the probe's convention, not the
language's: `identity_digest` and `witness_digest` are both content-addressed
over the full input tuple, so two demands competing for one slot get two
unrelated digests and no column says they compete.

## R6, which was a wrong prediction before it was a receipt

The expectation written first was `start r1 / torn down r1 / start r2 / done
r2`, teardown before the winner starts. That is not what happens. The winner's
`add` and the loser's `del` are on **one tick** (R1 measured both on tick 3),
and a tick's delta halves are a set, not a sequence: `runtime/ticklog.ts:52-54`
sorts each half lexicographically and emits `add` before `del`, and rxoracle's
own N2 normalization already records that a tick log has no intra-tick order to
compare against. Nothing in the data says whether the teardown or the new start
happens first. The probe took `add` first. Taking `del` first is an equally
faithful reading of the same tick stream and yields a different ledger:

    switch            start r1 / start r2 / torn down r1 / done r2
    switch-del-first  start r1 / torn down r1 / start r2 / done r2

Both are in the receipts. This is invisible while concurrency is unbounded and
becomes a real decision the moment it is bounded: `del` first frees the slot
before the winner claims it, `add` first can momentarily exceed the bound. It
is card 4.

Two measurement bugs were found and fixed getting here, both recorded in the
probe's own comments because both would have produced a confident wrong answer.
The teardown was first logged from the child's `close` event, which reported
every teardown after the winner's start even when it had been issued first (the
ledger was ordering itself by process reaping, not by the demand stream). Then,
logged from the unsubscribe function without a guard, it marked every
*completed* run as torn down, because rxjs unsubscribes an inner on completion
too.

## Hard question 1: is teardown one site or many?

**One file, four sites, and the one that actually kills the process is already
written.** All of it is `v6/tsv2/serve/1_hosts.ts`.

| # | site | lines | what changes |
|---|---|---|---|
| 1 | `liveDemand$` | :449-460 | `delta.add.map(...)` becomes both signs. This is the site the `effect_abort` ruling already names. |
| 2 | the flattening pipeline | :410-418 | `concatMap` over batches cannot selectively unsubscribe one inner. Needs `groupBy(witness)` plus per-group `switchMap`, or `concatMap` with a per-inner `takeUntil`. |
| 3 | `claimed` / `claimOnce` | :389, :473-478 | the in-process dedupe is monotone. A torn-down witness must be released or a re-assertion silently never refires. |
| 4 | `WitnessCache` | :76-129 | needs a release that deletes the `pending` row. The ruling names this too ("pending cache-row delete on abort"). |
| — | `runShellLine` | **:175** | **nothing.** `return () => child.kill();` already exists. It has never once been called. |

Site 5 is the finding worth keeping: the teardown is not missing, it is
unreachable. The runtime already tears down effect trees in two other places
for two other keys, and both are load-tested: `4_http.ts:412` uses `switchMap`
so a program swap unsubscribes the previous program's whole branch including
its host effects, and `4_http.ts:298` uses `takeUntil` so a dropped SSE client
ends its inner. Row granularity is the only key at which teardown does not
happen. This is not a new mechanism; it is the existing mechanism at a finer
key.

The sibling runtime `v6/dl/src/1_hosts.ts` has the same deafness by a different
route (it reads pending rows rather than deltas, and its `concatMap` at :433 is
described in its own comment as "the serialization lock"). It is not the graded
runtime and is a second, lower-priority site.

### What breaks

*The endurance goal:* nothing, and this is the part worth checking rather than
assuming. Teardown deletes a `pending` witness row. `WitnessCache.clearDeadLocks`
(:86-90) already deletes every `pending` row at subscribe time, on the stated
reasoning that a single-process runner cannot have anything in flight at boot.
A torn-down witness is therefore in exactly the state a crashed one is already
in, and boot replay already handles it: the demand row is durable, so if the
demand is still there at boot it refires, and if it is not, it should not.

*Exactly-once:* unaffected on the answered path. The `done` state is what
suppresses a refire, and teardown never writes `done`. The honest change is on
the *unanswered* path, and it is in the correct direction: today a torn-down
effect's answer lands and is memoized forever, so a demand that returns is
served from a cache entry produced by a run nobody wanted.

*The content-addressed cache:* unaffected by construction, and this is the
`salt_minting` ruling doing its job. Under content salts a fill is a cache
update addressed to `(identity, witness)`, valid for every scope that ever
demands that identity. A torn-down run simply produces no cache update.

*`__host_response_*` growth:* **improves, and this is the strongest practical
argument for the change.** R2 measured 2 response rows where exactly 1 was ever
wanted. Every superseded in-flight effect permanently stores an answer for a
demand that no longer exists. On the flagship shape (a route changing while a
fetch is in flight) that table grows with user impatience and never shrinks.

## Hard question 2: does door 1 obey the `effect_abort` ruling?

**It obeys it. No amendment is needed, and proposing one would be misreading
it.** The ruling is `best_effort_cancel_on_support_zero`, and its content is
that cancellation is a cost optimization and never semantics: "no one stop
arrow, no arrow stop exist, is lie", a cancelled effect MAY still have spent or
landed, and correctness never depends on cancellation having worked.

Door 1 is exactly that. R2 shows the current system is already correct without
any cancellation (`body` holds one row, the right one), so adding teardown
cannot change a correct answer into a wrong one. It changes how much the
machine spends getting there. The ruling further requires a painted warning at
the abort site and a debug line per abort attempt and outcome; that obligation
is unbuilt along with everything else and rides the implementation.

The one thing an implementer could get wrong is treating teardown as a
guarantee. The `sh` host in the flagship writes to a ledger before its sleep, so
a torn-down run has already had its side effect. That is the ruling's own
invariant, and the receipt for it is in `labs/teardown-flatten/receipts.sh`:
the switch ledger's `torn down r1` line sits after a `start r1` that already
appended to the file.

## Hard question 3: the concatMap serialization

Separate from cancellation, graded separately in R5, and on the flagship shape
it is the **larger** cost. Two independent jobs take the sum of their durations
rather than the max. Under supersession the two costs compound: `concat` makes
the loser block its own successor for the loser's full duration, so the user
waits for a fetch they already navigated away from before the one they want
starts.

Serialization and cancellation are not independent at site 2, though. `concat`
and `switch` are the same seam, and the choice of flattener there decides both.
Teardown of an *in-flight* inner is compatible with `concatMap`
(`concatMap(demand => run(demand).pipe(takeUntil(delFor(demand))))` works and
lets the queue advance early). Teardown of a *queued, not yet subscribed* inner
is not, because `takeUntil` only listens once its inner is subscribed and a
`del` that arrives while the demand is still in the queue is simply missed. So
`concat` plus teardown needs an extra check at dequeue time. That is card 5.

**What would prove no regression** for moving to bounded `mergeMap`, in the
order I would want them:

1. `rxoracle` still 8/8, with `host_concurrency`'s declared expectation
   *flipped* and the flip stated. It currently asserts the ledger `start j1 /
   done j1 / start j2 / done j2` as a `diverges` result against mergeMap's
   overlap. Bounded merge with a limit above 1 makes that case `exact`. A
   change that makes an expected divergence close is exactly what that harness
   was built to make loud, and it should be allowed to be loud.
2. `marksExact` ordering receipts for a bound of 1, proving the bound is
   honoured and that `concurrency(1)` is genuinely the current behavior, so the
   change is a generalization rather than a replacement.
3. The `sprefa_extract` invocation grouping (`groupInvocations`, :365-384) still
   coalescing: it groups compatible extractor projections in one frontier, and
   that grouping is applicative reasoning that must not silently degrade into N
   spawns under concurrency.
4. `just green-all`, specifically `memory-soak` and `leak-soak`. Unbounded
   `mergeMap` over a demand stream is the classic unbounded-spawn shape, and
   the soak already asserts handles flat by type.
5. A count test, per the standing repo law that formerly-quadratic paths get
   count tests rather than end-state equality: spawns-per-tick flat as the
   corpus grows.

## Hard question 4: where does the ordinal belong?

**Nowhere. The lab's answer is that no ordinal is needed at all**, which is why
the four-flattener table has no row that wants one.

The sketch proposed an ordinal so that "which inner is newest" becomes ordinary
data and switch becomes a rule retracting non-latest rows. The measurement says
the retraction is already there and already carries the decision. Pricing the
three placements anyway:

| candidate | price | verdict |
|---|---|---|
| a column on the demand rel | every demand-producing rule must thread it; the demand rel is currently unkeyed by deliberate ruling (`rel_default_policy = value_unkeyed`) and this would key it; and it is **redundant**, since the surviving row after a `key(1)` replace already is the newest | rejected, cost with no benefit |
| the tick number | free, `now/1` is already live (`registry.pl:60`) | rejected, and it fails at precisely the point of interest: two demands in the same tick share a tick number, and R6 showed intra-tick is exactly where the ambiguity lives |
| a new witness column | `__host_response_*` already has an `ordinal` column with a different meaning (multi-row host answers, added by the phase-2 extraction arc, `1_host_expand.pl:467`) | rejected, name collision on a live column |

An ordinal is only needed to express "newest wins" *without* a keyed relation,
which is the thing keyed relations already do. Recommendation: no ordinal, and
if one is ever wanted, the argument has to come from a program that cannot be
written with a key.

## Cards

Each card is written the way the user would type it before it is written the
way it lowers. None of cards 1, 4, 5 needs new syntax.

### Card 1 — teardown on `del`. Needs a ruling only on the opt-out.

No new syntax. This is the flagship program, byte-unmodified, and it already
means switch:

```
rel route_change(session_id: text, route_id: text) log keep(all).
rel open_route(session_id: text, route_id: text) key(1).
rel body(session_id: text, route_id: text, payload: text).

sh fetch_body(route_id: text) -> (payload: text) = `curl -s "{route_id}"`.

open_route(SessionId, RouteId) <+ route_change(SessionId, RouteId).
body(SessionId, RouteId, Payload) <- open_route(SessionId, RouteId), fetch_body(RouteId, Payload).
```

**1a, teardown is unconditional.** `key(1)` retracts, the demand `del`s, the
subprocess dies. Nothing in the source mentions cancellation, which is the
point: the program says what it wants to be true and the runtime stops paying
for what it no longer wants. This is what `effect_abort` already rules.

**1b, opt-out on the host declaration**, for an effect whose partial execution
is worse than its wasted execution:

```
sh charge_card(order_id: text) -> (receipt: text) uninterruptible = `...`.
```

The argument for 1a alone: the ruling already says a cancelled effect may have
spent, so `uninterruptible` promises something the system cannot deliver and
would read as a guarantee. The argument for 1b: a shell command mid-write is a
real hazard and the runner already treats generic `sh` as effectful (it refuses
to coalesce two identical generic invocations for exactly that reason,
:359-364). **Ruling wanted: 1a alone, or 1a plus 1b.** The lab leans 1a alone
and thinks 1b is better served by the effect being idempotent, but this is a
user call because it is a promise about the world.

Pure-rx lowering, both:

```ts
demand$.pipe(
  groupBy(demand => demand.witnessDigest),
  mergeMap(perWitness => perWitness.pipe(
    switchMap(demand => demand.sign === "add" ? runHost(demand) : EMPTY))))
```

Graded in `flatten-probe.ts` as `switchFlattener`; the `del` is the switch
trigger and `EMPTY` is the teardown.

### Card 2 — `exhaust` needs an in-flight relation. This is the only new surface.

**2a, the witness cache becomes an ordinary readable relation:**

```
rel in_flight(host: text, slot: text).

open_route(SessionId, RouteId) <+ route_change(SessionId, RouteId), not(in_flight('fetch_body', SessionId)).
```

**2b, the same guard as a match arm**, which reads better left to right and is
the shape the ratified `|+>` arms were minted for:

```
match route_change(SessionId, RouteId) {
  ; not(in_flight('fetch_body', SessionId)) |+> open_route(SessionId, RouteId)
}
```

Is this TIER 0? **The relation is; the construct is not.** No new construct
appears in either spelling: `not/1` is live, `match/2` is live, `|+>` is
ratified. What is new is that `in_flight` exists at all. The rows already exist
in `__host_witness` and are runtime-private. So this is a visibility decision,
and it is the same class as the `compound_storage = struct_as_rows` ruling's
boundary-invisible storage plane, being asked to become visible in one specific
place.

The honest cost, and the reason this is a card and not a recommendation: an
`in_flight` relation makes the *world's* timing readable by rules, so a program
can derive different rows depending on how long a subprocess took. That is a
larger change to what the language is than adding an operator would be, and the
`slot` column has no definition yet (card 2's real open question, since
`identity_digest` and `witness_digest` are both content-addressed over the full
input tuple and neither is a slot).

Pure-rx lowering: `groupBy(slot) -> exhaustMap(run)`, graded as
`exhaustFlattener`.

### Card 3 — bounded concurrency. Needs a ruling on whether it is language at all.

**3a, on the host declaration:**

```
sh fetch_body(route_id: text) -> (payload: text) concurrency(4) = `curl -s "{route_id}"`.
```

**3b, no surface at all** — a runner flag, `--host-concurrency 4`, defaulting to
something above 1.

The argument for 3b: R5 established that `merge` and `concat` are the same
program, so a number that picks between them is a property of the machine, and
the same program on a bigger box wants a bigger number. The argument for 3a: a
rate-limited API and a local `grep` want different numbers in the same program,
which no single flag can express, and the declaration is where a host's other
world-facing facts already live. **Ruling wanted.** The lab leans 3a with a
default, because the per-host case is real and 3b cannot express it, but notes
that `concurrency(1)` then becomes a way to write a serialization lock into a
program, which is a machine property leaking into source.

Pure-rx lowering: `mergeMap(run, limit)`. `limit = 1` is `concatMap` exactly,
which is the current behavior and makes this strictly a generalization.

### Card 4 — intra-tick sign order. Needs a ruling. No syntax.

From R6. Within one tick, does the effect plane process `del` before `add`, or
`add` before `del`? The tick log does not say, both readings are faithful, and
they produce different ledgers. Free while concurrency is unbounded; a real
decision under a bound, where `del` first frees the slot and `add` first can
exceed it.

**4a, `del` before `add`, always.** Teardown precedes construction, the bound
is never exceeded, and it reads as "stop wanting the old thing, then want the
new one".

**4b, `add` before `del`.** Matches the byte order `ticklog.ts` already emits
and keeps the effect plane's reading identical to the log's.

The lab recommends 4a on the concurrency argument alone, and notes 4b's
argument is about matching a serialization order that N2 already documents as
carrying no meaning.

### Card 5 — dropping a queued demand under `concat`. No syntax.

Under `concat`, a demand can retract while still in the queue, never having been
subscribed. `takeUntil` cannot catch that, so the effect runs anyway and the
teardown is silently a no-op for exactly the demands that were cheapest to
cancel. Two spellings, both runtime:

**5a**, check liveness at dequeue: `concatMap(demand => stillDemanded(demand) ?
run(demand) : EMPTY)`, one extra read per dequeue.

**5b**, keep a live set updated from both signs and filter the queue against it,
no read.

Not a user card unless the answer is "do not fix it", in which case the silence
needs naming.

### Card 6 — what happens to `unsubscribe/1`. Needs a ruling.

`unsubscribe/1` is registry status **reserved** with lower role
`wrapper(rel_atom, refuse(lifecycle))` (`v6/prolog/compile/registry.pl:50`), and
rxoracle's `unsubscribe_teardown` case exists to assert it stays refused
(`bop check` exit 2, `unsupported_construct(lifecycle_arm(unsubscribe))`).
Card 1 makes teardown a real runtime event for the first time, so the word is no
longer reserved for something that does not happen.

**6a, it stays refused.** Teardown is best-effort per `effect_abort`, so making
it observable would let a program derive rows from something the ruling says is
not semantics. This is the conservative reading and the lab's lean.

**6b, it becomes live as an observation on the demand plane**, which is what a
program would actually want to write:

```
rel abandoned(route_id: text) log keep(all).

abandoned(RouteId) <+ unsubscribe(open_route(SessionId, RouteId)).
```

The problem with 6b is visible in its own lowering, and it is measured as `R7`
against `labs/teardown-flatten/finalize-already-observes.dl6`: `finalize/1` is
already live and already means per-row retraction on the departure frontier, so

```
abandoned(RouteId) <+ finalize(open_route(SessionId, RouteId)).
```

derives `abandoned = [["r1"]]` on the supersession **today**, with no host, no
effect plane, and no teardown existing at all. So 6b would be a second
spelling for a live construct, distinguished only by whether a subprocess was
actually killed, which is precisely the non-guarantee. That argument favours
6a, and it also answers the rxoracle case's own note that `finalize` "is not the
same event": it is not, but the difference is unobservable by ruling.

## Dependence on the other lane

`lane/rel-as-stream` is labbing whether a rel can BE a stream. The two answers
touch at one point and are otherwise independent.

The touching point is **card 2**. If a rel can be a stream, then `in_flight` is
not a new relation to be made visible but a view on a stream the runtime
already has, and card 2 stops being a visibility decision about storage and
becomes an instance of that lane's general answer. If a rel cannot be a stream,
card 2 stands as written and needs its own ruling. Nothing else here depends on
it: cards 1, 4 and 5 are runtime changes under existing syntax, card 3 is a
number, and card 6 is a refusal that stays or goes on its own argument.

The reverse dependence is worth naming too. This lab establishes that `del` at
the effect plane is a *sufficient* teardown signal without any slot key. If the
other lane concludes rels are streams and proposes subscription identity as a
consequence, that conclusion should not re-introduce a per-subscription id on
the effect plane, because `salt_minting = content_addressed` already ruled
against exactly that ("the salt is witness data, never a subscription id") and
this lab's switch result shows none is needed.

## What this lab did not do

No production file was edited. The probe measures a signal and runs its effects
*beside* the engine's rather than patching the runner, so what it proves is that
the demand stream is sufficient input to every flattener, not that a particular
patch works. An implementation still owes: the four sites in hard question 1,
the ruling's warn-paint and per-abort trace line, and a fail-first receipt that
goes red before the fix. The `exhaust` case is the only one whose expectation
would change under a real implementation, since the probe's slot derivation is
not the language's.
