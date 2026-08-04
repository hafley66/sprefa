# rxprim fused contract: `one()` as a rel-declaration property, `any`/`one` block lowering, typed merge

Lane: `sprefa-lab-fuse`, worktree base `3d97fd4f`. PLAN ONLY, docs only, no
code edits, no commits. This contract fuses three prior lanes onto the opus
spine, per `plans/2026-08-04-rxprim-duel-verdict.md`:

- BASE (spine): `~/projects/sprefa-plan-rxprim-opus/PLAN.md`
- GRAFT A (kimi): `~/projects/sprefa-plan-rxprim-kimi/PLAN.md`
- GRAFT B (flash): `~/projects/sprefa-plan-rxprim-flash/PLAN.md`

Grafts keep their source's numbers and receipts, cited as `(kimi PLAN.md §N)`
or `(flash PLAN.md §N)`. The spine's own numbers are cited as `(opus §N)`.

Two new rulings landed after the duel and bind the whole document here; they
are quoted in full in section 1 and applied as rewrites everywhere the spine
or a graft conflicts with them: `admission_word` (the reserved door becomes
lossless queued admission, concat-family spelling, reject `throttle` and
`zip` as words) and `block_lowering_first` (the block's lowering is the
construct; braces are a later sugar wave).

## TOC

1. [Ruled ground](#1-ruled-ground)
2. [What ships, in three stages](#2-what-ships-in-three-stages)
3. [Preamble: reconcile table](#3-preamble-reconcile-table)
4. [Measured receipts](#4-measured-receipts)
5. [Grammar anchors](#5-grammar-anchors)
6. [S1: the `one(Positions)` rel-declaration property](#6-s1-the-onepositions-rel-declaration-property)
7. [S2: the merge block, lowering is the construct](#7-s2-the-merge-block-lowering-is-the-construct)
8. [S3: typed merge via the enum tag column](#8-s3-typed-merge-via-the-enum-tag-column)
9. [Conformance fixtures and COUNT tests](#9-conformance-fixtures-and-count-tests)
10. [What is not built](#10-what-is-not-built)
11. [Landing sites](#11-landing-sites)
12. [Open questions](#12-open-questions)
13. [Marble type lattices](#13-marble-type-lattices)
14. [Discards from the duel](#14-discards-from-the-duel)
15. [Verdict recap](#15-verdict-recap)

---

## 1. Ruled ground

Three rulings bind the shape of this contract. Quoted verbatim from
`v6/prolog/conformance/rulings.pl`, the last three entries.

```prolog
% 2026-08-04 morning: the tick boundary, ruled after the v8/event-loop survey
% (macrotask = tick, microtask drain = rounds, boundary = queue exhaustion;
% v8 owns no clock and neither do we).
ruling(tick_boundary, ingress_transaction_list, user,
       'user 2026-08-04: "make the events that are in-tick be a list of events, and its usually a list of one". A tick dequeues ONE ingress transaction = an explicit list of events, list of one in the common case. Simultaneity is opt-in: it exists only when the submitter deliberately batched (one file save, one commit, one schedule row), never manufactured by the engine coalescing independent sources into a shared tick. Consequences: same-tick multi-writer conflicts shrink to refereeing DELIBERATE batches (the one/any family scope-cut); independent contenders land on successive ticks automatically (the deferral door happens by construction); the engine surface already matches (submit takes IArrivalBatch, concatMap runs one batch per tick, 3_engine.ts:104); the constraint binds every future ingress path (live_event, bus, clock binds): one submission = one tick, no auto-coalescing.').

% 2026-08-04 midday: the duel words, ruled. Ends the throttle-vs-zip fork
% (plans/2026-08-04-rxprim-duel-verdict.md word 1).
ruling(admission_word, lossless_queue_concat_family, user,
       'user 2026-08-04: "no dropping events, exhaustMap is not what i want, this is concatMap territory, idk why its zip but dont lose info". The reserved admission door = LOSSLESS QUEUED admission: one admission per key per tick, remaining contenders WAIT for successive ticks, nothing is dropped. Drop-flavored spellings (throttle, exhaust) are REJECTED for this construct; zip is rejected as the WORD while its lockstep semantics survive; the surface spelling comes from the rx concat family, exact form priced in the fuse contract. one_pick_order (within-tick pick = arrival order) is untouched: it referees who is FIRST in a deliberate batch, this ruling says the rest queue instead of vanishing.').

% 2026-08-04 midday: block sugar timing, ruled (duel word 2). The lowered form
% is the construct; braces come later as sugar over it.
ruling(block_lowering_first, flat_rels_catalog_edges_arg_distribution, user,
       'user 2026-08-04: "if a file is our first block syntax its not really sugar anymore... make a middle of the road abstraction that we open later... relate rels to each other after we lower them into longer names and if we capture arg from outside world its implicitly captured, distribute that arg into every thing, that sounds sugarable". Block construct v1 = the LOWERING: children land as flat rels with long mangled names (module-catalog M5 spelling) plus catalog rows relating them; an outer arg the block captures is IMPLICITLY DISTRIBUTED into every child rel as a leading demand-key column (module-catalog M1, data-driven scalar args). The brace surface is sugar over that lowering and arrives in a later wave; a FILE is the degenerate first block already. Consistent with modscope decisions 7 (module = rel/0 with children), 8 (dotted heads contribute), 10 (block-under-rel = extension surface).').
```

Application of these two rulings inside this contract:

- `throttle(...)` and `zip(perKey, ticks$)` no longer appear as live spellings.
  The reserved door is lossless queued admission; its rx lowering is
  concatMap-shaped and its exact surface spelling is an OPEN pricing question
  (section 12). The historic names appear only in the verdict recap, section 15.
- Every previous block-sugar wording is rewritten so the LOWERING is the
  construct (flat mangled rels + catalog rows + captured-arg distribution) and
  the brace/arm surface is the later sugar wave (section 7).

---

## 2. What ships, in three stages

| stage | construct | new semantics | gate |
| --- | --- | --- | --- |
| S1 | `one(Positions)` rel-declaration property | first-wins admission, arrival order per tick, both doors | conformance + plunit + text-door |
| S2 | merge block lowering (`any`/`one`), flat rels + catalog rows + captured-arg distribution, brace surface later | the lowered flat-rels form is the construct; today's arms are the spelling | plunit desugar equality + golden-flex |
| S3 | enum name in tag-column type position | checker only, no run-time diff | plunit + golden-flex |

S1 is independent and lands first. S2 gives the `one(Positions)` program a
block-shaped surface whose construct is its lowering. S3 is a checker widening
over the enum tag column S2 writes.

Three rulings drive the shape: `one_decl_surface` (rulings.pl:585) makes the
construct a rel-declaration property beside `key(1)` and `log keep()`;
`one_pick_order` (rulings.pl:579) makes the pick read the arrival index on both
doors; `one_admission_no_lockout` (rulings.pl:582) keeps first-wins and the
takeover door both sound.

Standing note honoured: `one(Positions)` is orthogonal to `kind`, so it works
on a log head and on a keyed head without picking a side. It does not deepen
the keyed-vs-log split that rulings.pl:586 says will be revisited.

---

## 3. Preamble: reconcile table

Grafted from `(flash PLAN.md, the directive-sketch reconcile)`. The directive
carried three invented shapes; each is measured against rulings and the real
grammar before the contract keeps or drops it.

| directive sketch | verdict | where it lands |
| --- | --- | --- |
| `any { arms }` / `one { arms }` brace block | brace form is a later sugar wave per `block_lowering_first`; the construct is the lowering | the merge block lowering (section 7); braces come later |
| `enum gate_source = pre_commit \| timer.` keyword enum | dropped; the real enum is a rel decl with semicolon variant disjunction | typed merge uses the real enum decl plus a tag column |
| `one` via bounded `log keep(count(1))` | rejected; retention prunes at tick end and the survivor would be the last arm | `one` is its own property, never bounded-log retention |
| `one` via cross-rule negation guards | dropped as the mechanism; both doors referee it differently by the COMPOSE race | `one` adds no negation, no `edge_absence`, no cross-rule negation; arbitration is a fold on arrival index |

What survives, restated:

- `any` is merge. It is already expressible as N edge arms on one unbounded log
  head, all rows land same tick, nothing arbitrates. A typed variant adds a tag
  column fed from an enum. `any` needs zero new surface to be expressible.
- `one` is per-tick arrival-index arbitration over the arms of one head, made
  loud by a rel-declaration property. Two doors must agree on arrival index
  (`one_pick_order`). The fold keeps first-wins sound and leaves the takeover
  door open (`one_admission_no_lockout`).

---

## 4. Measured receipts

### R1. The `one()` shape adds zero clock dependencies and zero boundaries

Program A = two arms, two triggers, no guard (what `one(1)` will decorate).
Program B = today's attempt 3, the negation guard
(conformance/fixtures/one_vs_any.pl:80-89).

```
TWO ARMS, NO GUARD (the one() decl shape):
  dependency(rule(1,edge,dispatch_first/2),dispatch_ack/1,dispatch_first/2,z,n,positive,0,trigger)
  dependency(rule(2,edge,dispatch_first/2),dispatch_seal/1,dispatch_first/2,z,n,positive,0,trigger)
  CLOCK: PASSES
  BOUNDARIES: []

TWO ARMS, NEGATION GUARD (today attempt 3):
  dependency(rule(1,edge,dispatch_first/2),dispatch_ack/1,dispatch_first/2,z,n,positive,0,trigger)
  dependency(rule(1,edge,dispatch_first/2),dispatch_first/2,dispatch_first/2,b,n,negative,0,edge_absence)
  dependency(rule(2,edge,dispatch_first/2),dispatch_first/2,dispatch_first/2,b,n,negative,0,edge_absence)
  dependency(rule(2,edge,dispatch_first/2),dispatch_seal/1,dispatch_first/2,z,n,positive,0,trigger)
  CLOCK: PASSES
  BOUNDARIES: [not_provable(arm_absence_batch_invariance(rule(1,edge,dispatch_first/2),dispatch_first/2)),
               not_provable(arm_absence_batch_invariance(rule(2,edge,dispatch_first/2),dispatch_first/2))]
```

Moving the arbitration from the body to the declaration deletes two
`edge_absence` edges and two `not_provable` boundaries. This is the whole clock
argument for the design, measured. `(opus R1)`

### R2. A merge head whose arms sit at different grades from one origin is refused today

```dl
rel armed(repo: text) log keep(all).
rel gate_fire(repo: text) log keep(all).
armed(Repo)     <+ pre_commit(Repo).
gate_fire(Repo) <+ pre_commit(Repo).
gate_fire(Repo) <+ armed(Repo).
```

rx lowering of the intent:
`merge(preCommit$, armed$).pipe(map((row) => ({ repo: row.repo })))`.

```
  dependency(rule(1,edge,armed/1),pre_commit/1,armed/1,z,n,positive,0,trigger)
  dependency(rule(2,edge,gate_fire/1),pre_commit/1,gate_fire/1,z,n,positive,0,trigger)
  dependency(rule(3,edge,gate_fire/1),armed/1,gate_fire/1,z,n,positive,1,trigger)
CLOCK REFUSAL: unsupported_construct(clock_path_conflict(pre_commit/1,gate_fire/1,0,1))
CLOCKS: [pre_commit/1-pre_commit/1-0, armed/1-pre_commit/1-0,
         gate_fire/1-pre_commit/1-1, gate_fire/1-pre_commit/1-0]
```

A merge head must be clock-flat: every arm reaches the head at one offset per
origin. `any` and `one` inherit that constraint unchanged and add nothing to it.
`(opus R2)`

### R3. An enum name in column-type position is refused today

```
PARSED: prog([enum_decl(gate_source,(pre_commit;timer)),
              col_type(gate_fire/2,source,gate_source),
              col_type(gate_fire/2,repo,text)],[])
findings=[]
VIOLATION: column_type_unknown gate_source
```

Source text: `rel gate_source(pre_commit() ; timer()).` then
`rel gate_fire(source: gate_source, repo: text).`. The refusal comes from
`0_program_check.pl:164-170` by way of `0_type_plane.pl:126-128`, because
`declared_type_name/2` reads `type_decl/2` entries only and enum expansion mints
none. S3 exists to close this. `(opus R3)`

### R4. Nullary enum variants parse and expand today

`rel gate_source(pre_commit() ; timer()).` parses to
`enum_decl(gate_source,(pre_commit;timer))` and expansion yields:

```
prog([col_type(gate_source_pre_commit/1,id,int), keyed(gate_source_pre_commit/1,[]),
      col_type(gate_source_timer/1,id,int),      keyed(gate_source_timer/1,[]),
      col_type(gate_source_tag/2,id,int),        col_type(gate_source_tag/2,tag,text)],
     [ (gate_source_tag(Id,pre_commit) <- gate_source_pre_commit(Id)),
       (gate_source_tag(Id,timer)      <- gate_source_timer(Id)) ])
```

Two consequences S3 must own: the tag column's storage is `text`, and a
tag-only enum mints one dead variant rel per variant carrying `keyed(Ref, [])`.
Suppressing those rels changes enum expansion, so it is filed as a user
question (section 12) instead of taken inside this lane. `(opus R4)`

### R5. `one(1)` is a parse error today, `log keep(all) key(1)` is not

`rel gate_fire(source: text, repo: text) key(1) one(1).` gives a parse error at
the `one(` position. `log keep(all) key(1)` parses clean. Decl modifiers parse
permissively and refuse by name, which is the slot `one(Positions)` enters.
`(opus R5)`

---

## 5. Grammar anchors

Verified before any snippet in this contract was written.

| anchor | file:line | what it fixes |
| --- | --- | --- |
| enum decl = rel with semicolon variant disjunction | `compile/test/plunit_tests.pl:1118-1125` | no `enum` keyword exists |
| match block = paren block, `; guard ARROW head` arms | `v6/dl/fixtures/golden-flex.dl6:461-465`, `527-531` | braces are wrong; parens and leading `;` |
| arm arrows `\|->` level and `\|+>` edge | `golden-flex.dl6:469-471`, ruling `match_arm_tokens` (rulings.pl:423) | left-to-right reading is the ruled reason |
| decl modifiers, any subset in any order | `compile/parse_dl.pl:596-608` | `one(...)` is one more clause |
| `files(glob)` worktree feed | `v6/dl/fixtures/files-hosts.dl6:11`, `50` | the word for enumeration |

Every dl snippet below carries its pure-rxjs lowering.

---

## 6. S1: the `one(Positions)` rel-declaration property

### 6a. The rel-declaration spelling

```dl
rel dispatch_first(dispatch_id: int, note_tag: text) log keep(all) one(1).
dispatch_first(DispatchId, 'acked')  <+ dispatch_ack(DispatchId).
dispatch_first(SealedId, 'sealed')   <+ dispatch_seal(SealedId).
```

rx lowering:

```ts
const dispatchFirst$ = merge(
  dispatchAck$.pipe(map((row) => ({ dispatchId: row.dispatchId, noteTag: "acked" }))),
  dispatchSeal$.pipe(map((row) => ({ dispatchId: row.dispatchId, noteTag: "sealed" }))),
).pipe(
  groupBy((row) => row.dispatchId),
  mergeMap((group) => group.pipe(take(1))),
);
```

`one(Positions)` names the ADMISSION KEY: the column positions that identify one
contest. At most one row per admission key is ever admitted, and the first
occurrence to derive one wins. Term form in `Decls` is `one(Ref, Positions)`,
the exact shape of `keyed(Ref, Positions)`.

Parser and reader land as `(opus §4a)`: one clause beside `key_clause/3`
(compile/parse_dl.pl:727-730) reached from `decl_a_modifiers/4` (:596-608);
`declared_admission/3` beside `declared_key/3` at 0_program_check.pl:56; the
twin accessors `analyze:decl_one/3` and `engine:decl_one/3` mirror
`decl_key/3` and get the same cross-door agreement test the key already has.

`merge`, never `concat`, which is the operator pair rulings.pl:580 names. The
group is never torn down, so `take(1)` closes the key for the life of the
program: state equals history. `(opus §4b)`

Interaction with every existing decl checker, stated rather than discovered:

| pairing | verdict | reason | checker home |
| --- | --- | --- | --- |
| `one(K)` + `log keep(all)` | legal, the canonical shape | one row per key ever, so the log is the state; the log-vs-key vacuous case | n/a |
| `one(K)` + `log keep(count(N))` | REFUSE `one_with_bounded_retention(Ref, count(N))` | retention prunes at tick end and would re-open an admission the program declared closed | 0_program_check.pl, beside `retention_head_conflict_risk` (:449) |
| `one(K)` + `key(K)` | legal, same positions | first-wins replaces last-write-wins on the same key | n/a |
| `one(K1)` + `key(K2)`, K1 \== K2 | REFUSE `one_key_positions_mismatch(Ref, K1, K2)` | two different folds on one head is under-specified | 0_program_check.pl, beside `keyed_log_rel` (:109) |
| `one(K)` on a level-headed rel | REFUSE `one_on_level_headed_rel(Ref)` | admission is an edge-write concept, the twin of `keyed_level_head` | 0_program_check.pl:102-104 pattern |
| `one(K)`, two arms sharing a trigger ref | REFUSE `one_head_conflict_risk(Ref, SharedTriggerRefs)` | one occurrence, two candidate rows, arrival order gives no answer | analyze.pl:1341-1353, widen `decl_key(Decls, HeadRef, _)` to `decl_key ; decl_one` |
| `one(K)` + duplicate positions in K | REFUSE, existing class | the duplicate-position clause generalizes to `one/2` | 0_program_check.pl:95 |

### 6b. Clock-checker treatment

Per arm, per role, from R1's measurement `(opus §4c)`:

| arm shape | role | ReadRing | WriteRing | Sign | Grade | causal? |
| --- | --- | --- | --- | --- | --- | --- |
| plain trigger atom, ref not edge-headed | `trigger` | z | n or b | positive | 0 | yes |
| plain trigger atom, ref edge-headed | `trigger` | z | n or b | positive | 1 | yes |
| `latest(Atom)` sample in the arm | `edge_sample` | b | n or b | state | 0 | no, constrains only |
| `not(Atom)` in the arm | `edge_absence` | b | n or b | negative | 0 | no |
| `finalize(Atom)` trigger | `edge_departure` | z | n or b | negative | 1 | yes |

`one(Positions)` contributes NO row to this table. It adds no dependency edge,
requires no new role, and needs no new `clock_role/4` fact. 3_clock_check.pl is
untouched by S1. New boundary: NONE, because the spelling carries no
`not(Head)` goal; the `arm_absence_batch_invariance` boundary never fires. The
design does not need a replacement boundary, stated as the claim an auditor
should attack `(opus §4c)`:

> First-wins admission is BATCH-INVARIANT under a stable arrival order. Split
> one batch into two ticks anywhere, preserving order, and the winner is the
> same row, because the winner is the first element of the merged sequence and
> tick boundaries do not reorder that sequence.

The reserved takeover door does NOT have this property (section 10), which is
the technical reason first-wins ships now.

Where the emitter stops consulting source arm order (the `one_pick_order`
obligation) `(opus §4c)`:

1. `lower.pl:1560-1566`, `edge_statement_single/8`. Widen `TriggerKind` to
   `ordered_arrival` when the arm carries `pre` atoms OR the head carries
   `one(_)`.
2. That flips the head onto the emitter's EXISTING ordered occurrence loop
   (`emit_ts.pl:1240-1246`, `:1526-1530`, `:1485-1520`), which is
   occurrence-major and arm-minor. The default path is arm-major and
   occurrence-minor (:902-912), the concat side COMPOSE.md measured.
3. Same widening for the LEGACY negation spelling, which is exactly the shape
   `arm_absence_batch_invariance` already names. `one_admission_no_lockout`
   forbids refusing that spelling.

Oracle side needs no ordering change: `process_occurrences_/9`
(conformance/engine.pl:378-395) already walks occurrences in list order. It
needs the admission probe only (6d).

### 6c. Where the tie is refused, and why it must be

conformance/engine.pl:361-366 builds the arm list with
`findall(rule(Head,Body,Items), member((Head <+ Body), Rules), Edges)`, so
within ONE occurrence the derived rows arrive in SOURCE ARM ORDER. Any design
that lets two arms admit from one occurrence therefore reintroduces source arm
order as the tiebreak. Two refusals keep that unreachable `(opus §4d)`:

- compile time: `one_head_conflict_risk(HeadRef, SharedTriggerRefs)`, the
  `edge_head_conflict_risk` sibling (analyze.pl:1334-1353), fires when two arms
  on a `one()` head share a trigger ref.
- run time: `one_admission_tie(HeadRef, Key, Rows)`, the `keyed_conflict/3`
  sibling (conformance/engine.pl:397-403), fires when ONE occurrence derives
  two different rows for one admission key through a single arm's join.
  Identical rows dedupe and never throw.

### 6d. Lowering shape, and the N+1 it must not become

The admission probe folds into the arm's own project SQL as one more
`NOT EXISTS` conjunct, reusing `compile_negative_uses/5` (lower.pl:386-403)
with the admission key columns as the correlation:

```sql
NOT EXISTS (SELECT 1 FROM "dispatch_first" a0 WHERE a0."dispatch_id" = ?1)
```

No extra statement per occurrence, so the emitted statement COUNT is identical
to the same program without `one()`. That is a COUNT test, section 9. The keyed
grouping and tie throw ride the ordered loop's existing per-occurrence Map
(emit_ts.pl:1496-1510); the admission branch is a second Map keyed on the
`one()` positions. `(opus §4e)`

### 6e. Oracle landing

`apply_edge_writes/6` (conformance/engine.pl:405-423) currently appends to a
log head unconditionally. The admission probe goes in front of that branch: if
`decl_one(Decls, Ref, Positions)` holds and a row with that key is already in
`Store0`, the write is skipped and the row does not enter `Written`, so it
never becomes a boundary delta and never becomes a T+1 carry occurrence.
`(opus §4f)`

---

## 7. S2: the merge block, lowering is the construct

Bound by `block_lowering_first`: the construct is the LOWERING, not the block
syntax. Children land as flat rels with long mangled names plus catalog rows
relating them; any outer arg the block captures is implicitly distributed into
every child rel as a leading key column. The brace surface is sugar over
that lowering and arrives in a later wave. A file is the degenerate first
block.

Grafted content from `(kimi PLAN.md §3a, §4a)` with the ruling rewrite applied.
The desugar discipline is kimi's: expansion to ordinary rels and rules runs in
a new `0_merge_expand.pl` beside `0_match_expand.pl`, before analyze, strat,
lower and the clock checker, so every downstream pass sees ordinary rules.

Today's spelling of the construct is the flat arms described in section 6
(`one`) and by `any`'s N-arm merge; the block form that wraps them is the later
sugar wave. What the contract fixes now is the lowering shape and the checks a
future block must run.

The lowering shape, per `block_lowering_first`:

- flat rels with long mangled names (module-catalog M5 spelling),
- catalog rows relating those rels,
- a captured outer arg implicitly distributed into every child as a leading
  key column (module-catalog M1, data-driven scalar args).

The grammar delta against `match`, kept verbatim from `(kimi PLAN.md §3a)`:

> The one grammar delta against match: match has a shared scrutinee atom in the
> block header and puts only guards in the arm's first slot
> (`0_match_expand.pl:expand_match_arm/3` writes `ArmHead <- SourceAtom,
> Guards`); the merge block has no shared scrutinee, because each arm reads its
> own source, so the arm's first slot holds the arm's whole body conjunction.
> The arrow keeps its ratified left-to-right reading (ruling match_arm_tokens,
> rulings.pl:422-424): source first, head after the arrow.

The block-vs-decl validation a future block must carry, from `(opus §5a)` and
`(kimi PLAN.md §3a/§4a)`:

- every arm heads the SAME ref, else `merge_block_heads_differ(Refs)`;
- the block word agrees with the head's declaration, else
  `merge_block_policy_mismatch(HeadRef, BlockWord, DeclWord)`: `one (...)` on a
  `one(K)` decl, `any (...)` on a decl without it. This keeps
  `one_decl_surface` intact: arbitration lives in the declaration and the block
  only asserts what the declaration says.
- one policy per head: `merge_policy_conflict(Ref)` if a decl carries both
  `any` and `one` `(kimi PLAN.md §3a)`.

The pulse spelling of the ruled kind `(opus §5a, kimi PLAN.md §4a)`: the
canonical merge-of-pulses arm is an edge arm with an explicit `latest/1` sample
of the latch beside its trigger, `pulse_merge_spelling` (rulings.pl:558).

The `one (...) ` block's rx lowering is section 6's unchanged: the arms provide
the `merge(...)` grouping and the `one()` decl provides `groupBy` plus
`take(1)`. `(opus §5b)`

### Registry, printer and reserved-word fallout `(opus §5d)`

- compile/registry.pl gains two rows beside `surface(match/2, sugar, no_refs,
  block(match_arms), live)` at :189:
  `surface(any/1, sugar, no_refs, block(merge_arms), live)` and
  `surface(one/1, sugar, no_refs, block(merge_arms), live)`.
- The registry row IS the name reservation: `reserved_body_word`
  (0_program_check.pl:424-430) projects from `surface_for_term/6`, so a program
  with a rel actually named `any/1` or `one/1` gets a named refusal.
- print_dl.pl needs the block printer beside the match block, and the modifier
  lists at :228-230, :303-305, :313-316 need `one(Ref, Positions)` so the
  `=@=` round-trip stays exact.

### Interaction with `match` `(opus §5e)`

`match` and the merge block are duals and both stay: `match` shares the SOURCE
atom, the merge block shares the HEAD ref. A `match` block whose arms all head
one `one()` rel is legal and means the same as a `one` block; because
`validate_match_source/1` forces one shared source, all arms share a trigger
ref, and `one_head_conflict_risk` refuses it at compile time. That is correct:
`one` over a single scrutinee is a per-value pick, spelled `match` with
disjoint guards, needing no arbitration.

---

## 8. S3: typed merge via the enum tag column

R3 measured that an enum name in tag-column type position is
`column_type_unknown(gate_source)` today. Four checker edits close it, all
read-side `(opus §6a)`:

1. 0_type_plane.pl:126 gains an enum branch returning storage `text`, the
   storage the `<enum>_tag` already declares for its own tag column.
2. The enum name set reaches the type plane as a CONTEXT parameter computed
   from surface decls before expansion erases them; reuse `enum_context/2`.
3. 0_program_check.pl:164-170 `column_type_unknown` stops firing for enum
   names, from the same context.
4. New refusal `enum_tag_not_a_variant(EnumName, Tag)` when an arm writes a
   constant into an enum-typed column that is not a declared variant.

The tag is a string union at the type level and a TEXT column at the storage
level, so S3 is a checker stage with an empty run-time diff. The emitted
module's bytes for the S2 program and the S3 program are identical; that
identity is the acceptance receipt. `(opus §6b)`

Exhaustiveness, write side only `(opus §6c)`: a block writing into an
enum-typed tag column gets the mirror of `validate_match_coverage/2`: every
variant must be written by some arm, else `merge_block_tag_nonexhaustive`.
READ-side exhaustiveness (a `match` over the tag column proving guard coverage)
is NOT built here; today's guards are text comparisons and recognising them as
variant tests is a guard-analysis arc with its own cost (section 10).

Clock treatment: none. S3 touches the type plane and the program checker; the
dependency set, roles and grades are untouched. `(opus §6d)`

```dl
rel gate_source(pre_commit() ; timer()).
rel gate_fire(source: gate_source, repo: text, bucket: int) log keep(all).
gate_fire('pre_commit', Repo, Bucket) <+ pre_commit(Repo, Bucket).
gate_fire('timer', Repo, Bucket)      <+ latest(armed(Repo)), interval(1, Bucket).
```

rx lowering:

```ts
const gateFire$ = merge(
  preCommit$.pipe(map((row) => ({ source: "pre_commit" as GateSource, ...row }))),
  timerArm$.pipe(map((row) => ({ source: "timer" as GateSource, ...row }))),
);
```

---

## 9. Conformance fixtures and COUNT tests

Format per conformance/FIXTURES.md: `fixture(Name, prog(Decls, Rules),
InitialRows, Schedule, Expectations)`. FIXTURES.md's Decls list must gain
`one(Ref, Positions)` as part of S1, or every fixture below is unwritable.

### 9a. The four `one_vs_any.pl` fixtures, dispositioned `(opus §7a)`

| fixture | disposition |
| --- | --- |
| `any_two_tagged_arms_land_on_one_tick` (:37) | UNCHANGED. It is the `any` receipt; S1 does not touch it. S2 adds a plunit desugar-equality test, which lives in plunit_tests.pl because FIXTURES.md forbids desugar equality as a fixture. |
| `one_attempt_keyed_head_loses_the_first_arm_silently` (:53) | UNCHANGED. Keyed last-write-wins stays legal and silent; `one(1)` is the opt-in. The header comment gains one line pointing at the new fixture set. |
| `one_attempt_bounded_log_two_arms_refused` (:66) | UNCHANGED expectation, `throws(retention_head_conflict_risk(dispatch_first/2, count(1)))`. The refusal MESSAGE gains the `one(1)` rewrite. |
| `one_attempt_guard_by_negation_lands_one_unnamed_winner` (:80) | UNCHANGED expectation, REGRADED in status. After the 6b step-3 widening both doors answer by arrival order, so its disagreeing twin is added as F5. Header comment becomes "the legacy spelling, still legal per one_admission_no_lockout". |

Grafted safety receipt `(kimi PLAN.md §6.2)`: rewriting the keyed-attempt
fixture is safe whenever a later wave does so, because the un-`one` keyed shape
keeps today's last-write-wins behavior pinned by
`key_same_tick_ordered_not_conflict` (merge_family.pl:74-80). Opus keeps that
fixture unchanged in this wave; the receipt is recorded so the change is
reversible and auditable later.

### 9b. New fixtures `(opus §7b)`, F1 to F13

Program P (used by F1 to F4, F9):

```dl
rel dispatch_first(dispatch_id: int, note_tag: text) log keep(all) one(1).
dispatch_first(DispatchId, 'acked')  <+ dispatch_ack(DispatchId).
dispatch_first(SealedId, 'sealed')   <+ dispatch_seal(SealedId).
```

rx: `merge(ackArm$, sealArm$).pipe(groupBy(byDispatchId), mergeMap((group) => group.pipe(take(1))))`.

**F1 `one_arrival_order_decides_ack_first`**: arms ack, seal; arrivals ack,
seal. Winner `acked`.
**F2 `one_arrival_order_decides_seal_first`**: same arm order, arrivals sealed,
ack. Winner `sealed`.
**F3 `one_arm_order_reversed_ack_first`**: arms seal, ack; arrivals ack, seal.
Winner `acked`, identical to F1.
**F4 `one_arm_order_reversed_seal_first`**: arms seal, ack; arrivals seal, ack.
Winner `sealed`, identical to F2.

F1 to F4 ARE COMPOSE.md's four-run race table; the claim is that all four now
grade on both doors with the arrival answer.

**F5 `negation_guard_arms_pick_by_arrival_on_both_doors`**: the legacy spelling
with the DISAGREEING order. Gradeable only after the 6b step-3 widening; the
sabotage receipt for `one_pick_order`.

**F6 `one_admission_closes_the_key_across_ticks`**: schedule
`[[+dispatch_ack(1)], [+dispatch_seal(1)]]`. State-equals-history receipt: the
admission survives the tick boundary.

**F7 `one_admission_is_per_key_not_global`**: one batch,
`[+dispatch_ack(1), +dispatch_seal(1), +dispatch_seal(2), +dispatch_ack(2)]`.
Expect `[+dispatch_first(1,acked), +dispatch_first(2,sealed)]`. Guards against
an implementation that probes existence of ANY row instead of the key's row.

**F8 `one_batch_split_gives_the_same_winner`**: F7's sequence delivered as four
single-arrival ticks. Batch-invariance executed rather than asserted.

**F9 `one_pick_order_holds_across_carry_and_arrival`**: a program where the
head has one arm triggered by an arrival rel and one by a rel written on the
previous tick, so tick 2 carries a carry occurrence AND an outside arrival. The
carry occurrence is processed first, so the carry arm wins. Grades the pick-key
definition: position in the occurrence list, never the numeric stamp (2000 for
carry, 1 for the outside arrival).

**F10 `one_head_conflict_risk_shared_trigger_refused`**: two arms on a `one(1)`
head both triggered by `dispatch_ping/1`.
`throws(one_head_conflict_risk(dispatch_first/2, [dispatch_ping/1]))`.

**F11 `one_with_bounded_retention_refused`**: `log keep(count(1)) one(1)`.
`throws(one_with_bounded_retention(dispatch_first/2, count(1)))`.

**F12 `one_key_positions_mismatch_refused`**: `key(1) one(2)`.
`throws(one_key_positions_mismatch(dispatch_winner/2, [2], [1]))`.

**F13 `one_on_level_headed_rel_refused`**: `one(1)` on a rel headed by `<-`.
`throws(one_on_level_headed_rel(dispatch_view/2))`.

### 9c. COUNT tests, the formerly-quadratic path

The admission probe is a per-occurrence existence question, the exact shape the
N+1 law bans as a per-row read. `(opus §7c)` holds the line, additive:

1. `one_admission_emits_no_extra_statements` (compile/test/plunit_tests.pl):
   compile program P with `one(1)` and without; assert the emitted statement
   COUNT is EQUAL. The probe is a `NOT EXISTS` conjunct inside the arm's
   existing project SQL, so a design that regresses to a separate probe
   statement per occurrence fails at N=2.
2. `one_admission_probe_searches_the_key_index` (v6/tsv2 test, the
   `structPlane.test.ts` EXPLAIN pattern): assert the admission subquery's plan
   line is SEARCH over the index on the `one()` positions, never a full index
   walk.

Grafted second COUNT test `(kimi PLAN.md §4d)`, the cascade counterfactual. The
rejected cascade desugar put k-1 negated guards on arm k: 6 arms would carry
15 `edge_absence` edges and N(N-1)/2 = 15 arm-side terms, 21 total. The landed
desugar carries zero. Two grading programs assert exact integers:

- `one_arm_count_stays_linear_2`: 2 arms on one `one` head; exactly 2
  `trigger` edges into the head, 0 `edge_absence` edges program-wide, 2 emitted
  edge statements, 1 write.
- `one_arm_count_stays_linear_6`: 6 arms; exactly 6 `trigger` edges, 0
  `edge_absence` edges, 6 emitted edge statements, 1 write. The test's comment
  names the rejected cascade path numbers (15 absence edges, 21 arm-side terms).

A third, cheaper receipt `(opus §7c)`: `just text-door` must stay byte-identical
for every program that carries no `one()`, proving S1 is inert on the corpus.

### 9d. Clock test `(opus §7d)`

`compile/test/3_clock_check.test.pl` gains `one_decl_adds_no_dependency_edges`,
asserting R1 as a test: the dependency list and the boundary list for program P
are equal with and without `one(1)`, and both boundary lists are `[]`. Sits
beside `two_arm_negation_guard_is_a_race_not_one` (:636-655), which stays as
written and keeps its sabotage receipt.

---

## 10. What is naturally NOT built

### 10a. The takeover door, now the lossless queued admission

`one_admission_no_lockout` names two sound folds and forbids a design that
forecloses either. First-wins ships; the second door stays open, rewritten per
`admission_word` (section 1): LOSSLESS QUEUED admission, one admission per key
per tick, the remaining contenders WAIT for successive ticks, nothing is
dropped. The drop-flavored spellings (`throttle`, `exhaust`) and the word `zip`
are rejected; the surface spelling comes from the rx concat family, exact form
is an OPEN pricing question (section 12).

The rx lowering is concatMap-shaped (the ruling names this territory verbatim):

```ts
const gateLeader$ = merge(...arms).pipe(
  groupBy((row) => row.repo),
  mergeMap((group) =>
    group.pipe(
      // one admission per key per tick; the rest queue for successive ticks
      // concatMap-shaped; exact surface spelling OPEN (section 12)
    ),
  ),
);
```

Why it is second rather than first: it is NOT batch-invariant. Two arrivals in
one tick admit one row; the same two arrivals split across two ticks admit two
rows and emit a replacement delta. Its result depends on the outside world's
batching, which is what the batch-invariance boundaries exist to label. When it
is built it needs its own non-refusing boundary, and that boundary is the
design work this lane is NOT doing. `(opus §8a, rewritten for admission_word)`

### 10b. Also not built `(opus §8b)`

| deferred | reason |
| --- | --- |
| read-side enum exhaustiveness (a `match` over a tag column proving guard coverage) | needs guard analysis that recognises `Tag == 'air'` as a variant test; S3 delivers write-side coverage only |
| the fold construct's name | outside the scope fence; the fold name routes through the stream_cards per rulings.pl:453 |
| dead variant rels for tag-only enums | R4 measured them; suppressing them changes enum expansion, a separate arc |
| refusing the negation-guard spelling once `one()` exists | forbidden by `one_admission_no_lockout`; it stays legal and gains arrival-order agreement through 6b step 3 |
| the brace block surface | `block_lowering_first`: the lowering is the construct; braces arrive in a later sugar wave |

---

## 11. Landing sites

| file:line | edit | stage |
| --- | --- | --- |
| compile/parse_dl.pl:596-608 | `one_clause/3` in `decl_a_modifiers/4`; the reserved admission-word clause beside the removed-word precedent | S1 |
| compile/parse_dl.pl:727-730 | `one_clause/3` beside `key_clause/3` | S1 |
| compile/parse_dl.pl:270 | `declaration_source_ref(one(Ref,_), Ref)` | S1 |
| 0_program_check.pl:14, :56 | export and define `declared_admission/3` beside `declared_key/3` | S1 |
| 0_program_check.pl:95-130 | four new `program_violation/3` clauses (6a table) | S1 |
| analyze.pl:53 | `decl_one/3` beside `decl_key/3` | S1 |
| analyze.pl:1156, :1230 | refusal class list plus `compiler_refusal/3` rows | S1 |
| analyze.pl:1341-1353 | `check_no_edge_head_conflict_risk/2` guard reads key OR admission | S1 |
| conformance/engine.pl:103 | `decl_one/3` | S1 |
| conformance/engine.pl:151, :208 | refusal class list plus `engine_refusal/3` rows | S1 |
| conformance/engine.pl:397-403 | `one_admission_tie/3` clause | S1 |
| conformance/engine.pl:405-423 | admission probe in front of the log-append branch | S1 |
| lower.pl:1560-1566 | `TriggerKind = ordered_arrival` when the head carries `one(_)`, and when an arm negates an edge-headed ref | S1 |
| lower.pl:386-403 | reuse `compile_negative_uses/5` for the admission conjunct | S1 |
| emit_ts.pl:1496-1510 | admission Map and tie throw inside `applyOrderedOccurrence` | S1 |
| print_dl.pl:228-230, :303-305, :313-316 | `one(Ref, Positions)` in the three modifier lists | S1 |
| conformance/FIXTURES.md | `one(Ref, Positions)` in the Decls list | S1 |
| new 0_merge_expand.pl, wired in 1_expansion.pl | lowering to flat rels + catalog rows + captured-arg distribution, before analyze | S2 |
| compile/registry.pl:189 | `surface(any/1, ...)`, `surface(one/1, ...)` block rows | S2 |
| print_dl.pl | block printer beside the match block | S2 |
| 0_type_plane.pl:126 | enum branch returning `text` storage | S3 |
| 0_program_check.pl:164-170 | `column_type_unknown` accepts enum names | S3 |
| 3_clock_check.pl | NO EDIT (R1) | all |
| compile/registry.pl:208-214 | NO NEW `clock_role/4` ROW (R1) | all |

Battery to re-run at each stage: `just green-all`, and inside it `just
conformance`, `just plunit`, `just text-door`, `just golden-flex`, `just
roundtrip`. `(opus §9)`

---

## 12. Open questions

1. The reserved admission door's exact surface spelling. `admission_word` rules
   the semantics (one admission per key per tick, the rest queue, nothing
   dropped, concat-family spelling) and the rx lowering is concatMap-shaped
   (section 10a), but the exact dl surface form is priced HERE and not
   invented in this contract. Candidates from the rx concat family only.
2. Tag-only enums mint one dead variant rel per variant carrying
   `keyed(Ref, [])` (R4). Suppress them, or leave them? Suppressing edits enum
   expansion.
3. `one(Positions)` versus a name from the SQL pool. The vocabulary tiebreak
   takes the SQLite spelling on doubt; SQLite has no word for this fold,
   prolog's `once/1` is the exact concept and rx's is `take(1)`, so `one()` is
   the user's own word backed by prolog. Confirm, or name another.
4. The loud-loser requirement, grafted from `(flash PLAN.md §Construct 2,
   §Emitter)` as an open question. During `one` admission a dropped candidate is
   invisible in the delta model: the delta shows only the survivor. Two
   readings: a VISIBLE drop channel (the tick log records a named refusal line
   per dropped row, so a reader sees that two arms fired and which was dropped)
   versus tick-log-is-the-audit (the arrival list is the record, and that is
   enough this round). The verdict lists this as the flash graft at clause 5;
   opus's delta model currently chooses the audit reading, but the visible drop
   channel is not foreclosed and should be priced before the fixtures pin the
   delta shape.
5. Does the `any ( ... )` / `one ( ... )` block earn its brace wave, given it
   is pure spelling over arms that already work? S1 alone satisfies the "one of
   these" ask; the block lowers to flat rels + catalog rows per
   `block_lowering_first`. `(opus §10 Q4, rewritten)`

---

## 13. Marble type lattices

Slot from `plans/2026-08-04-marble-type-lattices.md`, read in full at
`~/projects/sprefa-lab-fuse`. This section keeps its three lattice value sets
and the extension laws verbatim and links the source doc for the rest.

The marble model gives every rel one value per axis, inferred by abstract
interpretation over the closed operator set, same fixpoint shape as the subscribe
cone.

**Lattice value sets (verbatim from the source doc, section 1).**

Axis `subject`: which rx subject class a late subscriber sees. Flat lattice; any
two distinct classes join to unknown: `⊥ unreachable`, `from_table` (cold
`from()` over storage), `replay` (`ReplaySubject(keep N)`), `behavior`
(`BehaviorSubject` latch), `subject` (bare `Subject`, live only), `⊤ unknown`.

Axis `cardinality`: rows per key per tick, a six-point interval lattice. No
parameter N anywhere; `keep(count(N))` stays a declaration, never a lattice
point, so ascending chains are finite: `⊥`, `0`, `1`, `0..1`, `1..ω`, `0..ω ⊤`.

Axis `completion`: rx completion behavior. Flat lattice: `⊥`, `complete`
(emits then completes), `never` (rx NEVER, stays live), `⊤ unknown`.

**Extension laws (verbatim from the source doc, section 2).**

| law | statement | what it buys |
|---|---|---|
| top-default | every construct absent from the transfer table maps to ⊤ on every axis | a new operator costs zero rows for soundness; precision rows are added only when someone wants them |
| refinement-only | later work may move a rel down a lattice (more precise), never to an incomparable point that changes consumer behavior | emitted code survives model growth |
| ancestor-correct consumers | every consumer (rust lowering first) must be correct at ⊤ and at every value above the one it optimizes for | refinement is always pure optimization: Vec narrows to Option, a kept task becomes droppable; correctness never moves |
| finite lattices | all three lattices are finite and parameter-free | SCC fixpoint terminates by construction; the widening wall from the 2026-08-03 feasibility sketch dissolves |
| new flat point | extending a flat axis = adding one incomparable point; existing joins are untouched | until(F) lands in completion later without touching complete/never programs |

Storage is EAV rows `marble_fact(RelRef, Axis, Value)`; an unqueried axis is
absent and absence means ⊤, so old programs and fixtures never need backfill
when an axis is born. Transfer rows are precise-first with a catch-all ⊤ clause
last (the top-default law as code). The three descending axes drive the rust
consumer: cardinality narrows `Vec<Row>` to `Option`/single, `subject`
`from_table`/`replay`/`behavior` does a boot read of storage before stream
attach while bare `subject` skips it, and `completion complete` drops the task
after settle.

Relevant to this contract: the tick_boundary ruling fixes ingress cardinality
to 0..ω, with 0..1 arising only under a `one()`-family decl; both first-wins
and the queued takeover yield cardinality ≤ 1 per key per tick, so either fold
gives the same marble value. Marble rows are inference outputs one-directionally
downstream of decls; no cycle with `one_decl_surface`.

Source of the rest (contradiction audit, worked rx lowering, rust mapping,
live_event resolution, parked axes): `plans/2026-08-04-marble-type-lattices.md`.

---

## 14. Discards from the duel

Named in `plans/2026-08-04-rxprim-duel-verdict.md`; one line each, with the
verdict's reason.

| discarded | reason (verdict doc) |
| --- | --- |
| kimi's edge_head_conflict_risk stand-down | reopens the source-arm-order tiebreak the ruling removes |
| kimi §5 + flash §3 typed merge | both assume enum-name column typing and tag-column exhaustiveness that do not exist today; opus R3+S3 replace them with four named checker edits |
| flash's COUNT-as-conformance-expectation | a form the harness does not have |

---

## 15. Verdict recap

The duel verdict `(plans/2026-08-04-rxprim-duel-verdict.md)` ended the fork on
word 1 between the opus `throttle(1)` spelling and the shelf sketch's
`zip(perKey, ticks$)` spelling. The ruling `admission_word` rejected both
words for this construct and replaced them with lossless queued admission from
the rx concat family; those two historic names appear nowhere above as live
spellings, only here. `block_lowering_first` resolved duel word 2: the block's
lowering is the construct, and the brace surface is a later sugar wave.
