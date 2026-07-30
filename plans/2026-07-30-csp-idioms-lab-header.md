# CSP idioms lab — header (planner-seeded contract)

User direction (2026-07-30): "make an opus lab with goal of making toy
programs that fit the problem domain or are essential to the domain of csp
and we test it out and lab out the ugly and repetitive nature of it."

This executes the standing sugar discipline (ruling `scan_surface`): write
real programs with shipped constructs, let the ugliness show up under
repetition, and sugar afterwards from evidence, never from a guess.

## Seed receipt (coordinator, 2026-07-30, both doors)

The composed producer/queue/bounded-channel program below compiled clean
through `bop check` and ran through `dl6_oracle.pl` with the expected log:
exactly one take per drain tick, order preserved, zero loss, keep(count(3))
eviction VISIBLE as a minus delta (the just-landed retention-minus), empty
drain takes nothing. One refusal hit en route and it was right:
`edge_into_unkeyed_set(taken/1)` until `taken` got `key(1)`.

```
rel produce(payload: text)                 log keep(all).
rel drain(cause: text)                     log keep(all).
rel pending(ordinal: int, payload: text)   log keep(all).
rel cursor(name: text, at: int)            key(1).
rel taken(ordinal: int)                    key(1).
rel chan(ordinal: int, payload: text)      log keep(count(3)).

cursor('q', 1)            <+ produce(_Payload), not(cursor('q', _At)).
pending(1, Payload)       <+ produce(Payload), not(cursor('q', _At)).
cursor('q', NextAt)       <+ produce(_Payload), pre(cursor('q', At)), NextAt := At + 1.
pending(NextAt, Payload)  <+ produce(Payload), pre(cursor('q', At)), NextAt := At + 1.

item(Ordinal, Payload)    <- pending(Ordinal, Payload).
ready_min(min(Ordinal))   <- item(Ordinal, _Payload), not(taken(Ordinal)).

taken(Ordinal)            <+ drain(_Cause), latest(ready_min(Ordinal)).
chan(Ordinal, Payload)    <+ drain(_Cause), latest(ready_min(Ordinal)), latest(item(Ordinal, Payload)).

backlog(count(Ordinal))   <- item(Ordinal, _Payload), not(taken(Ordinal)).
```

## The idiom set (each = one program, graded, ugliness measured)

Essential CSP/Go/occam idioms. Every program runs through BOTH doors
(dl6_oracle + the compiled runtime; the served leg via curl where the idiom
needs a clock bind — labs/rel_as_stream/receipts.sh shows both leg patterns).

1. **buffered channel** — the seed program, promoted to the lab's baseline.
2. **worker pool** — N consumers, one queue, each item taken by exactly one
   worker; work-stealing fairness stated (term order will pick — measure it).
3. **pipeline** — 3 stages, each a channel, item flows stage 1 -> 2 -> 3;
   measure ticks per item per stage (effect-chain lab found N+1 per stage on
   HOST chains — is a pure-rel pipeline 1 tick per stage or free?).
4. **fan-out / fan-in** — one producer to N channels, N producers to one
   channel (merge = one cursor over N producers, stream lab R4 cites).
5. **select / alternation** — first-available of two channels wins, loser
   stays queued; the CSP primitive most likely to be inexpressible or ugly.
   If arrival-order-within-tick decides the winner, name the determinism
   contract it leans on.
6. **timeout** — select between a channel and a clock (clock bind on the
   served leg; a scheduled `tick` rel on the oracle leg).
7. **done/cancellation channel** — a `done` row stops all takers; measure
   whether stop is same-tick or next-tick and whether in-flight items leak.
8. **rendezvous (unbuffered channel)** — producer's item is taken the same
   drain it becomes ready or not at all held? Grade what "capacity 0" even
   means here (keep(count(0)) is deliver-and-forget — probably the wrong
   tool; say why).
9. **semaphore / rate limiter** — at most K concurrent leases; lease/release
   as rows; the clock-joined variant is the rate limiter.

## What "lab out the ugly" means, measurably

- **Boilerplate census**: for each idiom, count decls/rules total and count
  the rules that are VERBATIM-SHAPE repeats of the cursor/ready/take triple.
  A table: idiom x (total rules, repeated-shape rules, novel rules). The
  sugar candidate is whatever shape tops that census — expected: the
  4-rule cursor numbering block (already ruled as card 1b `seq(name)` sugar,
  unwired) and the take-one pair.
- **Error surface census**: every mistake the author (you) actually makes
  cold-writing these — wrong base case, missing key, forgotten not(taken) —
  and whether the door said something useful. B4-class silent outcomes are
  findings.
- **Refusals hit**: named = fine; silent-wrong = defect, fail-first fixture.
- **Determinism**: any idiom whose answer depends on within-tick arrival
  order or term order gets that dependence STATED and graded (two orderings
  run, logs diffed — the match-frontier C2 method).
- **Cross-check**: each idiom's expected behavior stated first as the Go
  equivalent (5-line Go in a comment, unexecuted) so the grading has an
  external referent.

## Named slots

- slot_select_spelling: if select needs a construct, price it; if it needs
  only term-order luck, that is a finding not a feature.
- slot_rendezvous_meaning: what capacity-0 means in tick semantics.
- slot_seq_sugar_shape: confirm or amend the ruled `seq(name)` card-1b shape
  from the census evidence (this lab is the evidence that ruling asked for).
- slot_fairness: worker-pool assignment under term order — acceptable
  contract or needs a knob.

## Receipts required to land

- Every idiom: program text + schedule + oracle log + compiled/served log,
  byte-diffed where the legs share a schedule; refusal-path receipts.
- The two census tables (boilerplate, errors).
- Fixture-promotion candidates for the corpus (at least: buffered channel,
  select, done-channel).
- Verdict doc `plans/2026-07-30-csp-idioms-verdict.md`; lab files under
  `v6/prolog/labs/csp_idioms/`, die on landing.
