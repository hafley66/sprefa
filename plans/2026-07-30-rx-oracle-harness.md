# rx-oracle harness -- grading sprefa's event sequence against literal rxjs

Lane `lane/rx-oracle`, base `609066ee0f5f9b5e64837371680092134c11c20f`.
Harness lives at `v6/tsv2/rxoracle/`; `bash v6/tsv2/rxoracle/run.sh` is the
entry point and exits 0 when every case matches its declared expectation.
The format and normalization spec is `v6/tsv2/rxoracle/README.md`; this file is
the verdict.

Zero production edits. `git diff base..HEAD -- . ':!v6/tsv2/rxoracle'` is empty.
Zero new syntax: the one case that needs a construct which does not exist is
recorded as such and its `.dl6` file is left in the natural spelling.

## What the harness is

One contrived scenario, written twice, diffed event by event.

* **leg A** -- a literal rxjs `.ts` file per case. It imports `rxjs` and node
  builtins and nothing from this repository, down to its own copy of the
  four-line `emit` helper. A reader can check it against the rxjs docs without
  knowing anything about sprefa.
* **leg B** -- the same scenario as `.dl6`, driven **only from bash**:
  `bop serve --port 0`, `curl` for the program load, `curl` for every arrival
  batch, `curl -N /ticks` for the capture. Nothing in leg B imports this
  codebase from TypeScript. python3 appears exactly twice, as a line
  timestamper (`lib/stamp.py`, because bash on macOS has no sub-second clock)
  and as a JSON-to-lines formatter (`lib/lines.py`); neither drives anything.

Hermetic: `SPREFA_CONFIG=/nonexistent/rxoracle.toml`, `DL_NO_DAEMON=1`,
`:memory:` db, ephemeral port read back off the server's own first stdout line,
one `mktemp -d` removed on exit. No daemon is contacted; `~/.local/state` is
neither read nor written.

## Decision 1: the line format and the shared clock

    <step> <name> <sign> <payload>

`<sign>` is `+` next/add, `-` del, `!` error, `.` complete. `<payload>` is a
canonical JSON array, which is already exactly what `runtime/ticklog.ts`
writes, so leg B does no re-encoding. `<name>` is the observable's name on leg
A and the relation's name on leg B, and that name is the ONLY alignment between
the two legs.

**The shared clock is the STEP, not the tick and not the wall.** rxjs measures
in scheduler frames, sprefa measures in ticks, and neither number means
anything to the other, so both legs are driven by a third clock instead:

* a case declares `stepMs`; step `k` is `[k*stepMs, (k+1)*stepMs)`
* an input batch for step `k` is delivered at that step's **midpoint** on both
  legs -- leg A schedules it on a `VirtualTimeScheduler` at frame
  `k*stepMs + stepMs/2`, leg B anchors its POST to the same nominal wall offset
* an event's step is `floor(time / stepMs)`: virtual frame on leg A,
  milliseconds since `t0` on leg B

Leg A is deterministic by construction. Leg B is policed rather than trusted:

* **the boundary guard.** An event landing within `guardMs` of a step boundary
  fails the run loudly as a straddle. A badly authored case says so instead of
  going red at random. Measured margins over the guard in the current corpus
  are 113ms to 245ms.
* **intra-step order is discarded** (N2), so jitter inside a step cannot change
  the answer at all.

**The guard earned itself immediately.** The first draft slept `stepMs` after
each POST, so each POST's own round trip accumulated at roughly 40ms per step;
by step 4 of `latest_sampling` the tick landed 167ms past its midpoint and the
guard failed the run. Every POST is now anchored to an absolute target derived
from `t0`, and measured offsets went flat at ~273ms. That defect would have
been an intermittent red diff without the guard.

## Decision 2: the normalizations

Four rules, each with a reason, all decided before the cases were run. None was
added to make a diff pass.

| id | rule | reason | scope |
|---|---|---|---|
| **N1 STEP-FOLD** | every tick a step produces, drain ticks included, folds into that step; the tick number never appears in a line | sprefa's tick number is an engine-internal drain counter with no rxjs counterpart, and it is not stable within sprefa either: `extra_drain_tick` is an unowned ARCH-row defect where a re-assertion produces one extra row-less tick, and the runtime-bridge arc measured the served engine producing 4 ticks where the schedule-fed oracle produces 3 for identical deltas. Grading it would grade a number sprefa already disagrees with itself about. | always |
| **N2 INTRA-STEP-SORT** | inside one step, lines sort by `(name, sign, payload)` | `runtime/ticklog.ts` already sorts `add`/`del` lexicographically and orders relations alphabetically, so leg B has no intra-tick order to compare against. Order ACROSS steps is preserved, so per-stream sequence is still graded. | always |
| **N3 SIGN-PROJECTION** | leg B's `-` lines are dropped | an rxjs `Observable` has no retraction channel; `next` is its only value-carrying notification and no combinator un-emits. A `-` line has no leg-A counterpart that could ever exist. | opt in; **opting in IS the finding** and every case below that does so carries "sprefa retracts here and rxjs cannot" in its row |
| **N4 INTERNAL-REL-HIDE** | leg B lines for `__`-prefixed relations are dropped | `__host_demand_*`, `__host_response_*`, `__host_witness` are compiler-minted storage-plane relations of the class the `compound_storage = struct_as_rows` ruling calls boundary-invisible; a program's author never wrote them. | always on; `showInternal` can opt a relation back in and **no case does**, deliberately -- see below |

`showInternal` is implemented and unused on purpose. `__host_response_<name>`'s
leading columns are a witness digest and an ordinal minted by the compiler, so
opting it in would produce a column of leg-B-only lines saying nothing except
"sprefa mints digests". The question that relation answers is measured instead
by the **receipts block**: `marksLines` / `marksExact` against the host spawn
ledger and `idbRows` against `GET /idb/:rel`, asserted in bash against the live
server before teardown. Receipts add nothing to either line file and cannot
make a diff pass; `marksExact` is a whole-file equality, so it proves ORDER and
not only count.

## The per-case table

| case | verdict | what it measures |
|---|---|---|
| `mergemap_accumulates` | **EXACT MATCH** | the control. A level rule over an accumulating source IS mergeMap's accumulation: nothing supersedes anything, a third request over an already-used key produces its own rows. 13 lines each, byte-identical. If this ever diverges the harness itself is wrong. |
| `latest_sampling` | **MATCH MODULO N3** | `latest()` in an edge body vs `withLatestFrom`. Identical event for event, including the trigger that arrives before the sampled source has any row: both drop it, neither had to be told to. The only leg-B lines with no counterpart are the `-` half of the keyed `config` replace. |
| `keyed_replace_vs_distinct` | **MATCH MODULO N3** | an identical value re-arriving on a `key(1)` head is zero-delta, which is exactly `distinctUntilChanged`. Five readings, two of them exact repeats: both legs emit three. Same N3 residue. |
| `same_tick_collapse` | **DIVERGES** | three transitions for one entity in ONE arrival batch. rxjs sees 3, sprefa's tick boundary reports the net 1. `alpha` and `beta` are events nowhere. No normalization opted into, so the whole gap is the diff. |
| `scan_state_feedback` | **DIVERGES** | `scan` vs `pre()`. `pre_occurrence_loop` landed 2026-07-30, so this COMPILES now (it was `edge_body_needs_pre` until today). Three single increments then two in one batch: the total reaches **5 and not 4**, so the ordered occurrence loop genuinely computed the intermediate and the second occurrence consumed it. It is still not an event. Sharper than the case above: computed, used, unobservable. |
| `host_concurrency` | **DIVERGES** | two host demands raised in one tick. `serve/1_hosts.ts` runs invocations under `concatMap`, so the second subprocess does not start until the first exits. Same two rows, steps `2 + 2` on rxjs against `2 + 4` on sprefa; spawn ledger is exactly `start j1 / done j1 / start j2 / done j2`. Defensible for shell effects, and not what `mergeMap` means. |
| `switchmap_inner_in_flight` | **DIVERGES** | the flagship, below. |
| `unsubscribe_teardown` | **INEXPRESSIBLE** | observing a teardown. `registry.pl` has `unsubscribe/1`, axis `time`, lower role `wrapper(rel_atom, refuse(lifecycle))`, status `reserved`: the word exists, the surface parses it, nothing is lowered behind it. `bop check` exits 2 with `unsupported_construct(lifecycle_arm(unsubscribe))`. The `.dl6` file is left in the natural spelling. `finalize/1` compiles and is NOT a substitute: the update-arm lab measured it as a per-row retraction firing the tick AFTER the departure, where an rxjs teardown ends the subscription synchronously at the moment its reason ends, so rewriting onto it would report a match for a different question. |

## The cancel-inner answer, measured

`switchmap_inner_in_flight` routes a session to `r1` (a 1.95s shell host),
routes to `r2` while `r1` is still in flight, lets `r1` answer, routes BACK to
`r1`, then lets `r2` answer. Every claim below is a receipt from that run, not
a reading of the code.

**Does the effect get aborted? No.** Spawn ledger, asserted as a whole-file
equality: `start r1 / done r1 / start r2 / done r2`. `r1`'s subprocess ran to
completion roughly a second after its demand row was retracted.

The code path matches: `HostRunner.liveDemand$` (`serve/1_hosts.ts`) reads
`delta.add` on the demand relations and **nothing reads `delta.del`**. There is
no `-delta` listener, so there is no site at which a cancel could fire.
`runShellLine`'s teardown does call `child.kill()`, but that teardown is only
reached when the invocation observable is unsubscribed, which happens on
program swap or server teardown, never on demand retraction. The
`effect_abort = best_effort_cancel_on_support_zero` ruling's owed lowering
(AbortSignal through the host run, cancel map, pending-row delete) is not built
here.

**Does the late answer land anywhere? Yes, durably.** The dead inner's answer
arrives as an ordinary arrival on `__host_response_fetch_body` and mints its
own tick, carrying that relation's delta and nothing else.
`GET /idb/__host_response_fetch_body` holds **2 rows** at the end of a run
whose user relation holds 1. It is never retracted; no `-` on that relation
appears anywhere in the capture.

**Does it derive anything? Not at the moment it lands.** The tick that carries
the late `r1` response carries zero `body` deltas, because `open_route` no
longer holds `(s1, r1)` and the level rule's join finds nothing. At the user
relation, at that instant, sprefa looks exactly like `switchMap`.

**Then the divergence shows up one step later, and it is bigger than the
cancel.** Routing back to `r1` derives `body(s1, r1, r1-body)` **in the same
tick as the re-demand**, with **no new spawn** (the ledger stays at two starts).
The stored response is a cache, and the in-process `claimed` set plus the
durable `__host_witness` guarantee the host does not re-run. rxjs's `switchMap`
re-subscribing runs the inner AGAIN and delivers one fetch-duration later. The
measured per-event difference:

    rxjs    03 body + ["s1","r2","r2-body"]      (the winner at that time)
            05 body + ["s1","r1","r1-body"]      (re-subscribed, 1.95s later)
    sprefa  03 body + ["s1","r1","r1-body"]      (cache hit, same tick, no spawn)

So the honest one-line answer: **sprefa does not cancel, it memoizes.** The
superseded effect completes, its answer is kept forever keyed on the witness,
and re-demanding the same witness is a synchronous cache read rather than a
re-subscription. `switchMap`'s user-visible "the loser never lands" is
reproduced, and its "re-subscribe re-runs" is not.

### How you would see the difference from outside

Four surfaces, all exercised by the harness:

1. **the tick stream** (`GET /ticks`): a tick carrying only
   `__host_response_<name>` deltas and no user-relation delta is a dead inner's
   answer landing. Under normalization N4 that tick contributes zero lines,
   which is why the flagship's divergence is measured on `body` and its
   internal-plane evidence on receipts instead.
2. **the relation itself** (`GET /idb/__host_response_<name>`): more rows than
   the program's own relations can account for. 2 against 1 in this run.
3. **the process side**: the subprocess runs to completion. Any host that
   touches the world -- writes a file, POSTs, spends money -- spends it after
   its demand is gone. The spawn ledger is the receipt shape.
4. **timing on re-demand**: a re-demand answered in the SAME tick is a cache
   hit; a re-demand answered a fetch-duration later would be a
   re-subscription. This is the discriminating observation, and it is
   observable from the tick stream alone with no internal relation shown.

### Named consequences, stated not fixed

* **an in-flight effect's cost is not bounded by its demand.** Cancellation is
  ruled to be cost optimization and never semantics, and the harness confirms
  the semantics are unaffected. The cost is not optimized at all today: zero
  `-delta` listener, zero abort path.
* **`__host_response_<name>` only grows.** No retraction appears in any capture
  and `__host_witness` durably keeps every witness claimed. I did not measure a
  GC or look for one, so this is an observation over one run and not a claim
  about the design.
* **serialized effects make the window bigger.** `host_concurrency` shows the
  runner is `concatMap`, so a superseded inner does not merely finish, it
  *blocks* its successor for its full duration. In the flagship, `r2`'s spawn
  did not start until `r1`'s -- already dead -- had exited.

## Steps I stopped rather than improvised

* **`bop check` accepts the `pre` program and exits 0.** I had planned
  `scan_state_feedback` as the inexpressible case on the strength of
  SCOREBOARD's `edge_body_needs_pre` bucket. Rather than assume a stale
  scoreboard or a broken door, I drove the program: it loads, and with a base
  case it folds correctly. `ARCH.pl` says `pre_occurrence_loop` landed
  2026-07-30. The case became a graded divergence and a different construct was
  found for the inexpressible slot. No production file was touched to make
  either outcome happen.
* **`__host_response_*` payloads are not comparable, so I did not invent a
  normalization to make them so.** The digest and ordinal columns have no
  leg-A counterpart. Rather than project them away with a case-specific rule
  (exactly the ad-hoc-regex failure the brief names), the evidence moved to the
  receipts block, which asserts counts and ledger order and touches neither
  line file.
* **No CLI flag and no serve change was needed or made.** Everything leg B
  needs -- ephemeral port, program load, arrivals, tick stream, relation read
  -- already exists on `bop` and `4_http.ts`.

## Cracks in the harness itself

* **`stepMs` and host durations are hand-tuned per case** so events land near
  step midpoints. The guard makes a bad choice loud rather than flaky, but it
  does not choose for you. The tightest measured margin in the corpus is 113ms
  over the guard.
* **the SSE capture races the first batch.** `run.sh` waits 400ms after opening
  `GET /ticks` and then asserts `"tick":1` is present, failing the run as
  broken rather than as a divergence if it is not. That is a guard, not a
  guarantee.
* **leg A's step alignment is by convention.** Each `leg-a.ts` hardcodes its own
  schedule in a header comment that must agree with `case.json`. Making leg A
  read the manifest would make it generic and stop it being a literal rxjs
  program a reader can check on its own, which is the whole value of leg A.
  Disagreement between the two shows up as a step-index divergence, which is a
  visible red and not a silent pass.
* **the corpus grades relations, not scheduling.** `!` and `.` are in the line
  format and no leg-B capture can ever produce them, because sprefa's tick log
  has no terminal notification. Error and completion propagation are not
  measured here at all.
