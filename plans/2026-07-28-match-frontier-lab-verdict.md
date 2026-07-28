# MATCH + ARROWS + FRONTIERS: lab verdict (2026-07-28)

Contract: `plans/2026-07-28-match-frontier-lab-header.md`.
Lab: `v6/prolog/labs/match_frontier/` (7 files, entry `lab.pl`, 63 PASS, exit 0,
stdout is PASS lines only, stderr empty).
Untouched and re-run to prove it: conformance `go.pl` 110 PASS, `roundtrip.sh`
ALL GRADES PASS (G1 110/110, G2 no parse errors, G3 110/0), `cd v6/tsv2 &&
pnpm test` 6/6.

---

## VERDICT LINE

**The arm design holds on the event axis and cracks in four places: the Ta
frontier, the flagship transition rule, negation inside `+>` arms, and every
lifecycle arm over a Log rel.**

More precisely:

- **HOLDS.** `next` / `finalize` / bare-atom arms under a `+>` arrow are pure
  sugar. They ground out to exactly one kernel rule each, with no new engine
  phase, no new delta kind, and no change to R7. `finalize` is *defined on* the
  minus side of the boundary diff that already ships. 23 of 36 legality cells
  are legal and every one of them has a written rx lowering.
- **CRACKS 1: Ta is not a frontier.** It cannot be given semantics by an rx
  scheduler (schedulers are semantically transparent), and the only rx shape
  that re-enters a source is a Subject bridge, which a standing law bans. The
  user's dissolution hypothesis is **CONFIRMED** by the model: a pending rel
  plus a consuming rule reproduces primitive Ta exactly, with one named extra
  quiescence tick, and wins on determinism, durability, visibility and
  matchability. Ta needs no marker, no arrow variant, and no scheduler
  distinction. The frontier menu shrinks to **Tn / Ti + rels**.
- **CRACKS 2: the flagship transition rule loses transitions.** N keyed
  replaces of one key inside one tick produce ONE firing, not N. The same two
  inputs delivered one per tick produce two firings. The multiplicity is a
  function of how the scheduler batched arrivals, not of the data.
- **CRACKS 3: `not()` in a `+>` arm is unstratified and arrival-order
  dependent.** Same program, same two rows, different order inside one tick,
  different output, no diagnostic anywhere.
- **CRACKS 4: a lifecycle arm over a Log rel is statically dead.** Log rels
  never emit a minus delta, and the one case where a Log row genuinely leaves
  (retention) prunes before the boundary diff and emits nothing at all.

Cracks 2, 3 and 4 predate the arm design. What the arms change is that they
make all three *easy and natural to write*, which converts three engine
subtleties into three user-facing traps.

---

## Table 1 (Q1): LEGALITY MATRIX

36 cells, none blank. `desugar_cell/3` succeeds on every legal cell and throws
the exact named term on every refuse cell (`lab.pl` q1 checks).

Legend: **L** legal, **R(err)** refuse with that named error, **A(SLOT)**
ambiguous against that slot.

| item | `+>` arm | `->` arm | classic `<+` body | classic `<-` body |
|---|---|---|---|---|
| bare atom | L trigger, `only(Atom)` | L **join, NOT a trigger** | L any-body-atom trigger | L ordinary read |
| `next()` | L identical to bare | R(`lifecycle_arm_in_level_arm(next)`) | A(SLOT-SUGAR-SCOPE) | R(`lifecycle_arm_in_level_arm(next)`) |
| `finalize()` | L `only(departed(Atom))` | R(`departed_in_level_rule/1`) | L `departed/1` | R(`departed_in_level_rule/1`) |
| `complete` | A(SLOT-COMPLETE) | R(`lifecycle_arm_in_level_arm(complete)`) | A(SLOT-COMPLETE) | R(`lifecycle_arm_in_level_arm(complete)`) |
| async-marked atom | A(SLOT-TA-MARK) | R(`async_in_level_rule`) | A(SLOT-TA-MARK) | R(`async_in_level_rule`) |
| comparison guard | L (tsv2 refuses) | L | L (tsv2 refuses) | L |
| row pattern | L | L | L | L |
| enum destructure | L (tsv2 refuses) | L | L (tsv2 refuses) | L |
| `not()` | L **UNSTRATIFIED** | L stratified | L **UNSTRATIFIED** | L stratified |

Counts: 23 legal, 8 refuse, 5 ambiguous.

Notes that are findings, not colour:

1. **The `->` arm's subject is not a trigger.** A level rule has no occurrences
   at all (`engine.pl:286` runs `level_closure/5` over the whole base), and
   `only/1` in a level body is an ordinary read (`body.pl:99`,
   `level_eval.pl:109`). So the same syntactic position means "the thing that
   fired" under `+>` and "a table I join" under `->`. That is the single
   biggest readability cost of mirroring the arrows.
2. **Two refusals are already engine law, not lab opinion.**
   `engine.pl:111-112` throws `departed_in_level_rule(Ref)` at load time, and
   `body.pl:102` makes `departed/1` unsatisfiable as a read. The lab reuses the
   exact terms.
3. **"tsv2 refuses" is a lowering gap, not a language gap.** `analyze.pl`
   refuses `edge_marked_with_extra_goal`, `edge_body_needs_comparison` and
   `edge_body_needs_json_destructure` today. The oracle allows all three.

---

## Table 2 (Q2): FRONTIER MATRIX

24 cells. 12 coherent, 7 incoherent, 5 refused.

| axis | time | head kind | verdict | termination | tick log shows |
|---|---|---|---|---|---|
| value | Tn | level | coherent, **arrival-derived rows only** | bounded | plus-delta at T, arrival-derived half only |
| value | Tn | edge-set | coherent (`:236-254` + per-occurrence `:210-212`) | bounded | one net plus-delta; intermediates invisible |
| value | Tn | edge-log | coherent, plus a stamp `:241-242` | bounded | one plus-delta **per new stamp** |
| value | Tn | effect demand | coherent for the demand row only | bounded | demand row only, nothing of the result |
| value | Ti | level | coherent (`PostWriteLevelRows :296`) | drain-capped | delta at T, readable T+1 |
| value | Ti | edge-set | coherent (Tn implies Ti) | drain-capped | unchanged |
| value | Ti | edge-log | coherent | drain-capped | unchanged |
| value | Ti | effect demand | coherent for a synchronous host only | drain-capped | demand at T, fill whenever |
| value | **Ta** | level | **incoherent**: a view deferring its value is a wrong view | bounded | nothing distinguishable from Ti |
| value | **Ta** | edge-set | **incoherent**: no rx scheduler gates visibility; only a rel holds a value across a boundary | bounded | identical to Ti except the tick INDEX, which is engine-chosen |
| value | **Ta** | edge-log | **incoherent**, plus the stamp minting point is unspecified | bounded | stamp order becomes engine-chosen |
| value | **Ta** | effect demand | **incoherent**, and this is the exact cell a pending rel already covers | bounded | primitive Ta shows nothing; a pending rel shows the queue |
| occurrence | **Tn** | level | **refused** | unbounded | nothing, the run does not terminate |
| occurrence | **Tn** | edge-set | **refused**: in-transaction loop, no cap exists at occurrence granularity | unbounded | nothing |
| occurrence | **Tn** | edge-log | **refused**, and every lap mints a stamp | unbounded | nothing |
| occurrence | **Tn** | effect demand | **refused** | unbounded | nothing |
| occurrence | Ti | level | coherent | drain-capped | N-stage chain costs N ticks |
| occurrence | Ti | edge-set | coherent (`ArrivalCarry :302-304`, departures `:307-311`) | drain-capped | **the collapse lives here**: a row written and overwritten inside one tick is never an occurrence |
| occurrence | Ti | edge-log | coherent, one per stamp | drain-capped | duplicates fire twice |
| occurrence | Ti | effect demand | coherent, the T+1 occurrence starts the host | drain-capped | demand at T, host at T+1 |
| occurrence | **Ta** | level | **refused**: a level rel has no occurrence of its own | bounded | nothing |
| occurrence | **Ta** | edge-set | **incoherent**: delivery tick is engine-chosen, so the log stops being a function of program plus schedule | **stranded** | queue invisible; a stranded queue is silent loss |
| occurrence | **Ta** | edge-log | **incoherent**, plus engine-chosen stamp order | **stranded** | same |
| occurrence | **Ta** | effect demand | **incoherent**, and a pending rel covers it at zero construct cost | **stranded** | same |

**The asymmetry nobody stated (ambiguity 3 below):** "Tn" is two different
frontiers depending on head kind. An edge write's value is Tn-visible to later
occurrences in the same tick, but a level row derived from that write is not,
because `MidLevel` is computed once at `:286` and passed frozen through the
whole occurrence loop (`:291`), with the post-write recompute only at `:295`.

---

## Table 3 (Q3): RX DIRECTNESS

34 lowering expressions, one per legal Q1 cell and per coherent Q2 cell, plus
the four construct-level questions. Counts are the answer to "how direct":

| grade | count | meaning |
|---|---|---|
| DIRECT | 24 | plain operator composition; callbacks are pure functions |
| DIRECT-BUT-VACUOUS | 1 | compiles as plain rx and delivers none of the claimed semantics |
| ENCODED | 7 | needs a state table, a scan, or an order imposed from outside rx |
| IMPOSSIBLE | 2 | the semantics exceed rx |

The seven ENCODED and two IMPOSSIBLE, named:

| key | expression | grade | why not direct |
|---|---|---|---|
| bare atom in a classic `<+` body | `merge(...bodyAtoms.map(arrivalsOf)).pipe(mergeMap(hit => from(joinRestAgainstStore(hit))))` | ENCODED | the C2 any-body-atom model re-reads the rest of the body from the store |
| `not()` in `+>` arm / `<+` body (2 cells) | `withLatestFrom(liveTable$), filter(([row, live]) => !live.has(row.key))` | ENCODED | `liveTable$` is a held state table, not an rx value |
| `not()` in `->` arm / `<-` body (2 cells) | `concat(...strata.map(s => defer(() => of(recompute(s))))).pipe(last())` | ENCODED | `concat` expresses the stratum order; nothing in rx computes the strata |
| value/Tn/edge-set | `from(occurrences).pipe(concatMap(occ => applyWritesToStore(occ)))` | ENCODED | the per-occurrence store read is a mutation, not a value |
| Ta as a primitive queue | `const queue$ = new Subject(); merge(arrivals$, queue$)...` | ENCODED | **the only rx shape that re-enters a source is a Subject bridge, which the standing no-Subject-bridge corollary bans outright** |
| occurrence/Tn (all head kinds) | NONE | IMPOSSIBLE | re-entrant synchronous emission into a subscriber mid-`next()`; with `queueScheduler` it is an unbounded trampoline with no cap |
| incremental min/max over a retractable set | NONE | IMPOSSIBLE | `scan` cannot un-fold; a retracted contribution needs the whole bag |

The one DIRECT-BUT-VACUOUS entry is the headline:

```
edgeWrites$.pipe(observeOn(asyncScheduler))
```

This compiles, reads correctly, and **changes nothing observable**. rx
schedulers change WHEN a value is emitted in wall-clock terms and never WHICH
values are emitted or in what order. Spelling Ta after an rx operator that
provably cannot implement it is worse than inventing a word.

Four construct-level lowerings worth naming:

| construct | expression | grade |
|---|---|---|
| `finalize` arm | `relRows$.pipe(startWith([]), pairwise(), mergeMap(([prev, next]) => from(prev.filter(r => !next.includes(r)))))` | DIRECT |
| `complete` arm | `groupBy(row => row.scope, { duration: g => scopeGone$.pipe(filter(s => s === g.key)) })` | DIRECT |
| update / transition arm | `relRows$.pipe(pairwise(), mergeMap(([p, n]) => from(pairUpByKey(p, n).filter(([o, x]) => o && x && o !== x))))` | DIRECT |
| Ta as a pending rel | two ordinary rules, both already-lowered shapes above | DIRECT |

The `complete` lowering is a receipt in its own right: rx `groupBy`'s duration
selector completes the inner group exactly when the scope row departs, which is
the mechanical proof that `complete == finalize(scope_row)` and needs no
construct.

---

## Table 4 (Q4): CONTRADICTION HUNT

Every row ran. Scenarios on the real oracle call `engine:run_program/5`
read-only; only the two things the oracle has no machinery for at all (a
primitive Ta queue, spilling instead of throwing at the cap) run on the lab's
model interpreter, and `b2` proves the model agrees with the oracle on a shape
both can express before the model is trusted for the rest.

| # | scenario | runs on | result |
|---|---|---|---|
| a1 | 2 replaces of one key in ONE tick | oracle | **1 firing**, reporting `v0 -> v2`; `v1` never existed as far as the program can tell |
| a2 | the same 2 replaces across TWO ticks | oracle | **2 firings**, `v0->v1` and `v1->v2` |
| a3 | 1 replace each of 2 keys in one tick | oracle | 2 firings, one per key |
| a4 | 2 replaces, rel EMPTY at tick start | oracle | **0 firings** |
| b1 | finalize cascade cycle | oracle | `drain_overflow(100)` thrown, loudly |
| b2 | same cycle in the model | model | `drain_overflow(100)`, model agrees |
| b3 | spill-at-cap instead of throwing | model | run RETURNS a 101-tick log with a **nonempty residue**: silent loss |
| c1 | self-retraction chain of 3 | oracle | bounded, one departure per drain tick, 4 ticks, terminates |
| d1 | `not()` over an edge-headed rel, two arrival orders | oracle | `out(a)` vs `out(b)`: **different output, same inputs** |
| d2 | `not()` over a level-headed rel, two arrival orders | oracle | identical output, order-independent |
| e1 | lifecycle-outer vs enum-outer nesting | desugar | **identical rule sets**: nesting order not forced |
| e2 | a match block with an uncovered tag | desugar | desugars **cleanly, no complaint of any kind** |
| f1 | primitive Ta under two engine delivery choices | model | **two different tick logs** from one program and one schedule |
| f2 | pending-rel encoding vs primitive Ta | model | strip the pending rel's deltas and the logs are **identical** |
| f3 | the exact residual difference | model | **one trailing quiescence tick**, and nothing else |
| f4 | pending encoding run twice | model | identical; **no engine knob exists** |
| g1 | one `finalize` + one `next` in one body | oracle | fires ONCE, on the departure; the `next` atom degrades to a store read |
| g2 | TWO `finalize` atoms in one body | oracle | **never fires**; statically dead, nothing warns |
| h1 | old and new of one rel in one body | oracle | works; `departed` binds old, the join binds new |
| h2 | pure delete, no replacement | oracle | **silently produces nothing** |
| x1 | `finalize` over a Log rel | oracle | **never fires**, however the rel is retained |
| x2 | retention prune | oracle | row leaves the store with **no delta of any kind** |

### The multiplicity table the contract asked for (scenario a)

| situation | firings |
|---|---|
| N replaces of ONE key in ONE tick, rel non-empty at tick start | **1** |
| N replaces of ONE key across N ticks | N |
| 1 replace each of M keys in ONE tick | M |
| N replaces of ONE key in ONE tick, rel EMPTY at tick start | **0** |

The cause is `engine.pl:299-304` (carry-out is boundary-observable writes only,
the R2 rider) plus the set-diff boundary. It is not fixable inside the arm
sugar, and an "update arm" spelled the SQL-trigger way inherits it unchanged
unless it fires per occurrence, which would break R2. That fork is
SLOT-UPDATE-ARM.

### Scenario g: the one-body-one-time-cut reading is FORCED, not chosen

`engine.pl:162-166` substitutes away only the ONE `departed` goal the
occurrence matched, and `body.pl:102` makes `departed/1` unsatisfiable as a
read. So the departure is always the cut and any `next()` atom in the same body
always degrades to a store read against the settled state. There is a coherent
reading and the engine cannot choose another one.

The corollary is contradiction C5: a body with two `finalize` atoms is dead by
construction, since whichever departure fires, the other `departed` goal
remains and fails.

### Scenario f: the Ta dissolution hypothesis, graded

Both spellings, same program, same schedule:

```
primitive Ta:   out(X)  <+ta src(X)
pending rel:    pending(X) <+ src(X)
                out(X)     <+ pending(X)
```

| run | tick log |
|---|---|
| primitive Ta, engine delivers at T+1 | `1:{src:+a}` `2:{out:+a}` |
| primitive Ta, engine delivers at T+2 | `1:{src:+a}` `2:{}` `3:{out:+a}` |
| pending rel (only legal policy) | `1:{src:+a, pending:+a}` `2:{out:+a}` `3:{}` |
| pending rel, `pending/1` stripped, trailing empties trimmed | `1:{src:+a}` `2:{out:+a}` **identical to primitive Ta** |

Reading:

- primitive Ta's tick log is **not a function of program plus schedule**. It is
  a function of an engine choice. That breaks the standing item-9 grading law
  (logs diffed byte-for-byte against the oracle) at the root.
- the pending-rel encoding **reproduces primitive Ta exactly**. The one named
  difference is a single trailing quiescence tick, which is the edge write's own
  carry (`engine.pl:302-304`), not noise.
- the encoding additionally wins on three structural points primitive Ta cannot
  offer at all: the queue is a **durable rel** (the exactly-once endurance law
  already covers it), the queue is **visible in the tick log** (self-diagnosis
  law), and the queued rows are **ordinary rows and therefore matchable with
  ordinary arms**.

That last point answers the user's "is the carry itself matchable, it is some
next event" question: **under dissolution, yes, for free.** A `finalize(pending(X))`
arm tells you the deferred item was consumed, with no new construct at all.

**Dependency worth stating.** The dissolution works because the C2 unmarked
any-body-atom trigger ruling is in force: `consume(X) <+ pending(X), gate(G)`
makes BOTH atoms triggers, so a gate row arriving later re-fires the body
against settled pending rows. Under a marked-only trigger model the pending row
would sit unconsumed. Dissolution is therefore not independent of the C2 ruling;
it rides on it.

### Scenario b3: SLOT-SPILL, answered

Spilling the over-cap carry into a Ta queue instead of throwing does not fix the
nontermination. It converts a loud `drain_overflow(100)` into a run that returns
a log while the work sits in a queue nothing will ever deliver. That trades a
failure the self-diagnosis law demands be visible for one that is invisible.
**Error at the cap. Do not spill.**

---

## Table 5 (Q5): SYNTAX OVERLOAD

Seven woes, nine collision rows, twenty priced alternatives. Four parse
receipts run against the live SWI reader; a fifth records what is NOT in the
operator table.

### Collisions

| woe | symbol | where | nature | severity |
|---|---|---|---|---|
| level-arm arrow | `->` | prolog term form | ISO if-then-else, `op(1050, xfy)`. **Receipt: `body.pl:126` hands the whole `(alpha, beta -> gamma)` term back as ONE trigger atom named `(->)/2`, silently.** | HIGH |
| level-arm arrow | `->` | .dl surface | ruling `q8_key_vs_arrow` (`rulings.pl:50`) already assigns `->` the program/world column split on effect rels; the v5 `matches() -> (body)` idiom is live | HIGH |
| SSU arrow | `=>` | prolog term form | SWI single-sided-unification rules, `op(1200, xfx)`, same priority as `:-`; a top-level `Head => Body.` IS an SSU clause | HIGH |
| bar separator | `\|` | prolog term form | `op(1105, xfy)` plus the list-tail role; `{a \| b}` parses as a disjunction term. Also collides with the deferred `\|>` (`cut_pipe`) | MEDIUM |
| `match` word | `match` | SQL vocabulary | SQLite has a `MATCH` operator. Worse: `match` promises exhaustiveness, and under the typed-columns ruling there are no enum types, so no exhaustiveness check is decidable (scenario e2) | MEDIUM |
| lifecycle words | `finalize` | rx vocabulary | rx `finalize()` runs per SUBSCRIPTION on unsubscribe/complete/error; the design uses it per ROW. Same family, different granularity, which reads as correct and is not | HIGH |
| lifecycle words | `next` | rx vocabulary | correct for the plus envelope, but collides with v5 `@next`, which names the Ti FRONTIER. One word, two axes | MEDIUM |
| Ta marker | `@async` | at-marker | user has asked to avoid `@`; and v5 `@async` named a different thing, so continuity is a false friend | MEDIUM |
| event-arm arrow | `+>` | prolog term form | **no collision.** `current_op/3` reports nothing for `+>`, `<++`, `+>>` or `~>` | LOW |

One extra fact worth having: **`<-` and `<+` are not in the global operator
table either.** `engine.pl:72-73` declares them at 1150 xfx inside its own
module, and an `op/3` directive in a module is module-local in SWI, which is
why `level_eval.pl`, `ticklog.pl` and every file in this lab re-declares them. A
mirrored-arrow family doubles that per-module declaration cost.

### Alternatives, priced

**Level-arm arrow (3 options).**

| spelling | pro | con |
|---|---|---|
| **keep `<-`, drop the mirrored arrow** | zero new symbols, zero collisions, the corpus already reads this way, costs nothing | loses the source-major reading; the subject is no longer visually first |
| `=>` for BOTH arms, axis carried by the arm item | one arrow instead of two; the lifecycle word already tells you the axis | collides with SWI SSU at clause priority; DCG surface only, never the term form |
| `~>` | free glyph, reads as "maintained" rather than "appended" | not an rx, prolog or SQL word |

**SSU arrow (2), bar separator (2)** are in the lab file; both reduce to "do
not use the glyph in the term form".

**`match` word (3 options).**

| spelling | pro | con |
|---|---|---|
| `groupBy` | literally the rx operator this lowers to; makes no exhaustiveness promise | rx `groupBy` keys by a function, not a pattern |
| `materialize` | this IS the design (tagged envelopes); the most honest single word | no error arm here, no per-row complete, so it over-promises differently |
| `partition` | no exhaustiveness promise; exactly what non-lifecycle arms do | rx `partition` is binary; N arms stretch it |

**Lifecycle words (3 options).**

| spelling | pro | con |
|---|---|---|
| **SQL trigger family: `inserted` / `deleted` arms with OLD and NEW aliases** | exact prior art (`AFTER INSERT`/`AFTER DELETE`/`AFTER UPDATE OF`); unambiguous inside the SQL family; and `AFTER UPDATE` gives OLD and NEW in ONE body, which **dissolves the flagship transition rule's two-trigger cut problem entirely** | introduces an update arm, whose per-occurrence vs per-boundary reading is a new ambiguity (SLOT-UPDATE-ARM) |
| `next` / `departed` | `departed/1` is already the shipped kernel goal, so arm word and kernel word are one word | `departed` is an invented word, which the vocabulary law rejects |
| `next` / `delete` | both SQL words, obvious on first reading | `delete` reads as an imperative command, not an observed event |

**Ta marker (5 options, no-`@` first per the user directive).**

| # | spelling | pro | con |
|---|---|---|---|
| **A** | **NO MARKER AT ALL: pending rel plus a consuming rule** | zero constructs, which beats every spelling by definition. Durable queue (endurance law covers it free), visible in the tick log (self-diagnosis law), matchable with ordinary arms. Graded in f1-f4: reproduces primitive Ta exactly. Directly parallel to `clock_residency` | one extra rule and one extra rel per deferred hop, plus one quiescence tick. The user writes the queue instead of the engine hiding it |
| B | `<++`, a doubled edge arrow carrying the frontier on the RULE | the user's own preference over any `@`; glyph is free; carrying it on the rule is more honest than on the atom | arrow proliferation (six glyphs with mirrors); and it keeps a primitive queue, which f1 shows is nondeterministic and therefore ungradeable |
| C | `observeOn(async, Atom)`, the literal rx word | satisfies the vocabulary law exactly; self-documenting | **the word is a trap.** rx schedulers are semantically transparent; this names an operator that provably cannot deliver the semantics (graded DIRECT-BUT-VACUOUS) |
| D | `async(Atom)`, a wrapper in the style of `latest()`/`combine()` | no `@`; sits in the existing wrapper family, parser needs nothing new | `async` is a JS keyword, not an rx/prolog/SQL word; keeps the primitive queue with all of B's problems |
| E | `@async`, v5 continuity | one row of migration cost | the user has asked to avoid `@`; and v5 `@async` named a different thing, so the continuity is a false friend |

**Event-arm arrow (2 options):** `+>` as the mirror of `<+` (free glyph,
genuinely readable, but two spellings for one rule means every diff, grep and
error message handles both), or no mirrored arrow at all (one spelling; the
match block already supplies the source-major grouping, so the arrow does not
have to).

---

## Table 6 (Q6): INVARIANT PRESERVATION

11 rows: 5 preserved, 3 broken, 3 needs-rule.

| invariant | verdict | evidence |
|---|---|---|
| sugar grounds out, lifecycle arms | **preserved** | two-arm block desugars to exactly the hand-written kernel pair |
| sugar grounds out, `complete` arm | **needs-rule** (SLOT-COMPLETE) | no kernel form. Candidate that needs no construct: `complete == finalize(scope_row)`, rx `groupBy` duration selector is the exact lowering |
| sugar grounds out, async marker | **needs-rule** (SLOT-TA-MARK) | no kernel form exists and none can: `run_ticks/7 :367-379` has two legs and no third queue. Under dissolution it grounds out to two ordinary rules |
| one rel = one rule kind | **needs-rule** (SLOT-LEVEL-ARMS) | a block with a `+>` arm and a `->` arm heading the SAME rel desugars cleanly into `(Head <+ _)` and `(Head <- _)`. The source-major shape makes the violation a two-line edit |
| stratification, `->` arms | **preserved** | `level_eval.pl:121-142` stratifies the desugared rule set; scenario d2 shows order-independence |
| stratification, `+>` arms | **BROKEN** (d1) | `engine.pl:284-286` hands `level_closure/5` only level rules. Edge rules are never stratified |
| occurrence multiplicity | **preserved** | one firing per occurrence holds exactly; the unstated consequence is the a1-vs-a2 collapse |
| R7 boundary diff | **preserved** | finalize arms are defined ON the minus side (`:331-341`); checked, finalize firings equal minus-delta count |
| retention / keep | **BROKEN** (x1, x2) | Log rels never emit a minus delta; retention prunes at `:293` before `boundary_deltas/6` at `:298` and emits nothing |
| content-addressed effect identity | **preserved, with a reading that must be stated** | finalize means THE ROW LEFT THE STORE, never THE WORLD WORK STOPPED. The `effect_abort` ruling is explicit that cancellation is best-effort and never semantic, so a finalize arm used as a compensation hook depends on something the ruling denies |
| exactly-once endurance | **BROKEN** | see below |

### Exactly-once endurance: what replays after a crash between drain ticks?

**Nothing, because the carry set is not stored anywhere.** `engine.pl` threads
`CarryOut` as a runtime term through `run_ticks/7` (`:370-379`). tsv2 does not
even keep the rows: it reduces carry to a boolean (`tickLoop.ts:31`,
`emit_ts.pl:560-561`) and re-derives triggers from this tick's own arrivals,
which is exactly why `analyze.pl`'s `check_edge_body_refs_not_derived` refuses
`edge_trigger_is_derived` outright.

So a lifecycle arm whose departure occurrence is sitting in the carry set when
the process dies loses that firing with no trace. The lab's structural receipt:
in the departure fixture the pending occurrence exists between tick 2 and tick
3, and no rel anywhere in the tick log holds it.

**This is an independent argument for dissolution that has nothing to do with
Ta.** A pending rel is a durable row the endurance law already covers. If the
carry set itself were materialized as a rel, this whole class would close.

---

## Numbered ambiguities, mapped to slots

| # | ambiguity | slot | status |
|---|---|---|---|
| 1 | Does `complete` mean scope close, and is it distinct from finalize of the scope row? | SLOT-COMPLETE | **CANDIDATE ANSWER**: they are the same thing. rx `groupBy` duration selector is the exact lowering. Zero constructs. Not resolved, proposed |
| 2 | An arm always emits the MARKED spelling `only(...)`; the corpus writes the same rules unmarked. Occurrence-identical for a single-rel-atom body, divergent the moment an arm carries a second rel atom as a guard | SLOT-LEVEL-ARMS | **OPEN**: which spelling is canonical decides whether arm guards can be rel atoms at all |
| 3 | "Tn" is two frontiers: edge-write values are Tn-visible, level rows derived from them are not (frozen `MidLevel`) | new, folded into SLOT-LEVEL-ARMS | **OPEN**: nothing in the sketch says so |
| 4 | Does a `cause` column belong on `finalize`? | SLOT-CAUSE | **PARTIALLY ANSWERED**: replace vs pure-delete is already derivable from whether the same-rel join binds (a1 vs h2). Level-support loss vs outside retraction is NOT distinguishable. Still OPEN for the second half |
| 5 | Drain overflow: error or spill to Ta? | SLOT-SPILL | **ANSWERED: error.** Spilling trades a loud failure for silent loss (b3) |
| 6 | Two-axis nesting order | SLOT-NEST | **ANSWERED: not forced** (e1). The residual is a different question: `match` over-promises exhaustiveness (e2) |
| 7 | Async carries marked or unmarked? | SLOT-TA-MARK | **ANSWERED: neither. Ta dissolves** (f1-f4, Q2 Ta rows, the DIRECT-BUT-VACUOUS lowering) |
| 8 | Final glyph choice | SLOT-ARROW | **PRICED, not picked**: 7 woes, 20 alternatives, recommendation ordering below |
| 9 | Are `->` arms restricted to guards and patterns, lifecycle arms refused? | SLOT-LEVEL-ARMS | **REFUTED AS POSED**: lifecycle arms in a level rule are ALREADY refused by engine law (`engine.pl:111-112` throws at load). The restriction actually needed is on the HEAD (one rel one rule kind), not on the arm item |
| 10 | May match-only sugar words (`next`, `finalize`) appear in a classic `<+` body? | SLOT-SUGAR-SCOPE (new) | **OPEN** |
| 11 | If an update/transition arm exists, does it fire per occurrence or per boundary? | SLOT-UPDATE-ARM (new) | **OPEN**. Per boundary inherits the a1 collapse; per occurrence breaks the R2 rider |

---

## Contradictions ranked by severity

| rank | contradiction | minimal reproducing scenario |
|---|---|---|
| **C1** | **Ta has no semantics an rx scheduler can carry, and the only rx shape that re-enters a source is a banned Subject bridge. Its tick log is a function of an engine choice, not of program plus schedule** | f1: one program, one schedule, two engine delivery choices, two different logs |
| **C2** | **The flagship transition rule silently loses N-1 of N intra-tick transitions; the count depends on scheduler batching, not on data** | a1 vs a2: the same two polls give 1 or 2 `changed` rows |
| **C3** | **`not()` in a `+>` arm is unstratified and arrival-order dependent, silently** | d1: `+src(a), +src(b)` yields `out(a)`; `+src(b), +src(a)` yields `out(b)` |
| **C4** | **A lifecycle arm over a Log rel is statically dead, and retention prunes with no delta at all** | x1 and x2 |
| **C5** | **A body with two `finalize` atoms is statically dead; nothing refuses or warns** | g2: both rels depart in one tick, the head rel stays empty |
| **C6** | **`match` promises exhaustiveness the type system cannot check (no enum types exist under the typed-columns ruling)** | e2: a block with an uncovered tag desugars cleanly |
| **C7** | **The Ti carry set is not durable in either implementation; a crash between drain ticks loses pending lifecycle firings with no trace** | q6 endurance structural receipt |
| **C8** | **One rel = one rule kind is a two-line edit away inside a single match block** | q6 one_rel_one_rule_kind check |
| **C9** | **`->` as the level-arm arrow is absorbed as a trigger atom silently in the term form, and contradicts the live `q8_key_vs_arrow` ruling in the surface** | `level_arrow_absorbed_as_trigger_atom` parse receipt |
| **C10** | **Spill-at-cap converts a loud failure into silent data loss** | b3: 101-tick log returned, nonempty residue |

C1 and C7 both point the same way, from opposite directions: **the thing that
carries work across a tick boundary should be a rel.**

---

## Prospective fixtures (written in the lab, NOT added to conformance)

1. **`departure_rename_as_finalize_arm`** . `engine_core.pl:117`
   `departed_fires_next_tick_on_retraction` re-expressed as a match block with
   a `finalize` arm. Graded on the tick log (the item-9 grading currency), not
   on term equality, because the arm form emits `only(departed(...))` where the
   corpus writes it unmarked. Both were run: **logs identical, finals identical,
   4 ticks.** Expected log unchanged, as the contract predicted.
2. **`transition_rule_keyed_replace_drives_changed`** . the flagship, one poll
   per tick, hand-written log confirmed. The same program batched differently
   is a1 and it loses a transition; that pairing is the fixture's real value.

---

## Syntax recommendation ordering

Priced, ordered, and offered as a recommendation the coordinator may discard.

1. **Ta marker: option A, dissolution. Nothing to spell.** Zero constructs beats
   every spelling by definition, and it is the only option that keeps the tick
   log a function of program plus schedule. It also answers the matchability
   question for free and closes half of C7. If the user later wants a
   *shorthand* for the pending-rel pair, `<++` (option B) is the right glyph to
   put on it, because it is sugar over two real rules rather than a primitive
   queue. Never `@`. Never `observeOn`, which names an operator that cannot do
   the job.
2. **Lifecycle words: the SQL trigger family**, `inserted` / `deleted` (or
   `next` / `deleted` if `next` is wanted for rx continuity), with OLD and NEW
   row aliases. It is real prior art, both words are SQL words, and the
   `AFTER UPDATE` shape gives OLD and NEW in one body, which removes the
   two-trigger cut question entirely. The cost is one new ambiguity
   (SLOT-UPDATE-ARM) instead of the several the `finalize` borrowing carries.
   Second choice: keep `finalize`, and write down loudly that it is per-row and
   not rx's per-subscription `finalize`.
3. **Level-arm arrow: keep `<-` and drop the mirrored arrow.** `->` is taken
   twice over (prolog if-then-else at 1050 xfy, and the live `q8_key_vs_arrow`
   ruling), and the failure mode in the term form is silent absorption as a
   trigger atom, not a parse error. If the source-major reading is wanted for
   `->` arms, buy it with the block structure, not with a second arrow.
4. **Event-arm arrow: `+>` is safe if it is wanted.** The glyph is genuinely
   free. The cost is real but ordinary: two spellings for one rule.
5. **Block word: `partition` or `groupBy` over `match`.** Both are rx words that
   promise only what the language can deliver. `match` is worth keeping only if
   someone intends to build the exhaustiveness check, which needs enum column
   types the typed-columns ruling does not have.
6. **Do not use `=>` or `|` anywhere in the term form.** `=>` is SSU at clause
   priority; `|` is an operator at 1105 xfy.

## Rules the design needs that nobody has stated

These are the "survives only under an extra rule" cases, listed as work, not as
opinion:

- refuse `finalize` / `departed` over a Log rel by name (C4)
- refuse a body with two departure trigger items by name (C5)
- refuse, or at minimum diagnose, `not(EdgeHeadedRel)` inside an edge body (C3)
- refuse a match block whose arms head one rel under both arrows (C8)
- decide whether the Ti carry set becomes a materialized rel (C7, and it is the
  same shape as the dissolution answer to C1)
