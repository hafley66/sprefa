# Time-plane unification lab, verdict

Header: `plans/2026-07-30-time-plane-unification-header.md`. Base
`51d0e193baa6c68c3e0525dfafb1e51c029614ae`. Runnable half
`v6/prolog/labs/time_plane/`, entry `bash v6/prolog/labs/time_plane/receipts.sh`
(exit 0 = every receipt held). Current run: **15 reference-engine receipts +
prototype blast-radius leg + one two-door case + cost measurement + sabotage,
7 PASS 0 FAIL.**

The user's spine, verbatim: "log rel is just rel with a time column ... rels
should have auto created_at_tick and updated_at_tick so to speak, and we could
historicalize them as well with a time column for easy versioning."

---

## 0. The short version

**H1 loses. H2 is already expressible and needs no plane. H3 is an ordinary
rel. One thing in the header's neighbourhood is worth doing, it is ten lines,
and it is not the unification.**

Three results carry the verdict.

1. **`log` cannot become sugar, and the reason is a measurement, not a
   preference.** A set rel absorbs an identical second arrival as a no-op and
   fires no occurrence (T1). A log rel fires two (T2). A set rel whose rows
   differ by one column also fires two (T3). So multiplicity is already free on
   the set plane once rows are DISTINCT, and what the log kind actually buys is
   the engine minting that distinctness when the world does not send it. An
   expansion module written in rules cannot mint it, because the occurrence it
   would need to fire on is the one the set plane just discarded. Any
   unification therefore keeps an engine branch; it relocates the branch from
   the store shape to the column fill, and the site count does not drop
   (section 5).

2. **The header's actual prize is separable and cheap.** Making retention emit
   an ordinary minus delta closes the retention-grading gap AND makes
   `finalize` over a log rel fire, which is a hole three prior arcs filed and
   none closed (stream-lab card 4, update-arm `SLOT-LOG-FINALIZE-REFUSAL`,
   consumption-arms assertion 17). Measured cost across the whole corpus: **one
   line of one artifact regrades**, conformance 209/209 unchanged, plunit
   239/239, sweep 145 total / 143 identical / **0 wrong**. Patch banked at
   `v6/prolog/labs/time_plane/retention_minus.patch`, 51 lines across two files.

3. **`created_at_tick` / `updated_at_tick` already work, today, with zero new
   constructs** (T15): `now/1` supplies the tick, `pre/1` carries the birth tick
   across a keyed replace, `not/1` supplies the base case. Two ordinary edge
   rules produce `thing(1, c, 1, 3)`: created pinned at tick 1, updated advanced
   to tick 3, across three replaces. H2 is sugar at most, class (a), and it is
   the same shape of boilerplate the stream lab priced for ordinals.

`locked(single_rel_type_system)` is not fought anywhere below.

---

## 1. Per-question verdicts

Each row carries the discriminating receipt, not the argument.

### Q1 identity split

**Verdict: the split is real, and it sits at the ARRIVAL boundary, not in the
row.**

With a seq column excluded from identity, a bare set rel does keep zero-delta
and a log rel does stack. That much holds. What the header did not ask, and
what decides the hypothesis, is who fills the seq.

| receipt | program | result |
|---|---|---|
| T1 | set rel, `+ev(a), +ev(a)` one batch | edge rule fires **once** |
| T2 | log rel, same two arrivals | fires **twice** |
| T3 | set rel, `+ev(a,1), +ev(a,2)` | fires **twice** |
| T13 | identical wire rows through both decls | 1 row on the set plane, 2 on the log plane |

T1 is the discriminating line. `absorb_set_arrival/5` returns `Changed = false`
for the repeat (`engine.pl:315-317`), and `absorb_arrivals/8` mints an
occurrence only on the changed branch (`engine.pl:301-305`). The second arrival
is not merely coalesced at the boundary, it is invisible to every rule in the
program. A `seq(...)`-style expansion has nothing to fire on.

Two escapes exist and both are priced, neither is sugar:

- the ENGINE fills the seq at absorption. Keeps a branch keyed on the decl,
  which is `kind(Ref, log)` under a different name.
- the WORLD fills the seq. Changes the arrival wire contract for every host,
  bind, and schedule, and hands programs the ability to forge order.

### Q2 the five cross-plane refusals

**Verdict: four survive or restate, one dissolves, and the one that dissolves
is answered by the ten-line prototype rather than by unification.**

| refusal | receipt | verdict on the unified model |
|---|---|---|
| `retract_from_log` (R7) | T6 | **SURVIVES, restated.** Under an engine-filled seq the world cannot name an occurrence, so the refusal keeps its force and gains a better sentence. The restatement the header allows: *no world or program action removes an occurrence; only the declared retention bound does, and it reports the reclamation.* The prototype implements exactly that and does not weaken R7, because the minus is engine-authored. |
| `log_on_level_headed_rel` | T7 | **SURVIVES unchanged.** A level rel's rows are recomputed from scratch every tick, so a seq column on one would be recomputed too and would not be monotone. TICK-MODEL theorem four is untouched. |
| `keep_on_non_log_rel` | T8 | **RESTATES.** Same check, new wording ("keep on a rel with no occurrence plane"). Zero behaviour change. |
| `missing_retention` | T9 | **RESTATES.** Same shape as above. |
| `keyed_log_rel` | T10 | **SURVIVES.** A key is a membership invariant and occurrences have no membership. Keying the seq column would be trivially true and meaningless; keying anything else contradicts stacking. |
| finalize-over-log (silent, no refusal today) | T5 baseline, receipt (p) prototype | **DISSOLVES.** See Q4. |

The finalize row is the one worth arguing about. Two prior arcs recommended
adding a refusal. The measurement says the better answer is to make the natural
spelling work: it costs less than the refusal would (a refusal needs two
implementations plus a fail-first fixture; the fix needs ten lines and regrades
one artifact line).

### Q3 retention visibility

**Verdict: it works, both doors, and the regrade is ONE line corpus-wide.**

Prototype: `engine.pl boundary_deltas/6` gains the symmetric half of the stamp
diff (stamps in `Store0` and not in `Store` become minus rows);
`1_incremental.ts boundaryDelta` drops its `relation.kind === "set"` guard on
the `del` array. The emitter already staged `sign: -1` events from the
retention `RETURNING` (`applyRetentionStatement`), so the guard was the only
thing suppressing them. Both doors were suppressing symmetrically, so **there
was no hidden divergence** of the review-A4 class here.

Measured, whole corpus:

| gate | baseline | under prototype |
|---|---|---|
| conformance | 209 PASS / 0 FAIL | 209 PASS / 0 FAIL |
| plunit | 239/239 | 239/239 |
| sweep | 145 total | 145 total, 143 identical, **0 wrong**, 2 pre-existing run_error |
| checked-in oracle artifacts changed | n/a | **1 of 145** |

The one regraded artifact, in full:

```diff
--- a/v6/prolog/compile/out/retention_count_prunes_oldest.oracle.jsonl
+++ b/v6/prolog/compile/out/retention_count_prunes_oldest.oracle.jsonl
 {"tick":1,"deltas":{"event":{"add":[["one"]],"del":[]}}}
 {"tick":2,"deltas":{"event":{"add":[["two"]],"del":[]}}}
-{"tick":3,"deltas":{"event":{"add":[["three"]],"del":[]}}}
+{"tick":3,"deltas":{"event":{"add":[["three"]],"del":[["one"]]}}}
```

The reason the regrade is this small is worth recording, because it is luck
rather than design: exactly **one** fixture in the 209-fixture corpus declares
`keep(count(N))` (`retention_count_prunes_oldest`, `event/1`, N=2), and its
expectations block carries only `final/2`, no `deltas/2`. Eighty fixtures
declare a log rel; seventy-nine of them use `keep(all)`, which prunes nothing.
So the corpus barely exercises retention at all. That is a coverage finding in
its own right, independent of this lab.

### Q4 finalize over a log

**Verdict: it fires, and the R7 restatement holds.**

Baseline T5: `gone(Name) <+ finalize(ev(Name))` over a `keep(count(1))` log rel
collects **zero** rows across three arrivals, with no refusal and no diagnostic.

Under the prototype, same program, oracle door:

```
{"tick":3,"deltas":{"ev":{"add":[[3,"c"]],"del":[[1,"a"]]}}}
{"tick":4,"deltas":{"ev":{"add":[[4,"d"]],"del":[[2,"b"]]},"gone":{"add":[[1,"a"]],"del":[]}}}
{"tick":5,"deltas":{"gone":{"add":[[2,"b"]],"del":[]}}}
```

Receipt (p) grades that program on BOTH doors, byte-diffed per rel: 8 deltas,
identical. The departure carry already routes minus deltas of listened rels to
the next tick (`engine.pl:431-434`), so nothing new was needed there.

R7 compatibility, stated rather than assumed: the minus does not say the
occurrence un-happened, it says the record was reclaimed under a bound the
program declared. The distinguishing property is that only `keep(...)` can emit
it. `retract_from_log` still throws (T6 runs under the prototype too, via the
conformance leg).

Cost, named: a pruning log rel with a `finalize` listener now mints drain ticks
for the departures (the 3-tick schedule above runs to 6). Programs that do not
bind `finalize` on that rel pay nothing, because `listened_departure_refs/2`
gates the carry.

### Q5 blast radius, measured

**Verdict: unification touches ~120 sites to delete ~6. The prototype touches 2
files to close the gap.**

Full site inventory below. Classification is against H1b, the only viable
reading (log = rel + engine-filled seq column, boundary-invisible), since H1a
(rule-level sugar) is refuted by Q1.

**engine.pl (the oracle)**

| site | predicate | verdict |
|---|---|---|
| `:213-214` | `entry_row/2` | **DIES** (srow/lrow unwrap collapses) |
| `:204` | store comment | DIES |
| `:466-467` | `delta_ref_is_set/2` | **DIES** |
| `:451` | `boundary_deltas/6` LogAdds arm | **DIES** (set diff over the widened row gives multiplicity) |
| `:216-219` | `log_stamps/3` | MOVES (becomes an ORDER BY on the seq column) |
| `:379-381` | `next_seq/3` | MOVES, and its scope becomes a graded contract (slot_seq_scope) |
| `:385-398` | `apply_retention/3`, `prune_rel/3`, `dropped_entry/3` | MOVES (prune by seq order) |
| `:297-305` | `absorb_arrivals/8` log arm | **SURVIVES** as a column-fill branch |
| `:309` | `absorb_arrivals/8` retract guard | SURVIVES |
| `:315-326` | `absorb_set_arrival/5` | SURVIVES (and becomes a wasted scan on log rels) |
| `:363-374` | `apply_edge_writes/6` | SURVIVES |
| `:514-521` | `seed_store/3` | SURVIVES as column-fill |
| `:155-158`, `:193-196` | `engine_check_order/1`, `engine_refusal/3` | SURVIVE (Q2) |
| NEW | seq allocation, boundary projection of the seq column | **ADDED** |

**lower.pl (the compiler)** — `:7`, `:20`, `:116`, `:227` `relplan_kind/3`,
`:698-706` `rel_ddl/6` log clause, `:1380-1390` `arrival_statement/2` log
clause, `:1487` edge trigger kind gate, `:1553-1613` `edge_statement_single/8`,
`:2645-2662` `retention_statement/3` + `retention_statements/3`, `:2816`
`boot_seed_statement/5`, `:2950` call site. **All SURVIVE.** Two change shape:
`rel_ddl` may gain a PK on the seq column, and `retention_statement`'s
`ORDER BY rowid` becomes `ORDER BY <seq>`, which is where slot_seq_scope
becomes a cross-target contract.

**emit_ts.pl** — `:43-44`, `:539`, `:544`, `:675`, `:680-683`, `:692`, `:701`,
`:733`, `:785`, `:831-842`, `:912-913`, `:922`, `:940`, `:965`, `:1172`,
`:1187`, `:1266`, `:1280`, `:1438`, `:1510-1511`, `:1522`, `:1557`, `:1596`,
`:1626-1628`, `:1657`, `:1722`, `:1898`, `:1936`, `:1950`, `:1999`. **All
SURVIVE**; they emit the kind into TypeScript and would emit the renamed kind
instead.

**0_program_check.pl** — `:73-78` `declared_kind/3` + `relation_kind/3`,
`:135-137` `keyed_log_rel`, `:141-142` `log_on_level_headed_rel`, `:146-149`
`keep_on_non_log_rel`, `:365-367` `missing_retention`. **All SURVIVE** (Q2).

**parse_dl.pl** `:394-395`, `:514-521` `keep_clause//1`, `:558`;
**print_dl.pl** `:228`, `:230`, `:303-304`, `:313-315`;
**3_clock_check.pl** `:153` `relation_plane/3`, `:288`. **All SURVIVE** (the
`log` word and `keep(...)` are still spelled).

**v6/tsv2/runtime** — `1_incremental.ts:313` `applyEdgeStatement`, `:541`
`applyRetentionStatement`, `:591` `boundaryDelta`, `:636`, `:660`, `:708`
`applyArrivals`; `diff.ts:35` `multisetDiff`; `types.ts:120`, `:147`,
`:196-200`, `:209`. `boundaryDelta:591` **DIES** into the uniform diff; the rest
SURVIVE.

Totals: **6 die, 6 move, ~105 survive, 2 new sites appear.** Against that, the
prototype is 51 patch lines across `engine.pl` and `1_incremental.ts`.

There is also a generated-code tail the inventory surfaced: `retract_from_log`
appears in 292 emitted `.ts` modules under `compile/out/` and
`v6/tsv2/gen_emitted/`, one throw site each. Those regenerate, so they are
mechanical, but they are why a kind rename is not free.

### Q6 metadata plane cost

**Verdict: measured at 7.5 bytes/row and +30-34%; the statement claim is
PROVEN; and the plane is unnecessary because programs can already do it.**

`bash v6/prolog/labs/time_plane/metadata_cost.sh`, real SQLite 3.43.2, 100k
rows, page_count × page_size:

| shape | base | with `created_at_tick` + `updated_at_tick` | delta |
|---|---|---|---|
| log rel (plain rowid table, `lower.pl:698`) | 2,473,984 | 3,223,552 | **+30.3%**, 7.5 bytes/row |
| keyed set rel (PK table) | 2,203,648 | 2,957,312 | **+34.2%**, 7.5 bytes/row |

Q6b, the header's prove-or-refute: **PROVEN, updated_at rides the existing
write.** The shipped keyed arrival is one upsert
(`lower.pl set_arrival_sql_parts/4`); stamping is one more assignment in the
`DO UPDATE SET` list, with `excluded` carrying the incoming tick. Receipt:
insert at tick 7 then replace at tick 9, one statement each, yields
`1|second|7|9`.

Sabotage receipt attached, because the correct shape is one clause away from
the wrong one: listing `created_at_tick` in the `DO UPDATE SET` list destroys
the birth tick (7 becomes 11). The birth column must be write-once at the SQL
level.

The result that removes the need for the plane is T15: the same semantics are
already reachable as two ordinary edge rules.

```
rel thing(id: int, payload: text, created_at_tick: int, updated_at_tick: int) key(1).

thing(Id, Payload, Born, Tick) <+ arrive(Id, Payload), now(Tick),
                                  pre(thing(Id, _Old, Born, _Was)).
thing(Id, Payload, Tick, Tick) <+ arrive(Id, Payload), now(Tick),
                                  not(thing(Id, _P, _B, _U)).
```

Final row after three replaces: `thing(1, c, 1, 3)`. Created pinned, updated
advancing, zero new constructs.

T14 records the trap this would exist to close: the naive one-rule spelling
gives `thing(1, b, 2)`, a column named `created_at_tick` holding updated_at
semantics, silently. That is the honest argument FOR sugar, and it is an
ergonomics argument, the same class as the stream lab's card 1a.

**Rx lowering** (standing repo law, every snippet carries one):

```ts
const thing$ = arrive$.pipe(
  groupBy((arrival) => arrival.id),
  mergeMap((perId) =>
    perId.pipe(
      scan((prior, arrival) => ({
        id: arrival.id,
        payload: arrival.payload,
        createdAtTick: prior?.createdAtTick ?? arrival.tick,
        updatedAtTick: arrival.tick,
      }), undefined),
    )),
);
```

`pre/1` is `scan`'s accumulator, `not/1` is the `?? arrival.tick` base case. The
two rules are the two branches of one fold.

### Q7 historicization

**Verdict: a shadow history rel, +15.2%, and it needs no construct.**

Same script, 100k rows, 10% churn (10k rows replaced once):

| shape | bytes | vs current-only |
|---|---|---|
| (i) current only, no history | 2,445,312 | baseline |
| (ii) current + shadow history rel | 2,818,048 | **+15.2%** |
| (iii) rel-as-log + max-tick-per-key view | 4,214,784 | +72.4% |

As-of read plans, both indexed, count-test law satisfied:

```
(ii)  SEARCH cur_history USING INDEX cur_history_id (id=? AND from_tick<?)
(iii) SEARCH cur USING INDEX cur_id_tick (id=? AND at_tick<?)
```

(iii) is the pattern the channel thread already named (keep-until
`min(consumed.ordinal)`, the Kafka low-watermark). It costs nearly five times
what (ii) costs, because every version of every row stays in the live table and
the current-value read has to go through the max-tick view. (ii) keeps the hot
table at its current size and pays only for superseded versions.

(ii) is an ordinary rel the program declares and an ordinary edge rule that
writes the old row on replace, which is the update-arm verdict's shape
(`changed(K, Old, New) <+ finalize(r(K, Old)), r(K, New)`) with the pair widened
by two tick columns. No new construct, and no default: history is opt-in
because the program has to write the rule.

### Q8 does `seq(name)` become THE unified mechanism

**Verdict: no. Card 1b and unification stay separate, and 1b consumes the log
kind rather than replacing it.**

The stream lab's 1b example is
`stream(Name, _, Payload) <+ event(Name, Payload)`, whose trigger `event` must
itself be a log rel or the second identical arrival is gone (T1). So `seq(...)`
sugar is expansion over the ORDINAL-MINTING boilerplate on a rule-written head,
and it presupposes the log kind on the world-fed source. It cannot be the
mechanism that makes the log kind unnecessary.

Ranking, criteria visible:

| option | buys | costs |
|---|---|---|
| 1b `seq(name)` sugar alone | the four-rule ordinal chain becomes one line; base case cannot be written wrong | one column type; the minted cursor rel must be made boundary-invisible |
| unification alone | one store shape in the oracle | ~120 sites touched to delete 6; slot_seq_scope becomes a cross-target contract; no semantic gain |
| 1b + unification | nothing the two do not buy separately | both cost lists, and 1b still needs the log kind underneath |

### Q9 grading contract

**Verdict: 1 line versus 80 fixtures, and that ratio is the whole answer.**

- **Prototype (retention minus):** the tick log's SHAPE is unchanged; one
  existing tick line gains a `del` entry. Measured: 1 of 145 artifacts, listed
  in full under Q3.
- **Seq as a VISIBLE column:** every log rel's rows gain a column, so all 80
  log-carrying fixtures regrade. Worse than a content change:
  `ticklog.pl` sorts `add`/`del` lexicographically by each row's own JSON text,
  so adding an ordinal also reorders rows within every tick. The json_ticklog
  regrade precedent (244 artifacts, 12 changed) is not the right comparison;
  this would be closer to 80.
- **Seq as a boundary-INVISIBLE column:** tick logs are byte-unchanged, but
  then the column is unreadable by rules without a new read spelling
  (slot_metadata_read_spelling), and unreadable metadata does not serve the
  "historicalize them for easy versioning" half of the user's ask.

The third option is the one that looks free and is not: it buys byte-identity
by making the feature inaccessible.

---

## 2. Slots

| slot | status | measured reason |
|---|---|---|
| `slot_seq_scope` | **FILLED: per-rel, and the ORACLE is the side that must change** | T11: a set rel's arrival consumes a log rel's ordinal, so surfacing today's counter would give one log rel ordinals 1 and 3 with the gap minted by an unrelated rel. T12: the counter is shared between log rels (`zeta`=1, `alpha`=2, `zeta`=3), so per-rel numbering and the oracle's disagree on every interleaved tick. SQLite's per-table `rowid` already implements the recommendation, at
`lower.pl:2655` (`ORDER BY rowid DESC` inside the retention DELETE).

**Both citations this slot has been carrying are stale, corrected here.** The
header (`:77`) and the stream lab (`:317`, `:329`, `:349`, `:673`) cite
`engine.pl:356-358` and `lower.pl:2275`. `engine.pl:356-358` is the tail of
`check_occurrence_conflicts/2`; the global counter is `next_seq/3` at
`engine.pl:379-381`, plus the `Seq0` thread through `absorb_arrivals/8` at
`:291-313`, which is the half that makes set arrivals consume ordinals (T11)
and which neither citation covered. `lower.pl:2275` is a JSON path-segment
comment; the rowid ordering is at `:2655`. The stream lab recorded this as ungraded-because-unobservable; that is right for the graded tick log, which groups by rel, and wrong for `engine.pl`'s raw delta list, where `boundary_deltas/6` msorts by stamp across rels. Any surfacing makes it a cross-target contract. |
| `slot_updated_at_semantics` | **FILLED: keyed rels only** | T1. On an unkeyed set rel an identical re-arrival is a zero-delta and fires nothing; bumping `updated_at` would manufacture a delta from an arrival no rule can see, destroying the zero-delta property that R7 boundary diffing depends on. The header permitted this conclusion and the receipt supports it. |
| `slot_history_read_spelling` | **HANDED BACK, recommend refused-for-now** | Q7: the shadow history rel is +15.2% and its as-of read is an ordinary indexed join (`SEARCH ... USING INDEX`). A body word would wrap a join that one line already writes. Recommend no spelling until a program proves it needs one. |
| `slot_metadata_read_spelling` | **HANDED BACK with a recommendation: no spelling, because no hidden column** | T15 shows the columns can be ordinary declared columns the program fills with `now/1`. Ordinary columns need no read spelling and cost no regrade, because they exist only in programs that declare them. The alternative (auto columns, hidden, plus a read word) costs a new mechanism and either an 80-fixture regrade or inaccessibility (Q9). |

---

## 3. What should actually happen

Ranked, with the receipt each rests on. No fiat; the third is a real user call.

1. **Take the retention minus.** 51 patch lines, two files, one artifact line
   regrades, 0 wrong on the sweep, and it closes a hole three arcs filed
   (`SLOT-LOG-FINALIZE-REFUSAL`, stream-lab card 4, consumption-arms 17) plus
   the standing "retention-grading gap" class the ledger names four times. It
   also removes the reason for a proposed refusal, which is a construct-budget
   saving. Patch: `v6/prolog/labs/time_plane/retention_minus.patch`. Needs a
   fail-first fixture and the R7 restatement written into TICK-MODEL.md
   section 5 before it lands.
2. **Do not unify the planes.** Q1 refutes the sugar reading, Q5 prices the
   storage reading at ~120 sites touched to delete 6, and Q9 prices the visible
   half at 80 fixtures. Nothing in H1 buys a semantic the language lacks.
3. **`created_at` / `updated_at` sugar is an open ergonomics call, not a plane.**
   T15 says the semantics ship today; T14 says the naive spelling is silently
   wrong. If sugar is wanted it is the stream lab's card 1b shape (one expansion
   module, enum/match precedent) over TWO rules, and it should be opt-in per
   rel, never automatic: automatic costs 7.5 bytes/row on every rel (Q6a) to
   serve the rels that asked for it.
4. **Coverage finding, unowned.** One fixture in 209 exercises
   `keep(count(N))`. Retention is close to untested, which is why the regrade
   was one line and why the finalize hole survived three arcs. Worth two or
   three fixtures regardless of what happens to the rest of this verdict.

---

## 4. Fixture promotion candidates

Distilled for the coordinator. All four run on the shipped engine except where
marked.

| candidate | shape | grades |
|---|---|---|
| `set_rel_identical_arrival_is_one_occurrence` | T1 | the arrival-boundary coalescing that no rule can see. Currently pinned nowhere. |
| `log_rel_identical_arrival_is_two_occurrences` | T2 | its twin; together they are the log/set distinction as a graded pair. |
| `created_at_pinned_updated_at_advances` | T15 | the two-rule metadata idiom, so a future sugar has an oracle to match. |
| `retention_prune_is_a_visible_minus` | receipt (p) | **requires the prototype.** Fail-first today; the fixture is the gate for item 1 above. |

---

## 5. Receipts index

Hermetic: `SPREFA_CONFIG=/nonexistent/time-plane.toml`, `DL_NO_DAEMON=1`,
ephemeral ports, `:memory:` servers, every measurement db under `mktemp`.
Nothing read or wrote `~/.local/state` and no daemon was contacted. The
prototype patch is applied and reverted inside `receipts.sh`, on every exit
path including interrupt.

| # | claim | how |
|---|---|---|
| T1 | a set rel's identical re-arrival is one occurrence | `0_receipts.pl`, real engine |
| T2 | a log rel's identical re-arrival is two | same |
| T3 | a set rel with distinct rows is also two | same |
| T4 | `keep(count)` prunes and reports no minus | same |
| T5 | `finalize` over a pruning log fires nothing | same |
| T6-T10 | the five refusals throw | same |
| T11 | a set arrival consumes a log rel's ordinal | same, direct `absorb_arrivals/8` call |
| T12 | the arrival counter is shared between log rels | same |
| T13 | identical wire rows, one row vs two by decl alone | same |
| T14 | the naive one-rule `created_at` is really `updated_at` | same |
| T15 | `created_at` + `updated_at` ship today, zero new constructs | same |
| (patch) | `retention_minus.patch` applies to the shipped tree | `receipts.sh` leg 2 |
| (conf) | conformance 209 -> 209, 0 fail under the prototype | same |
| (plunit) | plunit green under the prototype | same |
| (p) | the prune is a visible minus and `finalize` fires, both doors | `receipts.sh` leg 3, byte-diffed per rel |
| (sweep) | 145 total, 143 identical, 0 wrong, 1 artifact line regraded | `bash v6/tsv2/scripts/sweep.sh` under the patch |
| Q6a | +30.3% / +34.2%, 7.5 bytes/row at 100k | `metadata_cost.sh`, real sqlite3 |
| Q6b | `updated_at` rides the existing upsert; sabotage on `created_at` | same |
| Q7 | history shapes at +15.2% vs +72.4%, both reads indexed | same |
| (s) | widening `keep(count(2)->3)` changes the graded log | `receipts.sh` leg 5 |

### Prior work cited rather than re-derived

| source | what was taken |
|---|---|
| `plans/2026-07-30-rel-as-stream-lab.md` | R12 (prune invisible), receipt (d) (eviction observable one hop downstream), card 1b's `seq(name)` shape, card 4's three options, the two-door harness and its per-rel normalization |
| `plans/2026-07-28-consumption-arms-verdict.md` | assertion 17, the `s1`-`s4` retention pricing |
| `plans/2026-07-29-update-arm-verdict.md` | `SLOT-LOG-FINALIZE-REFUSAL`, the OLD/NEW pair shape reused in Q7 |
| `v6/prolog/compile/TICK-MODEL.md` | the five theorems, section 5's ring-error table |
| `plans/2026-07-28-types-as-rels-verdict.md` | the boundary-invisible storage-plane precedent (dictionaries, frontier-TEMP class) |
