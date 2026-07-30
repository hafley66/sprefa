# CSP idioms lab — verdict

Contract: `plans/2026-07-30-csp-idioms-lab-header.md`. Lab files:
`v6/prolog/labs/csp_idioms/` (die on landing). Base sha `c1518683`.

Receipts: `bash v6/prolog/labs/csp_idioms/receipts.sh` → **18 PASS 0 FAIL**.
Census: `python3 v6/prolog/labs/csp_idioms/census.py`.
Battery unchanged: `just conformance` exit 0, 226 PASS / 0 FAIL, zero fixtures
added by this lab.

## Headline

Nine CSP idioms were cold-authored in dl6 and run through both doors. **Eight of
nine are expressible and eight of nine agree byte-for-byte across the two
graded engines.** The language is not missing CSP constructs.

What it is missing is a way to say "queue" once. **73 of 94 rules (78%) are
verbatim-shape repeats**, and the single template `cursor`/`pending`/`item`/
`ready` accounts for 52 of them. The ruled `seq(name)` sugar (stream card 1b) is
confirmed by this evidence and should be **widened**: numbering alone covers 32
rules, the full queue template covers 52.

Separately, three CSP *correctness* properties are silently violated, all by one
root cause, and one program diverges between the two engines.

## Per-idiom results

| # | idiom | decls | rules | both doors | verdict |
|---|---|---|---|---|---|
| 1 | buffered channel | 6 | 9 | identical (62 deltas) | correct; eviction visible as a minus delta |
| 2 | worker pool | 6 | 9 | identical (34 deltas) | **exactly-once VIOLATED** (W1) |
| 3 | pipeline (3 stages) | 4 | 3 | identical (8 deltas) | correct, 1 tick/stage, fully pipelined |
| 4a | fan-in | 4 | 6 | identical (25 deltas) | correct |
| 4b | fan-out (N readers) | 7 | 11 | identical (62 deltas) | correct; N readers cost 0 rules |
| 5 | select / alternation | 10 | 16 | identical (29 deltas) | correct for priority; loser stays queued |
| 6 | timeout | 5 | 6 | identical (22 deltas) | correct |
| 7 | done / cancellation | 7 | 10 | identical (23 deltas) | correct, same-tick, no leak |
| 8 | rendezvous | 9 | 19 | **DIVERGES** (W4) | expressible, +1 tick latency |
| 9 | semaphore K=2 | 3 | 5 | identical (37 deltas) | **cap BREACHED within a tick** (W2) |

Discriminating log lines are quoted per finding below.

### Where dl6 beats the Go referent

- **(7) cancellation is deterministic.** `done` and `drain` in the same tick →
  the drain is blocked, nothing taken. Go's `select` is *random* when both arms
  are ready, so a cancelled Go loop may still consume one more job. dl6's
  answer is defined.
- **(5) select's loser genuinely stays queued.** Tick 4 has both channels ready,
  `a` wins, and `b2` is still there to be taken at tick 5. This is the property
  most likely to be got wrong and it holds.
- **(4b) N readers cost zero extra rules.** A reader is a row (`attach`), not a
  rule. The fan-out machinery is written once and generalises.
- **(3) the pipeline is free.** 3 rules, 4 decls, no queue template at all,
  1 tick per stage with full pipelining (item `b` trails `a` by exactly one tick
  throughout).

## Finding W1 — worker-pool exactly-once is violated, silently

Two workers polling in the **same tick** both read the same `ready_min` and both
receive item 1:

```
tick 2  assigned +[1,"w1","a"]   assigned +[1,"w2","a"]   taken +[1]
```

`taken` is keyed so it collapses to one row and the queue advances once — but
`assigned`, the rel that represents work actually handed out, carries the item
twice. Three items produced, four assignments. No refusal, no warning, both
doors agree.

## Finding W2 — semaphore cap is breached within a tick

Three `acquire` rows arriving together with nothing held all pass the
`Outstanding < 2` guard, because each reads the same frozen pre-edge state:

```
tick 7  granted +[11,"r1"] +[12,"r2"] +[13,"r3"]   held_count +[3]
```

`held_count` settles at **3** against a declared cap of 2.

### W1 and W2 are one root cause

Any capacity or exclusivity guard written as *"read an aggregate, compare it,
then write"* is enforced **per tick boundary, never per row**. N same-tick
arrivals all observe the same pre-edge state and all pass. This generalises the
consumption-arms pacing finding ((b) one-per-drain-tick is the only spelling
that implements a queue) from queues to *every* counted resource: worker pools,
semaphores, and rate limiters all break identically.

This is the most consequential result in the lab. It is not a spelling problem —
the programs are the natural ones — and the workaround (force every consumer
into its own tick) is exactly the constraint the consumption-arms verdict priced
as "the drain cap becomes a queue-length cap".

## Finding W3 — `count()` has no zero, and it kills a program silently

The first semaphore draft (`semaphore_naive.dl6`, kept as the receipt) grants
**zero** leases. `count()` over an empty set yields *no row*, not `0`, so
`latest(held_count(Outstanding))` matches nothing before the first grant — and
because nothing is granted the set stays empty forever. The semaphore is
permanently closed.

It compiles clean through both doors (`bop check` exit 0), runs to completion,
and produces a tick log containing only the `acquire` arrivals. No diagnostic
anywhere. This is design-review **A11 ("count never 0") biting a real program**.

The fix is a second rule with a `not(held_count(_))` base case — i.e. the *same*
base/step split the cursor block already needs. So A11 does not merely produce
one bug; it is a standing +1 rule tax on every aggregate compared to a threshold.

## Finding W4 — a two-door divergence on derived-trigger programs

Rendezvous fires off a **derived** trigger (`beat`), so the served engine needs
an extra carry tick per meeting: **15 served ticks against the oracle's 8**. Tick
count alone is absorbed by the standard per-rel normalization. What is not
absorbed is that an **aggregate over the carried rel takes a transient value
during the extra tick and publishes it**:

```
oracle   waiting_receivers +[1] (tick 3) ... -[1] (tick 7)      2 deltas
served   waiting_receivers +[1] -[1] +[1] -[1] +[1] -[1]        6 deltas
```

The oracle never observes the intermediate state; the served engine does. Both
engines *accept* the program, so this is a tick-log divergence on the
cross-target log contract — the same class as review-A4, with the same cause of
it going unnoticed (zero fixture coverage). Non-aggregate rels in the same
program (`met`, `waiting_offers`) match exactly, which is why only aggregates
expose it.

Plausibly the same row as ARCH `extra_drain_tick`; not asserted as identical,
since that row was filed against refCount re-assertion and this is a derived-
trigger carry. **Recommend: fixture + ownership.**

## Boilerplate census (mechanical; `census.py`)

Every rule normalised to its shape — rel names, atom constants, variables and
integers all replaced by placeholders — so only structure survives. Two rules
with the same normalised text are verbatim-shape repeats.

```
idiom                   decls  rules  repeated   novel
1  buffered channel         6      9         8       1
2  worker pool              6      9         8       1
3  pipeline                 4      3         3       0
4a fan-in                   4      6         2       4
4b fan-out                  7     11         7       4
5  select                  10     16        14       2
6  timeout                  5      6         3       3
7  done channel             7     10         9       1
8  rendezvous               9     19        18       1
9  semaphore                3      5         1       4
TOTAL                             94        73      21
```

**78% of the corpus is repeated shape.** Ranked:

| occurrences | idioms | shape |
|---|---|---|
| 12 | 5 | `ready(min(O)) <- item(O,_), not(taken(O)).` (and the `count` twin) |
| 8 | 6 | `item(O,P) <- pending(O,P).` |
| 8 | 6 | `cursor(K,1) <+ Src(_), not(cursor(K,_)).` |
| 8 | 6 | `cursor(K,N) <+ Src(_), pre(cursor(K,A)), N := A+1.` |
| 8 | 6 | `pending(1,P) <+ Src(P), not(cursor(K,_)).` |
| 8 | 6 | `pending(N,P) <+ Src(P), pre(cursor(K,A)), N := A+1.` |
| 4 | 4 | `taken(O) <+ drain(_), latest(ready(O)).` |
| 4 | 3 | `answered(Id) <- response(Id).` (rename-only projection) |

The four numbering shapes are **32 rules, 34% of the corpus, in 6 of 10
programs**. Adding the `item` view and the `ready` aggregate: **52 of 94, 55%**.

Two idioms need the block **twice** (select: two channels; rendezvous: two
sides), which is where rule counts hit 16 and 19.

### slot_seq_sugar_shape — CONFIRMED, with an amendment

The ruled `seq(name)` card-1b shape is confirmed: the numbering block is
overwhelmingly the top repeat, exactly as predicted. **Amendment from the
evidence:** sugar covering numbering alone captures 32 rules; sugar covering the
whole queue template — numbering + `item` view + `ready` selector + the take
pair — captures 52 and eliminates the entire body of every queue-shaped idiom.
The take rule is the one place programs genuinely differ (worker pool adds a
worker column, select adds a priority guard, done adds a cancellation guard), so
the honest split is: **sugar the numbering + view + selector; leave the take rule
written by hand.** That is 40 of 94 rules removed with no expressiveness lost.

The header predicted "the 4-rule cursor numbering block and the take-one pair".
The census says the take pair is only 4 occurrences and is the part that *varies*
— so the prediction is half right, and the `item`/`ready` pair (20 occurrences)
is the under-counted half.

## Error-surface census — the mistakes actually made

| # | mistake | what the door said | cost |
|---|---|---|---|
| E1 | any parse failure | `dl_parse_error(statement,[114,101,97,...])` — a raw **character-code list**, no file, no line, dumping from the failure point to EOF | had to **write a decoder** (`decode_err.py`) to read my own error |
| E2 | named a rel `subscribe` (reserved, registry.pl:52) | the E1 dump — the reserved word is never named | 4 bisection steps; the message misdirected me to the *arithmetic* (I tested literal `0`, literal `1`, then a `:=` bind before suspecting the **name**) |
| E3 | `count()` compared to a threshold with no base case | **nothing** — clean compile, clean run, dead program | one full run + a bisect; found only by reading the log |
| E4 | two workers polling in one tick | **nothing** — W1 | found only by reading the log |
| E5 | three acquires in one tick | **nothing** — W2 | found only by reading the log |
| E6 | rendezvous has no drain: either side can complete the pair | n/a (design-time) | without a merged `beat` trigger rel, every meeting rule must be written **twice**, once per trigger side |
| E7 | `overdue` never appears in the timeout tick log | **nothing** | the rel that *decides* the timeout is invisible (mid-tick level row, net-zero delta) — review A6 in the wild; makes the guard undebuggable |

**E1 is the finding to act on.** The design review called B4 ("refusals print as
swipl Unknown message with no file/line") "the worst part of the cold-author
experience". This lab found it is worse than recorded: for parse errors the
message is not merely unlocated, it is **not human-readable at all**, and the
sole reserved-word case that *is* well-diagnosed is the one the author is least
likely to hit. E2b proves the good path exists (`unsupported_construct(
lifecycle_arm(subscribe))`) — it is reached only when the argument happens to
parse as a rel atom.

Note the asymmetry across E3/E4/E5: **every semantic error in this lab was
silent.** Not one was caught by either door. The only loud failures were
syntactic, and those were unreadable.

## Named slots

- **slot_select_spelling — no construct needed; not a feature.** Priority select
  is expressible and correct today (16 rules, both doors identical, loser
  provably preserved). Round-robin is expressible too (an explicit turn rel).
  Go's *uniformly random* select is **not** expressible — there is no randomness
  source — but that is a fidelity gap nobody should want to close: determinism is
  the more useful contract, and the lab recommends **recording the divergence
  from CSP rather than adding a construct**. The ugliness in select is not the
  selection, it is the two duplicated queue blocks, which the seq sugar removes.
- **slot_rendezvous_meaning — capacity 0 is a JOIN, never a retention policy.**
  `keep(count(0))` is deliver-and-forget (rel_as_stream receipt (c)): an offer
  with no receiver present is *lost*. Rendezvous requires the opposite — the
  offer must *wait*. So capacity 0 means "meet only when both sides are present",
  which is a join condition, and **both sides therefore need queues**. Cost: two
  numbering blocks (19 rules) and **one extra tick of latency**, because the
  numbering block is itself an edge write, so a derived trigger necessarily lags.
- **slot_seq_sugar_shape — confirmed and widened.** See the census section.
- **slot_fairness — the header's framing is refuted, and the truth is worse.**
  The header asked whether term order picks the winner and whether that is an
  acceptable contract. Term order does **not** pick a winner: *both* workers win
  (W1). Determinism receipts D1/D2 show within-tick **arrival** order changes
  nothing — the answer is a function of term order, which is the reproducible and
  desirable half. The defect is not fairness, it is **exclusivity**. A knob for
  fairness would not help; what is needed is per-row consumption, which is the
  W1/W2 root cause.

## Fixture-promotion candidates

Handed back, not promoted by this lab (the contract adds no fixtures):

1. **`buffered.dl6`** — the baseline. Identical both doors, exercises keyed
   cursor, log retention with a visible eviction minus delta, `latest`, `pre`,
   `min`, and an empty drain. Strong general regression fixture.
2. **`select.dl6`** — identical both doors; pins the "loser stays queued"
   property, which nothing in the corpus currently covers.
3. **`done.dl6`** — identical both doors; pins same-tick cancellation.
4. **`workerpool.dl6` as a FAIL-FIRST fixture** for W1 (exactly-once). Currently
   green-as-wrong; it should be red until per-row consumption exists.
5. **`semaphore.dl6` as a FAIL-FIRST fixture** for W2 (cap breach), same status.
6. **`semaphore_naive.dl6`** for W3 — either a refusal target (an aggregate
   compared to a threshold with no reachable base case is arguably statically
   detectable) or a documented worked example.
7. **`rendezvous.dl6`** for W4 — the two-door divergence needs coverage
   regardless of which side is judged correct.
8. **`pipeline.dl6`** — cheap, and pins 1-tick-per-stage carry semantics.

## Recommendations, ranked

1. **Rule on per-row consumption (W1/W2).** Three of nine idioms are silently
   wrong for one reason. This is a semantics decision, not a sugar decision, and
   it blocks worker pools, semaphores and rate limiters — all of which the alpha
   will want.
2. **Make parse errors readable (E1/E2).** Decoding char codes by hand is not a
   viable cold-author experience, and the reserved-word case has a good path that
   simply is not reached. Low cost, high felt value.
3. **Widen `seq(name)` to the queue template (52 rules).** Confirmed by census.
4. **Fixture + own W4** before another derived-trigger program is written.
5. **Consider a static refusal for W3** (aggregate threshold with no base case).

## Not done / limits

- Rate limiter (the clock-joined semaphore variant) was **not** separately built:
  it is the semaphore plus the timeout's clock join, and it inherits W2 exactly,
  so a fourth receipt of the same defect was judged not worth the rules. Stated
  rather than silently dropped.
- The served leg drives arrivals over HTTP with `sleep` pacing, so it exercises
  real drain boundaries but is not a virtual-time test.
- Two orderings were diffed for the two idioms with same-tick multi-arrivals
  (D1, D2). Idioms whose schedules have one arrival per tick were not perturbed,
  since there is nothing to reorder.
