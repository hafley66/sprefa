# Point-free lab — verdict

Contract: `plans/2026-07-30-point-free-lab-header.md`. Lab files:
`v6/prolog/labs/point_free/` (die on landing). Base sha `2da8faef`.

Receipts: `bash v6/prolog/labs/point_free/receipts.sh` → **29 PASS 0 FAIL**.
Censuses: `python3 v6/prolog/labs/point_free/census.py` (this lab's corpus) and
`python3 v6/prolog/labs/point_free/csp_census.py` (the csp-idioms corpus, which
this lab did not write). Conformance unchanged and untouched: **256 PASS /
0 FAIL, exit 0** (`v6/prolog/labs/point_free/probe/conformance_count.sh`); this
lab added zero fixtures and edited nothing outside its own directory.

## Headline

**The mapping table is right, the moves are worth less than the header assumed,
and the corpus does not read like point-free rx because of one thing none of
the three moves touches.**

Fourteen rx-documentation patterns were written in dl6 and graded. **Nine of
fourteen get NOTHING from any of M1, M2 or M3** — they are already one or two
rules, and the operator rx spells is a declaration here rather than a body
word. Across the whole corpus the three moves together delete **10 of 30 rules
(33%)** and **5 of 41 declarations**.

Per move, with the receipt each rests on:

| move | verdict | what it actually buys |
|---|---|---|
| **M1 `scan`** | **AMENDED, weakest of the three** | 1 rule per fold. In the nine cold-authored csp programs the fold count is ZERO outside cursors (18 of 18 `pre` reads are cursors). On a rel folded by two triggers it makes the program LONGER, 2 rules → 4 (receipt 4d). Its real value is not brevity: it is a decidable place to put the keyed-head check, and the unrefused version is silently, spectacularly wrong (receipt 4a). |
| **M2 `seq`** | **CONFIRMED, and it is the one that pays** | 4 rules → 1 per cursor, re-measured independently at **27 of 94 rules (29%) across the csp corpus**, 9 cursor blocks in 7 of 10 idioms. |
| **M3 `\|>`** | **CONFIRMED for level rules, REFUSED for edge rules** | 3 rules → 1 and 2 declarations → 0 on a map/map/filter chain, same ticks, no latency. In an EDGE rule the same expansion is wrong two different ways, one loud and one silent (receipts 4b, 4c). |

And the finding the corpus produced that no move addresses: **the `pairwise`
idiom is wrong at one change per tick** (receipt Q1e, below). That is a
semantics hole, not a syntax one.

---

## Q1 — the mapping table, per row, with receipts

Every row is graded by a program in `v6/prolog/labs/point_free/today/` against
its own schedule, through the oracle door, and every one of them also returns
`check exit 0` from the text door (`checkall.sh`, 14 today programs plus the 6
expanded ones, all clean).

| rx | dl6 | receipt file | graded | verdict |
|---|---|---|---|---|
| `merge` | two rules heading one rel | `today/merge.dl6` | Q1a: `click + [2,"right"]` and `click + [3,"left"]` both land in the tick they arrive | **CONFIRMED** |
| `map`, `filter` | ordinary body items (`:=`, comparisons) | `today/map_filter.dl6` | 2 ticks, `tripled` carries only the rows passing `Raw > 0` | **CONFIRMED** — and already point-free in the sense that matters: neither names an intermediate rel |
| `scan` | M1, or two arms today | `today/counter.dl6` + `sugar/counter.sugar.pl` | leg 1: 14 deltas, sugar == today byte for byte | **CONFIRMED** the shape; M1 AMENDED, see above |
| `withLatestFrom` | `latest/1` in an edge body | `today/with_latest_from.dl6` | Q1c: exactly ONE `submission` delta, carrying the draft sampled at the submit tick; the later draft edit fires nothing | **CONFIRMED** |
| `switchMap` | `key(1)` | `today/drag_switch.dl6` | 4 ticks, last write per pointer wins | **CONFIRMED with a condition**: two rules may share a keyed head only while they are triggered by DIFFERENT refs. One shared ref is `edge_head_conflict_risk`, measured on `probe/scan_latest.dl6`. |
| `distinctUntilChanged` | boundary diffing at the rel edge | `today/distinct_until_changed.dl6` | Q1b: the repeated identical write is a ZERO delta — 3 `stable` deltas over 4 ticks, not 4 | **CONFIRMED** |
| `toArray` | `json_group_array/1` | `today/gather.dl6` | 3 ticks, group key is the head's other column | **CONFIRMED** |
| `pairwise` | `finalize` + read | `today/pairwise.dl6` | Q1d correct at one change per two ticks: `(10,14)` then `(14,9)`. Q1e **WRONG** at one change per tick: `(10,9)` then `(14,9)` — the middle value is skipped on one side and repeated on the other | **AMENDED — this row is only pairwise at a cadence slower than one change per tick** |

### The Q1e defect, written out

`step(Sensor, Previous, Current) <+ finalize(reading(Sensor, Previous)),
reading(Sensor, Current).` The departure is a NEXT-tick occurrence (the
update-arm verdict's "replace-tick plus one"), so the `reading` atom beside it
reads the rel as it stands a tick later. With values 10, 14, 9 on consecutive
ticks the pairs come out `(10, 9)` and `(14, 9)`; insert one idle tick between
each change and they come out `(10, 14)` and `(14, 9)`, correct.

This is not the update-arm verdict's U4 (that one is the SAME-tick collapse,
`v1 -> v2 -> v3` in one tick giving the honest endpoint pair). This is the
across-consecutive-ticks case, and it compiles clean with no diagnostic
anywhere. Pinned as receipt Q1e so that fixing it turns the receipt red on
purpose.

Its rx lowering, for the record, is the one that does NOT have this problem:

```
reading$.pipe(groupBy(r => r.sensor),
              mergeMap(group => group.pipe(pairwise())))
```

`pairwise()` holds the previous value in the operator; the dl6 idiom holds it
in a delta that has to survive a tick to be read. That difference is the
defect.

---

## Q2 — the corpus and the rules-deleted census

Fourteen programs, each written first in the today spelling by hand, then (where
a move applies) as a sugar term that `expand.pl` expands and the SHIPPED printer
renders back to `.dl6`. The expanded file's tick log is diffed against the
hand-written one; byte identity is the grade.

```
program                 moves   rules_today  rules_moved  deleted%  decls_today  decls_moved
--------------------------------------------------------------------------------------------
buffer_count            M2           5            2          60          4            3
counter                 M1           2            1          50          2            2
distinct_until_changed  -            1            1          0           2            2
drag_switch             -            2            2          0           3            3
gather                  -            1            1          0           2            2
joining_stage           M3           3            1          67          5            3
map_filter              -            1            1          0           2            2
merge                   -            2            2          0           3            3
pairwise                -            1            1          0           2            2
rate_limiter            -            2            2          0           3            3
retry_backoff           M1           3            2          33          3            3
running_average         M1           3            2          33          3            3
sensor_pipeline         M3           3            1          67          4            2
with_latest_from        -            1            1          0           3            3
--------------------------------------------------------------------------------------------
TOTAL                               30           20          33         41           36
```

**Nine of fourteen programs are unchanged.** Those nine are not badly written
and are not waiting for a move: `merge`, `switchMap`, `distinctUntilChanged`,
`toArray`, `map`, `filter` and `withLatestFrom` all land on a declaration or an
ordinary body item, and the programs are one or two rules long already.

### The independent corpus: re-measuring M2

The csp-idioms verdict states "73 of 94 rules are verbatim-shape repeats, and
the single template `cursor`/`pending`/`item`/`ready` accounts for 52". That
lab is nine cold-authored CSP programs this lab did not write, which makes it
the only independent evidence for M2, so the number M2 is priced on is
re-derived from the files:

```
idiom         rules  cursor-template  cursor rels  rules after M2
------------------------------------------------------------------
buffered          9                4            1               6
workerpool        9                4            1               6
pipeline          3                0            0               3
fanin             6                4            1               3
fanout           11                4            1               8
select           16                8            2              10
timeout           6                0            0               6
done             10                4            1               7
rendezvous       19                8            2              13
semaphore         5                0            0               5
------------------------------------------------------------------
TOTAL            94               36            9              67   (29% deleted)
```

The per-idiom rule counts reproduce the csp verdict's own table exactly (94
total, and every row matches), which is the cross-check that the counting
method agrees with theirs.

**AMENDMENT to the csp verdict's recommendation.** It asks for `seq` to be
"widened: numbering alone covers 32 rules, the full queue template covers 52".
The numbering block measures at 36 rules collapsing to 9, so M2 as specified
deletes 27. The other 16 rules of the 52 are `item`, `ready_min` and `taken` —
queue semantics, not numbering, and a different construct. Widening `seq` to
cover them would be widening it to mean "queue", which is a much larger claim
than "this column is a total order".

---

## Q3 — where `|>` breaks

Four break rules, each a named refusal in `expand.pl` and each earned by
writing the unrefused expansion, running it, and finding a different answer.
`expand_point_free/3` with `unsafe(true)` skips the refusals so the receipts can
show the divergence.

### M3-1 `pipe_in_edge_rule` — a cut in an edge rule. THE decisive break.

Every cut makes the next stage's source a derived rel, so the natural
expansion turns an occurrence-driven rule into a state-derived one. It is wrong
two ways depending on the head's declaration, and only one of them is loud.

**Loud (receipt 4b).** With a `log` head the expansion does not even load:
`unsupported_construct(log_on_level_headed_rel(logged/2))`, on both doors.

**Silent (receipt 4c).** With a `key(1)` head it loads clean on both doors and
gives a different answer. Schedule: `+ping(cli,4)`, `-ping(cli,4)`,
`+ping(cli,4)`.

```
edge  (break/pipe_edge_silent_today.dl6)   tick 1  seen + ["cli",9]
                                           tick 2  (nothing)
                                           tick 3  (nothing)

level (break/pipe_edge_silent_unsafe.dl6)  tick 1  seen + ["cli",9]
                                           tick 2  seen - ["cli",9]     <-- retracts
                                           tick 3  seen + ["cli",9]
```

The rx reading of the difference: an edge rule is `source$.pipe(...)` and a
level rule is a `combineLatest` over current state. `|>` between them silently
swaps one for the other.

### M3-2 `pipe_head_is_aggregate` — an aggregate head

An aggregate's group key is exactly the head's non-aggregate columns. Under
`|>` the columns that reach the head are computed by liveness, so adding a later
stage that stops reading a column silently REGROUPS the aggregate. Refused
rather than defined.

### M3-3 `pipe_stage_name_collision` — two rules heading one rel

Both would mint `stage_<head>_<arity>_1`. Refused rather than disambiguated
with a suffix, because a suffix makes the minted rel's name, and therefore the
tick log, depend on clause order in the file.

### Stages that JOIN: **NOT a break** (measured)

The header asks about this one and the answer is no. `sugar/joining_stage.sugar.pl`
cuts a chain whose middle stage introduces a second source:

```
enriched(Sensor, Label, Value) <-
    reading(Sensor, Raw), Doubled := Raw * 2
 |> label(Sensor, Label)
 |> Value := Doubled + 1.
```

expands to three rules whose `reading`, `label` and `enriched` deltas are
identical to the hand-written three-rel version over four ticks, including the
retraction at tick 4 when the label departs. The minted rel simply carries one
more column. Its rx lowering is `withLatestFrom(label$)` plus a key filter,
which is the honest correspondence: rx cannot write a join inside `pipe()`
either.

### Diamonds: inexpressible rather than refused

`|>` is linear by construction, so there is no syntax for a second consumer of
a stage. The rule is therefore not a refusal but a statement of scope: **a
stage read by more than one rule must be NAMED**, and naming it is exactly
writing the rel the way it is written today. Nothing is lost; the sugar just
does not reach that case.

### A stage that needs its own retention or key: must be named

A minted level stage gets no `kind`, no `keep`, no `key` — it is a bare rel,
the ruled `rel_default_policy = value_unkeyed`. Inheriting the head's
declarations is precisely what the rel-spreading verdict already refused
("planes/keys NEVER travel"), and the same argument holds here for the same
reason: an inherited `keep` is invisible to tick-log grading. So a seam that
must be a `log` is a named rel, not a stage.

---

## Q4 — head-last spelling, priced, not wired

Both candidate glyphs are **free today** (receipts 5b, 5c): `|>` in a body and
`|->` outside a match block both fail with `dl_parse_error(statement, [ ... ])`,
the char-code dump the csp lab pinned as finding E2a. Nothing in the grammar
claims either.

**The parser cost of head-last is close to zero, and this is the surprising
part.** `parse_dl.pl:788` `rule_stmt/5` parses head-then-arrow-then-body. But
`parse_dl.pl:827` `match_arm/5` ALREADY parses body-then-arrow-then-head, with
`|->` and `|+>`, and already produces the ordinary `(Head <- Guards)` /
`(Head <+ Guards)` term. A head-last top-level statement is one more clause in
the `statement/6` dispatch chain (`parse_dl.pl:335`) reusing that predicate.
No new arrow family, no new term shape, no new variable threading — the arm
parser proves the body-first direction already works.

**The printer cost is where it actually lands, and it decides the answer.** The
term is `(Head <- Body)` either way; nothing in it records which spelling the
author used. So `print_dl.pl` must pick one, `dl_view/` regenerates in that one,
and G1's `=@=` round-trip still passes because the term is unchanged. That makes
head-last **input-only sugar**, exactly like the three aliases the construct
table already lists as "input only" (`<=`, `!=`, `=`).

**Priced both ways:**

| spelling | parser | printer | round-trip | buys |
|---|---|---|---|---|
| head-first (today) | shipped | shipped | shipped | — |
| head-last, input-only | 1 dispatch clause reusing `match_arm/5` | unchanged (always prints head-first) | unchanged | reading order matches rx and matches `match` arms, which already read source-first |
| head-last, round-tripping | same | a spelling flag the term does not carry | needs a new decl entry or a term wrapper | nothing further; the flag would exist only to reproduce the author's typing |

Beyond aesthetics it buys **one real thing**: the language currently reads
source-first inside a `match` block and head-first everywhere else, and an
author who learns `match` learns the opposite order for ordinary rules. Making
the two agree is a consistency argument, not a power argument. Not wired, per
the fences.

---

## Q5 — minimality

**Is any move derivable from the other two? One is, partially.**

**M2 = M1 + one minted keyed rel + two read arms.** This is not an argument, it
is the executable structure of `expand.pl`: `expand_seq/2` emits the cursor rule
with a `scan` in its head and `expand_scan/2` finishes the job, which is why the
declared expansion order is pipe → seq → scan and why reversing it produces a
program with an unexpanded `scan` in it. Reading the emitted file shows the
composition:

```
seq_numbered_1('q', At) <+ arrival(Payload), not(seq_numbered_1('q', _)), At := 0 + 1.
seq_numbered_1('q', At) <+ arrival(Payload), pre(seq_numbered_1('q', Carried)), At := Carried + 1.
numbered(Ordinal, Payload) <+ arrival(Payload), not(seq_numbered_1('q', _)), Ordinal := 1.
numbered(Ordinal, Payload) <+ arrival(Payload), pre(seq_numbered_1('q', At)), Ordinal := At + 1.
```

The first two rules are M1's output. The last two are the read, which M1 cannot
produce because it does not mint rels.

M1 is not derivable from M2 or M3. M3 is not derivable from either.

**So the minimal set is {M1, M3} plus a rel-minting facility, with M2 as a
convenience over M1** — and the census says the convenience is worth more than
the primitive: M2 deletes 27 rules of 94 in the independent corpus, M1 deletes
one rule per fold and there are no non-cursor folds in that corpus at all.

**Is there a fourth move the corpus demands? No.** The nine unchanged programs
are one or two rules each, and every rx operator they spell lands on a
declaration (`key(1)`, `log keep(...)`) or an ordinary body item. The one thing
the corpus surfaced that the moves do not fix is Q1e, and no syntax move fixes
it: `pairwise` is wrong because a departure is a next-tick occurrence, which is
a semantics question about when the minus delta becomes readable.

**One caution about the whole exercise.** With `pre_occurrence_loop` landed
(ARCH `pre_occurrence_loop, done`, 2026-07-30) the today fold is TWO rules, not
four. M1 was priced against a four-rule fold and is being delivered against a
two-rule one. That is the single biggest reason its census row is weak.

---

## Named slots

### slot_scan_spelling — **head position**, measured

`scan(Acc, Seed, Expr)` in the head, not `Acc := scan(Seed, Expr)` in the body.
The deciding receipt is the running-average program, which folds TWO columns:

```
running(Sensor, scan(CarriedTotal, 0, CarriedTotal + Value),
                scan(CarriedSeen,  0, CarriedSeen + 1))
  <+ sample(Sensor, Value).
```

Two accumulators in one head are ONE fold over a pair — rx spells the same
thing with an array accumulator, `scan(([sum, count], v) => [sum + v, count + 1],
[0, 0])` — and the expansion needs exactly one `pre` atom to bind both. In bind
position the two binds are two separate statements with nothing saying they
belong to one fold, so the expander would have to infer it from the head they
target, and an author who writes one bind and not the other gets a silent split.
Bind position also collides with `:=`'s meaning everywhere else: its left side
would have to be its own previous value.

A second measurement came out of this slot. The corpus spells the base arm two
ways — `not(head)` in `today/counter.dl6` and `not(pre(head))` in
`merge_family.pl:111` — and receipt 5d shows they produce the **same oracle
log** while only `not(head)` compiles (`not(pre(head))` is
`edge_body_with_negation`). So `scan` has exactly one legal expansion, and
`merge_family.pl:111` is an oracle-only fixture.

### slot_seq_scope — **the argument answers it; no switch to add**

`seq('q')` with an atom is one global order named `q`. `seq(Partition)` with a
variable is one order per value of that variable. There is no per-rel versus
per-name decision to make because the argument already states which was meant,
and the cursor is minted per (head rel, ordinal column) so two rules writing the
same ordinal column share one cursor.

The one cost this exposes: the partition column needs a type, and the expander
infers it from the literal or from the declared column the variable is read
from. Anything else is `seq_partition_type_unknown` rather than a default,
because a wrong column type is the `edge_head_column_type_mismatch` class the
corpus has been bitten by twice.

### slot_stage_naming — **deterministic, and it collides with a measured constraint**

The minted name is `stage_<head name>_<head arity>_<cut index>` — a function of
the rule's own head and the cut's position, with no counter, no gensym and no
hash of formatting, so it is stable across recompiles. Collisions refuse
(`pipe_stage_name_collision`).

**But there is a constraint the slot has to answer around.** Receipt 5a: a rel
name beginning `__` — the engine's own convention for `__host_*`, `__pre_*`,
`__departure_frontier_*` — **does not parse**. A leading underscore is the
variable marker, so `bop check` returns `broken: parse_failed`. That convention
is reachable only by term-level expansion, never by hand. Two consequences, and
they pull opposite ways:

- If minted rels keep the `__` convention they can never collide with an author
  name, but they can never be hand-written either, so the grading law's
  "byte-identical to its hand-desugared form" has no hand-desugared form to
  compare against for M2 and M3.
- If minted rels take legal names (what this lab did) they are hand-writable and
  gradable, and they can collide with an author's rel.

Receipt 2c makes the stake concrete: **M2's minted cursor is in the tick log**,
and renaming it alone breaks byte identity with the today spelling. That is the
cost card 1b predicted ("the minted cursor rel is now compiler-owned, so it
appears in the tick log unless it is made boundary-invisible"), now measured.
The clean resolution is the frontier-TEMP class from the `struct_as_rows`
ruling — minted rels boundary-invisible and `__`-named — but that changes what
the tick log contains, so it is a user-level call, not a lab one. **Named, not
guessed.**

A smaller version of the same hazard bit this lab twice while building it, and
both are worth knowing before anyone wires this: minted VARIABLE names collide
too. The running-average expansion first printed `running(Sensor, Folded,
Folded)` (two accumulators, one name, two contradictory binds), and the seq
cursor first printed `seq_...('q', At) <+ ..., pre(seq_...('q', At)), At := At + 1`,
a self-referential rule. A real compiler minting fresh variables never has this
problem; a PRINTED expansion does. Both are written up at their site in
`expand.pl`.

### slot_stage_retention — **forced by the arrow, not chosen**

A minted stage in a LEVEL chain gets no `kind`, no `keep`, no `key`: a bare rel,
the ruled `rel_default_policy = value_unkeyed`. That is correct rather than
convenient — a level rel is recomputed and diffed, so retention has nothing to
mean there.

The M2 cursor is the opposite case and it is not a choice either: it MUST be
`key(1)` or the edge write into it is `edge_into_unkeyed_set`.

So the slot's answer is that there is no single retention policy for minted
rels; the rule's own arrow forces it, and the edge case is refused outright
(M3-1). Inheriting the head's `keep`/`key` is refused for the reason the
rel-spreading verdict already gave.

### slot_pipe_word — **there is no sqlite word for anonymous staging, and that matters**

Under ruling `vocabulary_tiebreak = sqlite_first_then_sql_standard`, the order
is sqlite spelling, then ANSI/SQL standard, then rx/prolog words "only where the
concept has no storage-plane spelling".

Taking that seriously: **sqlite's spelling for a chain of stages is `WITH ... AS
(...)`, a common table expression, and CTEs are NAMED.** SQLite has no pipe
operator; `||` is concatenation, which is adjacent enough to be confusable.
There is no sqlite or SQL-standard glyph for "cut this body here and let the
seam be anonymous".

So the ruling's own third tier — rx/prolog words where the concept has no
storage-plane spelling — does not obviously unlock here, because the concept
DOES have a storage-plane spelling: it is `WITH`, and what `WITH` names is what
the today spelling already names. Three readings, and the choice is a user call:

1. **`|>` as the rx-tier glyph.** The concept "anonymous stage" genuinely has no
   sqlite spelling, so the third tier applies. Free today (receipt 5b), already
   the spelling the 2026-07-27 aggregate analysis recorded for the shipped
   surface. Cost: a glyph the DCG must own, and a word that is not in any of the
   three vocabularies the law names.
2. **`with` as the sqlite-tier word.** `alert(...) <- with reading(...), ... with
   ...` reads badly and `WITH` names its stages in SQL, so borrowing the word
   while dropping the naming is worse than borrowing nothing.
3. **No word.** The stage is a named rel, which is `WITH`'s own answer, and the
   census says M3 saves 2 rules on 2 of 14 programs.

This lab does not pick. It states that reading 1 is the only one that is
coherent, and that it needs the ruling's third tier to be read as applying —
which is a user's call on the ruling, not a lab's.

---

## What actually makes the corpus not read like point-free rx

Worth saying plainly because it is what the user asked and none of the three
moves is the answer.

An rx pipeline reads point-free because **the value is anonymous**: `map`,
`filter` and `scan` all receive one unnamed value and return one unnamed value,
and the chain never spells a variable. A dl6 rule body cannot do that, because a
row has N columns and every one of them is joined by NAME. `Doubled := Raw * 2`
names two variables where `map(r => r * 2)` names none.

M3 removes the names of the intermediate RELS. It does not, and structurally
cannot, remove the names of the intermediate VALUES — receipt 2b shows the
minted stage rels carrying exactly the same rows under different names, with
`Sensor`, `Raw`, `Doubled` and `Shifted` all still written out. That is the gap
between "three rules and two declarations" and "one rule", and it is real; it is
just not the same gap as "reads like rx".

The honest summary: **the moves make the corpus SHORTER, and M2 substantially
so. They do not make it point-free.** Whether shorter is what was wanted is the
user's call, and this verdict deliberately does not assume it.

---

## Fixture promotion candidates

Distilled for the coordinator. All run on the shipped engine; none needs a
construct.

| candidate | shape | grades |
|---|---|---|
| `pairwise_skips_middle_value_at_one_change_per_tick` | `today/pairwise.dl6` + `today/pairwise.schedule.json` | Q1e. A real defect with zero coverage today. Pin the wrong answer so the fix turns it red. |
| `pairwise_correct_with_idle_tick_between_changes` | same program, `probe/pairwise_gapped.schedule.json` | Q1d. Its twin; together they are the cadence condition as a graded pair. |
| `keyed_write_of_identical_row_is_zero_delta` | `today/distinct_until_changed.dl6` | Q1b. The `distinctUntilChanged` claim, currently pinned nowhere as a fixture. |
| `latest_samples_without_triggering` | `today/with_latest_from.dl6` | Q1c. `latest/1`'s asymmetry as a whole program rather than a one-construct fixture. |
| `fold_over_log_head_forks_on_every_prior_row` | `break/scan_on_log_head_unsafe.dl6` | 4a. One hit produces two rows. This compiles clean today and is the argument for whatever check M1 would carry. |

---

## Open, named, not guessed

1. **slot_pipe_word needs a ruling read, not a lab pick.** Whether
   `vocabulary_tiebreak`'s third tier applies when the concept has a storage-plane
   spelling that names what this move wants anonymous.
2. **Minted-rel visibility (slot_stage_naming) is a user-level call.** Legal names
   are gradable and collidable; `__` names are collision-proof and unwritable.
   Receipt 2c shows the tick log is what changes.
3. **The Q1e pairwise defect is unowned.** It is not a point-free question and
   this lab did not fix it.
4. **Doc truth, found in passing and worth a separate pass.** `pre_occurrence_loop`
   landed 2026-07-30 and `bop check` accepts `pre` in an edge body (exit 0, and
   receipt 3 runs a `pre` program on the served engine byte-identically to the
   oracle). Three places still say otherwise: `registry.pl:76` marks `pre/1`
   `refused` with `wrapper(rel_atom, refuse(goal))`; `SCOREBOARD.md:209` and
   `:486` list `edge_body_needs_pre` at 13; `SYNTAX.md`'s generated table and its
   context table both call it refused. This lab changed none of them.
5. **`merge_family.pl:111` is an oracle-only fixture** (receipt 5d): its
   `not(pre(counter(Name, _)))` base arm is `edge_body_with_negation` on the
   compiler.

## Coordinator addendum (landing)

Re-verified on merged main (post affinity-fix): receipts 29/29, conformance
258/0. The pre-drift finding reproduced by the coordinator's own probe: a
pre-in-edge-body program compiles exit 0 through `bop check` while
registry.pl:76 still reads `refused` — filed as ARCH `pre_registry_drift`
(which arc opened the path, whether the compiled semantics is the
measured-wrong sampled reading on chained occurrences, and why the sweep's
13 edge_body_needs_pre fixtures still refuse while this probe compiles are
the three questions). Lab files died on landing; last full copy at commit
`89ccaccf` (`git show 89ccaccf:v6/prolog/labs/point_free/receipts.sh`).
