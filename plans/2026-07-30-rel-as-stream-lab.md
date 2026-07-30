# rel-as-stream lab

Lane `lane/rel-as-stream`, base `a4629623ff484eeb460487fbda96506980a091a6`.
Design record only. Zero production files changed, zero syntax proposed for
landing.

Runnable half: `v6/prolog/labs/rel_as_stream/`, entry
`bash v6/prolog/labs/rel_as_stream/receipts.sh` (exit 0 = every receipt held).
Current run: **12 reference-engine receipts + 7 two-door cases + durability +
sabotage, 10 PASS 0 FAIL, exit 0.**

The question, in the user's words:

> "i think rel as stream is also viable and can we get there using any existing
> mechanics without overbinding to some eventual overfitted opinionated rxjs
> lowering conceptually. scan sucks bc it forces a state model, but we need a
> way to express streams as well with what we have, so we can describe lowering
> target, or explain why we need new syntax that is tier 0 and not some lowering
> or new semantics to support the lowering."

---

## 0. The short version

**Yes, and the tier-0 list is empty.**

A rel is a stream when it is declared `log` and carries an ordinal column. The
sequence stops being an engine-internal stamp and becomes data, so it can be
joined, compared, aggregated, retained, and read back after a restart. Every
piece of that is live surface today: receipt (a) runs the program on the
reference engine and on the served emitter and the two logs match.

Three results are worth more than the build itself.

1. **The user's objection to `scan` is precise and the fix is not "remove the
   state".** The state model does not go away; receipt R3 pins that it is still
   one keyed rel and two rules. What goes away is the state model being the
   *only* observable. In one tick that batches two increments, the keyed cursor
   reports `1 -> 3` and the intermediate is gone, while the log rel reports
   ordinals 2 and 3 as separate rows. Same program, same tick, both rels
   (receipt R2). Datalog hands you state; a log rel plus an ordinal hands you
   the sequence of states, and it does it with constructs that already shipped.

2. **The whole rx Subject family is a two-word decl matrix**, not a type.
   `keep(count(0))` is a Subject: the row fires every edge rule that listens,
   never appears in its own tick log, and is gone at the boundary (receipt (c),
   both doors). `keep(count(N))` is a ReplaySubject(N). `keep(all)` is an
   unbounded replay. A keyed set rel is a BehaviorSubject. Nothing was added to
   get any of those.

3. **The named gap narrowed.** `CLAUDE.md` calls reader-driven retention "the
   ONE missing construct between log and channel-with-N-readers". Receipt (g)
   builds a bounded channel with reader-driven backpressure and *no* new
   construct: the writer is gated on the watermark, two rows are admitted, two
   are refused into a visible `dropped` rel, the reader advances, the next row
   is admitted. So the expressiveness half of that gap is closed. What is left
   is storage reclamation of already-delivered rows, which is a smaller and
   differently-shaped question (card 3).

`locked(single_rel_type_system)` is not fought anywhere in this document. There
is no second kind of thing. A stream is a rel with three declared properties.

---

## 1. The build

### 1.1 As the user would type it

```
rel event(name: text, payload: text) log keep(all).
rel cursor(name: text, at: int) key(1).
rel stream(name: text, ordinal: int, payload: text) log keep(all).

cursor(Name, 1)     <+ event(Name, _), not(cursor(Name, _At)).
cursor(Name, Next)  <+ event(Name, _), pre(cursor(Name, At)), Next := At + 1.
stream(Name, 1, Payload)    <+ event(Name, Payload), not(cursor(Name, _At)).
stream(Name, Next, Payload) <+ event(Name, Payload), pre(cursor(Name, At)),
                               Next := At + 1.
```

`v6/prolog/labs/rel_as_stream/ordinal_stream.dl6`. `bop check` exits 0.

The same program written left to right as one match block, using the arms
ratified today:

```
match event(Name, Payload) (
  ; not(cursor(Name, _At))                      |+> cursor(Name, 1)
  ; pre(cursor(Name, At)), Next := At + 1       |+> cursor(Name, Next)
  ; not(cursor(Name, _Seen))                    |+> stream(Name, 1, Payload)
  ; pre(cursor(Name, Prior)), Step := Prior + 1 |+> stream(Name, Step, Payload)
).
```

Receipt (b): the match-block form and the four-rule form produce the same tick
log on both doors. The north-star spelling costs nothing here, today.

### 1.2 The tick log, which is the whole argument in six lines

Reference engine over `ordinal_stream.dl6`, three arrival batches, the middle
one carrying two events:

```
{"tick":1,"deltas":{"cursor":{"add":[["clicks",1]],"del":[]},"event":{"add":[["clicks","a"]],"del":[]},"stream":{"add":[["clicks",1,"a"]],"del":[]}}}
{"tick":2,"deltas":{"cursor":{"add":[["clicks",3]],"del":[["clicks",1]]},"event":{"add":[["clicks","b"],["clicks","c"]],"del":[]},"stream":{"add":[["clicks",2,"b"],["clicks",3,"c"]],"del":[]}}}
{"tick":3,"deltas":{"cursor":{"add":[["clicks",4]],"del":[["clicks",3]]},"event":{"add":[["clicks","d"]],"del":[]},"stream":{"add":[["clicks",4,"d"]],"del":[]}}}
{"tick":4,"deltas":{}}
```

Tick 2. `cursor` goes `1 -> 3`; ordinal 2 was computed, consumed by the second
occurrence, and is not an event. `stream` carries 2 and 3. That is
`v6/tsv2/rxoracle/cases/scan_state_feedback`'s recorded divergence
("computed, used, unobservable") answered without touching the engine: publish
the sequence instead of only the fold.

### 1.3 The pure-rxjs lowering

Standing repo law. The program above is:

```ts
const cursor$ = event$.pipe(
  groupBy((event) => event.name),
  mergeMap((perName) => perName.pipe(scan((at) => at + 1, 0))),
);
const stream$ = event$.pipe(
  groupBy((event) => event.name),
  mergeMap((perName) =>
    perName.pipe(scan(
      (carry, event) => ({ ordinal: carry.ordinal + 1, payload: event.payload }),
      { ordinal: 0, payload: "" },
    ))),
);
```

`cursor$` is the fold's accumulator, `stream$` is the fold's emission sequence.
rx gives you both from one `scan` because `scan` emits every intermediate. The
tick model gives you `cursor` collapsed and `stream` complete, which is the same
two things with the batching made explicit rather than implicit. The rx version
has no equivalent of the batch, so it cannot show you which intermediates were
same-tick; the tick model can, and does, in the log above.

### 1.4 What it costs, stated without flattery

| cost | receipt |
|---|---|
| one keyed rel and two rules per stream, hand written | R3 |
| the ordinal is a program value, so a second rule can write it wrong (two writers, one key, one occurrence = `keyed_conflict`) | engine.pl step 5 |
| every consumer that cares about order pays an explicit comparison or `min`/`max` | R5, receipt (f) |
| the sequence is stored, so an unbounded stream is unbounded storage | card 3 |
| `pre` makes the rule occurrence-ordered, which is a real execution shape (`__pre_<rel>` snapshot, ordered occurrence loop), not a widening | SCOREBOARD.md `edge_body_needs_pre`, ARCH `pre_occurrence_loop` |

### 1.5 What it still cannot do

Exactly one thing, and it is narrower than the standing note says. A log rel's
own delta channel has no minus: retention removes rows and reports nothing
(R12, and consumption-arms assertion 17). `finalize/1` over the log therefore
fires nothing, forever, with no refusal.

But eviction *is* observable one hop downstream, and that is new here. A single
derived level rel over the log has a proper B-plane boundary, so its `del`
carries the evicted row, and `finalize` over *that* fires:

```
rel ev(ordinal: int, payload: text) log keep(count(2)).
rel live(ordinal: int, payload: text) .
rel evicted(ordinal: int, payload: text) log keep(all).

live(Ordinal, Payload)    <- ev(Ordinal, Payload).
evicted(Ordinal, Payload) <+ finalize(live(Ordinal, Payload)).
```

Receipt (d), both doors: `live` reports `del [[1,"a"]]` when `ev` prunes, and
`evicted` gets the row one tick later. So "retention is invisible" is true of
the log rel and false of the program. The workaround is one rule. Card 4 is
whether the hole gets a refusal that says so.

---

## 2. Is a stream a different thing, or a view of a table?

Both sides argued with worked programs. The verdict is at the end and it is not
a split decision.

### 2.1 The case that it is a VIEW of a table

**Argument 1: the consumer side is forced to be a table, by a shipped refusal.**
A per-reader projection over a log cannot itself be a log:

```
rel stream(ordinal: int, payload: text) log keep(all).
rel read_at(reader: text, at: int) key(1).
rel pending(reader: text, ordinal: int, payload: text) log keep(all).

pending(Reader, Ordinal, Payload) <-
    read_at(Reader, Cursor), stream(Ordinal, Payload), Ordinal > Cursor.
```

R6: `log_on_level_headed_rel(pending/3)`, which is TICK-MODEL.md theorem four.
Written as an ordinary level rel it works, and two readers hold independent
positions (R5, and consumption-arms assertion 16). So whatever a stream is,
*reading* one produces a table. If streams were a second kind of thing, the
consumer side is exactly where the second kind would have to appear, and the
language already refuses to put it there.

**Argument 2: TICK-MODEL.md has three objects and a log rel is already one of
them.** The N ring is the occurrence plane. Identical rows already stack there
without any ordinal (R10). A stream is not a fourth object; it is the N ring
with an order column bolted on. The ordinal buys ORDER and never identity.

**Argument 3: every Subject variant is a decl combination, measured.**

| rx | dl6 decl | receipt |
|---|---|---|
| `Subject` | `rel x(...) log keep(count(0))` | (c) |
| `ReplaySubject(N)` | `rel x(...) log keep(count(N))` | R12 |
| `ReplaySubject(inf)` | `rel x(...) log keep(all)` | (a) |
| `BehaviorSubject` | `rel x(...) key(...)` | R2 (`cursor`) |

`keep(count(0))` is the sharp one. The row fires the edge rules that listen, a
level rel over it derives and retracts inside the same tick and nets to zero, no
`ev` delta ever prints, and the final store is empty. That is deliver-and-forget,
spelled in two words that already exist.

**Argument 4: a table can carry something rx has no word for.** Receipt (h):
two server generations over one db file, program reloaded, and the ordinal
continues at 3 with no re-delivery of 1 and 2. rx subscriptions have no durable
position; every restart is a resubscribe. A stream whose position survives the
process is a table, definitionally.

**Argument 5: the classic stream combinators are already ordinary rules.**
Receipt (e), both doors byte-identical:

```
zipped(Ordinal, Left, Right)    <- left_at(Ordinal, Left), right_at(Ordinal, Right).
bucketed(Bucket, Ordinal, Load) <- left_at(Ordinal, Load), Bucket := Ordinal / 2.
```

`zip` is an equijoin on the ordinal. `bufferCount` is integer division on it.
`take`/`skip` are guards on it. `merge` is one shared cursor over N producers
(R4). `zip/2` is currently a **reserved, refused** row in `registry.pl`; that
refusal has outlived its reason (card 2).

### 2.2 The case that it is genuinely DIFFERENT

Taken seriously, because if there is a tier-0 it lives here.

**Claim 1: order is intrinsic to a stream and extrinsic to a table.** This is
the strongest form of the objection and it is true as stated. A table is a set
or a bag; its rows have no order, so order has to be a column, and every
consumer that cares pays a comparison. In rx, order is free: it is the sequence
of `next` calls and no operator has to mention it.

*Rebuttal.* The cost is real and it is ergonomics, not expressiveness. Receipts
(e), R4, R5 and (g) build zip, merge, N readers and backpressure out of that
column with no construct. Meanwhile making order intrinsic would cost something
the table version does not: an order that is not a value cannot be joined,
retained, compared against a cursor, or written to disk, and receipts (h) and
R5 all depend on doing exactly those things. This is a trade, and the tick model
already picked the side that composes.

**Claim 2: a stream's element is consumed, a table's row persists.**

*Rebuttal.* `keep(count(0))`, receipt (c). Answered in two words.

**Claim 3: a stream has terminal notifications; a table has no `error` or
`complete`.**

*Rebuttal.* Not re-derived here. `plans/2026-07-28-consumption-arms-verdict.md`
graded all six observer words down to shipped kernel forms
(`subscribe`/`unsubscribe` = `next`/`finalize` on the demand rel, `complete` =
`finalize` on the scope rel). The open questions there are spelling slots, not
expressiveness.

**Claim 4: a stream can be pulled; a table is pushed at you.**

*Rebuttal.* Nothing pulls in this engine and nothing should: the drain loop is
the only scheduler. Demand-driven production is the shipped host shape
(`__host_demand_*`), and receipt (g) shows the write side gated on a derived
watermark, which is what backpressure is for. Adding a pull channel would be
adding a scheduler, not a stream.

**Claim 5: a bounded stream forgets, and a table cannot report what it forgot.**

*Rebuttal, partial and honest.* True of the log rel (R12). False of the program
(receipt (d)). What remains true is that the log rel's own N-plane delta has no
minus and never will, because `retract_from_log` is a correct refusal (R7):
occurrences cannot un-happen. Card 4.

### 2.3 Verdict

**A stream is a view of a table, plus a naming convention.** Specifically it is
a rel with three declared properties:

| property | spelling today |
|---|---|
| multiplicity: occurrences, not membership | `log` |
| order: a total order over this rel's occurrences | an `ordinal: int` column, minted by a keyed cursor |
| retention: how much of the sequence is retained | `keep(all)` / `keep(count(N))` / `keep(count(0))` |

No second checker, no second type, nothing that fights
`locked(single_rel_type_system)`. Every property above is already checked by the
one checker that exists, and three of the five cross-plane theorems in
TICK-MODEL.md section 5 are exactly the checks that keep these properties
coherent (`log_on_level_headed_rel`, `keep_on_non_log_rel`, `keyed_level_head`).

---

## 3. The tier-0 test

Strict reading, as asked. (a) sugar over an existing lowering, (b) a new
lowering of existing semantics, (c) tier 0: a genuinely new semantic that no
lowering over current constructs can express. Only (c) justifies syntax.

| # | wanted construct | expressible today | class | reasoning |
|---|---|---|---|---|
| 1 | an ordinal column on a log rel | yes, 1 keyed rel + 2 rules or 4 match arms | **(a)** | receipts (a) and (b). An expansion module that stamps the four rules is the enum/match precedent exactly |
| 2 | an engine-minted ordinal (no cursor rel) | no surface | **(b)** | the value exists on both doors already: `st(Tick, Seq)` in engine.pl:357, `rowid` in lower.pl:2275. Only a surface is missing. See the crack below |
| 3 | deliver-and-forget | `log keep(count(0))` | none | receipt (c), both doors |
| 4 | bounded replay | `log keep(count(N))` | none | shipped |
| 5 | latest value | keyed set rel | none | shipped |
| 6 | merge of N producers | one shared cursor | none | R4 |
| 7 | `zip` | equijoin on the ordinal | none | receipt (e). The reserved refusal outlives its reason (card 2) |
| 8 | `bufferCount` | `Bucket := Ordinal / N` | none | receipt (e) |
| 9 | `take` / `skip` | guard on the ordinal | none | comparison ops are live in both body kinds |
| 10 | last element of a stream | `max(Ordinal)` in a level rule | none | receipt (f). `latest/1` does NOT do this; it samples the whole table |
| 11 | N readers at independent positions | keyed cursor + `min` | none | R5, consumption-arms 16 |
| 12 | backpressure / bounded channel | writer gated on the watermark | none | receipt (g). Overflow is a visible `dropped` row |
| 13 | eviction as an event | derived level rel + `finalize` | none | receipt (d), both doors |
| 14 | reader-driven storage reclamation | no | **(b)** | the DELETE machinery ships (`retentionstmt` with `RETURNING`, lower.pl:2275); the predicate is an ordinary derived rel (R8); only the POLICY SOURCE is a literal. Card 3 |
| 15 | `finalize` over a log actually firing | no, silently | **(b)** | needs the retention delete to reach the departure carry. Card 4 |
| 16 | wall-clock operators (`debounce`, `throttle`, `delay`) | partially | **(b)** | `now/1` is the tick, not the wall (R9, design review B8). Wall time enters as bind rows per `clock_residency`; the residue is resolution, not semantics |
| 17 | `error` / `complete` channels | grounded, spelling open | **(a)/(b)** | consumption-arms verdict, already ruled; not re-derived |
| 18 | flattening (`concat`/`switch`/`exhaust`/`mergeMap`) | scope-row PK shape | dependency | `scopes.pl` fixtures + `lane/teardown-flatten`. Section 6 |
| 19 | passing a stream as an argument | no | out of scope | `locked(higher_order_runtime_boundary)`; `plans/2026-07-30-rel-as-value-lab.md` section 5 settled it as compile-time specialization |

**TIER 0: empty.**

Nothing in the wanted set requires a semantic that no lowering over current
constructs can express. Two rows are (b) and both reuse a tick phase that
already exists. One row is (a) and is a pure expansion. Every other row is
already spelled.

### 3.1 The one crack found while pricing row 2

The two doors already carry two different internal ordinals, and neither is
graded, because neither is observable.

- `engine.pl:356-358`, `next_seq/3`: `findall(Number, member(lrow(st(Tick, Number), _), Store), Numbers)` scans **every** log rel's rows for that tick, so `Seq` is a single monotone counter **across all log rels** within a tick.
- `lower.pl:2275`, the retention DELETE: `ORDER BY rowid DESC LIMIT N`, and `rowid` is **per table**.

Those two definitions disagree about the relative order of occurrences in
different log rels within one tick. Nothing catches it because nothing can see
either number. Surfacing an engine-minted ordinal (row 2) therefore has a
prerequisite: pick one definition, and the pick is observable, so it becomes a
graded contract. That is a real cost on option 1c and it is priced there rather
than discovered later.

---

## 4. The lowering target, stated without rxjs

The brief asks for a description several runtimes could hit. The five statements
below mention no operator, no library and no scheduler. A runtime satisfies them
or it does not.

Let a **stream** be a rel `R` declared with multiplicity `occurrences`, an order
column `ord`, and a retention bound `B`.

1. **ORDER.** Every occurrence admitted into `R` receives an `ord` strictly
   greater than every `ord` previously admitted into `R`. Within one tick the
   assignment follows arrival order. The assignment is a pure function of
   (highest `ord` before the tick, index within the tick's ordered arrival
   list). Receipt R11 pins the arrival-order half; R1 pins the strictness.

2. **VISIBILITY.** Every admitted occurrence appears exactly once in `R`'s
   positive delta sequence, in `ord` order. Occurrences are never coalesced,
   even when identical (R10). This is the property the keyed plane does not have
   and the one the user is asking for (R2).

3. **RETENTION.** `B` may remove rows from `R`. The removal is not reported on
   `R` itself. It IS reported on any level rel derived from `R`, as an ordinary
   negative boundary delta (receipt (d)). A runtime that reports removal on `R`
   is a different language, not a better one, because `R` is the occurrence
   plane and an occurrence that un-happens is refused by construction (R7).

4. **DURABILITY.** `ord` and every cursor over it are storage, not session
   state. A restart continues the sequence; it does not replay it and does not
   reset it (receipt (h)).

5. **NO PULL.** Nothing reads on demand. The only scheduler is the drain loop.
   Flow control is a guard on the WRITE, evaluated against values the program
   itself derives, never a demand signal travelling backwards from a reader
   (receipt (g)).

Three runtimes against those five:

| runtime | ORDER | VISIBILITY | RETENTION | DURABILITY | NO PULL |
|---|---|---|---|---|---|
| SQL, the shipped emitter | a rowid table plus a cursor table | rows physically coexist (lower.pl:703 keeps log rels rowid tables precisely so duplicates count) | `DELETE ... RETURNING` at tick end | the db file | the tick loop |
| rxjs | `scan` carrying an index, or a `Subject` with a counter | `scan` emits every intermediate | `shareReplay({bufferSize})` or a bounded buffer | **cannot**: no durable position | violated by `Observable`'s own pull-on-subscribe |
| rust, no library | `Vec<Row>` plus a `usize` head, or a monotone counter | push into the Vec | truncate from the front | serialize the counter | a loop |

rx is the runtime that fails two of the five, and it fails them structurally.
That is the answer to "do not overfit to rxjs": the model above is not what rxjs
does, and where they disagree the tick model wins. The four disagreements, each
with the side taken:

| disagreement | rx | tick model | taken |
|---|---|---|---|
| what `latest` samples | `withLatestFrom`: the newest VALUE | `latest/1`: the current TABLE (receipt (f) returns 2 rows) | tick model. Sampling a rel gives the rel. The rx word is the one that is wrong here, which is design review B8's vocabulary finding with a receipt attached |
| teardown | a channel (`complete`) | a per-row minus (`finalize`) | tick model, TICK-MODEL.md section 2: the arms are the sign decomposition of one derivative |
| durable position | none | storage (receipt (h)) | tick model |
| cross-rel order | one operator chain, one order | per-rel order is the contract; cross-rel interleaving across a drain boundary is not | tick model, and it is MEASURED, not assumed: `receipts.sh`'s header records the case (d) diff that forced the grading to be per-rel |

The last row is worth naming as a limit rather than a win. The only rx choice
already welded into this engine is the host `concatMap` (hosts hardcoded, no
teardown path). Nothing in the five statements above needs it, mentions it, or
inherits it. That was the point of writing them without operators.

---

## 5. Cards

Each card carries at least two real spellings, user-typed form first.

### Card 1: how the ordinal is spelled

Row 1 and row 2 of the tier-0 table. Today it is four rules of boilerplate per
stream, and the boilerplate is identical every time.

**1a. Nothing. It stays four rules (or four match arms).**

```
rel stream(name: text, ordinal: int, payload: text) log keep(all).
cursor(Name, 1)     <+ event(Name, _), not(cursor(Name, _At)).
cursor(Name, Next)  <+ event(Name, _), pre(cursor(Name, At)), Next := At + 1.
stream(Name, 1, Payload)    <+ event(Name, Payload), not(cursor(Name, _At)).
stream(Name, Next, Payload) <+ event(Name, Payload), pre(cursor(Name, At)),
                               Next := At + 1.
```

Buys: zero language change, already graded on both doors. Costs: four rules and
one extra rel per stream, and every author writes the same `pre` chain by hand
with a real chance of getting the base case wrong.

**1b. Sugar: the ordinal is a column TYPE the expansion fills.**

```
rel stream(name: text, ordinal: seq(name), payload: text) log keep(all).
stream(Name, _, Payload) <+ event(Name, Payload).
```

`seq(name)` says "this column is a total order per `name`". One expansion module
stamps the cursor rel and the four rules, before analyze, exactly as
`0_enum_expand.pl` and `0_match_expand.pl` already do; both doors consult the
one expansion, which is the shipped precedent. Class (a), sugar. Buys: the
common case is one line and the base case cannot be written wrong. Costs: one
new column type, and the minted cursor rel is now compiler-owned, so it appears
in the tick log unless it is made boundary-invisible (the frontier-TEMP class
from the `struct_as_rows` ruling).

**1c. The engine mints it and a body binds it.**

```
rel stream(name: text, payload: text) log keep(all).
consumed(Ordinal, Payload) <- stream(_Name, Payload) @ Ordinal.
```

Class (b). The value exists on both doors already. Buys: zero storage, zero
extra rel, and the order is unforgeable by a program. Costs: **the two doors
currently define it differently** (section 3.1), so this option first has to
pick one, and the pick becomes a graded cross-target contract; it also adds a
binding position that no other construct uses.

### Card 2: `zip` is a reserved refusal that no longer needs to be one

`registry.pl` carries `zip/2` as `reserved`, `wrapper(atom_list, refuse(functor))`.
Receipt (e) shows the thing it refuses is an equijoin.

**2a. Delete the reserved row. `zip` is spelled as a join.**

```
zipped(Ordinal, Left, Right) <- left_at(Ordinal, Left), right_at(Ordinal, Right).
```

Buys: one fewer reserved word, and the language stops implying it owes a
construct it does not owe. Costs: an author who types `zip` gets
refusal-by-absence rather than a message.

**2b. Keep the row, make the refusal message name the join.** Costs a message,
buys the teaching moment, and B4 (refusals print as a bare swipl `Unknown
message`) is a standing complaint this would sit inside.

**2c. Make `zip` sugar that expands to the equijoin.** Buys the rx word for the
rx meaning. Costs a construct budget entry for something one line already says,
and the vocabulary law would then have to answer why `zip` is sugar and `merge`
is not.

### Card 3: reader-driven storage reclamation, the narrowed gap

`CLAUDE.md` calls this "the ONE missing construct". Receipt (g) shows the
expressiveness half is not missing. What remains is reclaiming storage for rows
every reader has already passed.

**3a. Nothing. Bound the channel at the write side and size `keep(count(N))` to
the bound.**

```
rel chan(ordinal: int, payload: text) log keep(count(8)).
rel dropped(payload: text) log keep(all).
watermark(min(At)) <- read_at(_Reader, At).

chan(Next, Payload)  <+ produce(Payload), pre(cursor('chan', At)),
                        latest(watermark(Mark)), Next := At + 1, Next - Mark =< 4.
dropped(Payload)     <+ produce(Payload), pre(cursor('chan', At)),
                        latest(watermark(Mark)), Next := At + 1, Next - Mark > 4.
```

Receipt (g), both doors. Buys: zero language change; overflow becomes a VISIBLE
row where retention's own prune is silent, which is strictly better than what
`keep(count(N))` alone does; the static bound is now provably safe because the
writer refuses to exceed it. Costs: the drop policy is the program's problem;
a slow reader loses rows rather than stalling the producer, and choosing to
stall instead requires the producer to have somewhere to park the row.

**3b. `s1` from the consumption-arms verdict: retention as an ordinary
retracting rule over the log.** Its full pricing is in that verdict and is not
relitigated here. Buys: the prune becomes a visible minus delta, and any join
expresses the bound. Costs: lifting `retract_from_log`, a new head kind for edge
rules, and a stratification obligation. Receipt (d) reduces the "visible minus"
half of the buy, since one derived rel already produces it.

**3c. A decl that names the bound rel** (`keep(until(watermark))`, verdict `s2`).
Buys: no change to the append-only law. Costs: one decl word, decls now depend
on rules, and the prune stays invisible on the log.

### Card 4: `finalize` over a log rel is silently dead

R12: retention removed two rows and the `finalize` arm fired zero times, with no
refusal. `SLOT-LOG-FINALIZE-REFUSAL` (update-arm verdict U5) recommended a
load-time refusal; this lab adds that the workaround is one rule.

**4a. Refuse at load, and name the workaround in the message.**

```
evicted(Ordinal, Payload) <+ finalize(ev(Ordinal, Payload)).
% -> finalize_on_log_rel(ev/2): a log rel has no negative delta;
%    project it through a level rel and finalize that.
```

Buys: a silent nothing becomes a sentence; decidable at load, same shape as the
three refusals the keyed-divergence lane landed. Costs: one refusal in two
implementations plus a fail-first fixture.

**4b. Leave it.** Costs: an author who watches a bounded log for eviction gets
silence forever.

**4c. Make it fire from retention's own delete.** The emitter already stages
`sign: -1` events from the retention `RETURNING` (`1_incremental.ts`
`applyRetentionStatement`). Buys: the natural spelling works. Costs: it
contradicts R7 at the surface (occurrences would visibly un-happen), and the
oracle has no such staging, so it is a two-door semantics change, not a fix.

### Card 5: `latest/1` over a log rel

Receipt (f): `latest(stream(Ordinal, Payload))` returned both rows. An author
reading the rx word expects the newest one.

**5a. Leave it. `latest` samples a rel and a log rel is a rel.**

```
head_at(max(Ordinal)) <- stream(Ordinal, _Payload).
```

Buys: consistency with `latest` everywhere else. Costs: the rx word means
something else in rx, which is design review B8's complaint.

**5b. Refuse `latest` over a log rel, message names `max(Ordinal)`.** Buys: the
surprise becomes a sentence. Costs: removes a use that is coherent (sampling a
whole log is a legitimate read) and needs a fixture to pin which uses die.

**5c. Define `latest` over a log rel as the max-ordinal row.** Buys: the rx
meaning. Costs: `latest` now means two different things depending on the callee's
decl, which is the kind of context-sensitivity the five theorems exist to
prevent.

### Card 6: does "stream" become a word at all

**6a. No word. A naming convention plus a SYNTAX.md section.** The three
properties (`log`, an ordinal column, a `keep` bound) are the definition and the
checker already checks all three. Buys: zero budget. Costs: nothing tells an
author they built two thirds of a stream.

**6b. A decl word.** `rel clicks(payload: text) stream.` expands to
`log keep(all)` plus a `seq` ordinal (card 1b). Buys: one word for the whole
shape. Costs: a fourth decl modifier, and it hides the retention choice, which
is the one an author most needs to make on purpose.

**6c. An analyze role only:** no surface, but `bop check` reports "this rel is
a log with no order column; a consumer cannot read it in order". Buys: the
teaching without the budget. Costs: a warnings mode, which the refusal
discipline currently does not have and has deliberately not had.

### Card 7: cross-rel order across a drain boundary is not a contract

Found while building the grading, section 4's last row.

**7a. Write it down as a non-contract.** One paragraph in TICK-MODEL.md next to
the grade table: per-rel delta order is graded; cross-rel interleaving is a
function of drain placement and differs between a schedule-fed door and a
served one. Buys: honesty, and a rule for every future harness. Costs: nothing.

**7b. Make it a contract.** Would require the served engine to place drains
where the schedule-fed oracle does. Buys: a stronger grading. Costs: the
runtime-bridge arc already measured this as "not fixable in general".

---

## 6. Dependency on `lane/teardown-flatten`

Stated precisely, in both directions.

**Their answer does not gate mine.** Everything in sections 1 through 4 is
merge-of-sources, ordering, retention and reading, none of which creates or
destroys a subscription. Receipt R4 is rx `merge` over source rels, not
`mergeMap` over inner subscriptions; the flattening family is entirely theirs.

**Mine may help theirs, in one specific place.** `concat` needs a queue, and the
consumption-arms verdict found that "(b) one-per-drain-tick is the only spelling
that implements a queue", with the drain cap becoming a queue-length cap. A
`concat` queue needs a total order over pending inners to decide which is next.
Receipts R4 and (g) supply exactly that: a shared cursor gives the total order,
and a writer gated on the watermark gives the bound without a new construct. If
their lane concludes that flattening needs an ordering primitive, this lane's
answer is that it does not need a new one.

**One shared risk, named.** Receipt (d) depends on `finalize` over a *derived
level* rel firing on the negative boundary delta. If their lane changes how
`delta.del` reaches the effect plane, receipt (d) is the thing to re-run first.

---

## 7. Receipts index

Everything asserted above and how it was produced. All runs hermetic
(`SPREFA_CONFIG=/nonexistent/rel-as-stream.toml`, `DL_NO_DAEMON=1`, ephemeral
ports, `:memory:` except the durability case's `mktemp` file). Nothing read or
wrote `~/.local/state` and no daemon was contacted.

| # | claim | how |
|---|---|---|
| R1 | the ordinal is a total order across and within ticks | `0_receipts.pl`, reference engine |
| R2 | the collapsed intermediate is a row on the log and no row on the keyed rel | same, one program, two rels, tick 2 |
| R3 | the state model moved, it did not leave | same, structural assertion on the program term |
| R4 | merge = one cursor over N producers | same |
| R5 | N readers hold independent positions | same; consumption-arms 16 re-run over an explicit ordinal |
| R6 | a reader's view of a log must be a level rel | same, `log_on_level_headed_rel(pending/3)` |
| R7 | occurrences cannot un-happen | same, `retract_from_log(ev/1)` |
| R8 | a reader-driven watermark computes and is inert | same, `retired(1)` derived, `stream(1,a)` still stored |
| R9 | `now()` is the tick, so it ties within one | same, `stamped(a,1)`, `stamped(b,1)` |
| R10 | identical occurrences already stack | same |
| R11 | the ordinal records arrival order inside one tick | same, both orders run |
| R12 | retention prunes and `finalize` over the log fires nothing | same |
| (a) | the build, both doors | `receipts.sh`, `ordinal_stream.dl6`, 13 deltas |
| (b) | match-block form is the same program | `receipts.sh`, `match_stream.dl6`, 13 deltas |
| (c) | `keep(count(0))` is deliver-and-forget | `receipts.sh`, `transient.dl6`; served `/idb/ev` empty, `/idb/recorded` full |
| (d) | eviction becomes an event one hop downstream | `receipts.sh`, `retention_event.dl6`, 12 deltas |
| (e) | zip and bufferCount are joins and arithmetic | `receipts.sh`, `zip_buffer.dl6`, 10 deltas |
| (f) | `latest()` over a log is not the last element | `receipts.sh`, `latest_log.dl6`, 6 deltas |
| (g) | writer gated on the watermark, overflow visible | `receipts.sh`, `backpressure.dl6`, 21 deltas |
| (h) | the ordinal survives a restart | `receipts.sh`, two server generations over one db file |
| (i) | the grading discriminates | `receipts.sh`, `At + 1` -> `At + 2`, diff goes red |
| the two internal ordinals | source reading: `engine.pl:356-358` global per tick, `lower.pl:2275` per table | not observable, therefore not graded |
| `bop check` accepts the build | `npm run bop -- check ordinal_stream.dl6`, exit 0 | |

### Prior work cited rather than re-derived

| source | what was taken |
|---|---|
| `plans/2026-07-28-consumption-arms-verdict.md` | assertions 16 (channel composes today), 17 (prune is invisible), 18 (static keep stalls a lagging reader), 19 and the `s1`-`s4` retention pricing; the six observer words grounding to kernel forms |
| `v6/prolog/compile/TICK-MODEL.md` | the three rings, lifecycle as sign decomposition, theorem four (`log_on_level_headed_rel`) |
| `plans/2026-07-30-rel-as-value-lab.md` | nothing is parametric; rule-level generics are a separate already-ruled axis; `locked(higher_order_runtime_boundary)` |
| `v6/tsv2/rxoracle/` | the `scan_state_feedback` case's recorded divergence, and N1's reason for not comparing tick numbers |
| `v6/prolog/labs/select_scan_cache/`, `scan_surface_composition/`, `generic_scan_instantiation/` | all three prototype scan shapes; none folded, and this lab proposes folding none of them |
| ARCH `pre_occurrence_loop` | `pre` is a chained mid-tick read through ordered occurrences, not a sampled read |
