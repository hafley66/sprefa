# Staged writes: the tool's namesake, measured

Lab branch `lane/staged-writes`, base `22c0c9f71ca6b16e848c53f8980f4b0c6e3d6ecd`.
Zero production edits. Runnable evidence: `v6/tsv2/labs/staged-writes/receipts.sh`
(`STAGED WRITES LAB HOLDS`, 25 assertions, exit 0), driving six `.dl6` programs
through the real compiler (`compile_dl6.sh`), the real served engine
(`v6/tsv2/serve/main.ts`) and the real `sh` host path. Nothing is stubbed.

## The one-paragraph answer

A v6 program can already locate a marker pair, compute the replacement text, and
stage the result as rows, and it can already write files, and the write can
already be gated behind a second explicit demand that a human pushes. All three
were measured end to end. The write plane stops at three places, and only one of
them is the back-pressure question. First, **a host's input is a row and a
write's payload is a relation**: there is no string aggregate in the registry, so
N lines cannot become one command, and the only shape that compiles is N spawns
and N whole-file rewrites. Second, **the engine is blind to the disk it wrote**:
effect identity is content-addressed over the demand, so reverting the file
behind the engine's back produces no re-write and no complaint. Third, **a write
is at-least-once**, measured by `kill -9` between the disk write and the answer,
and no ordering of the claim/settle pair can close that window. The
do-not-advance-until-answered rule fixes none of the three. What it costs is the
determinism of the tick log, which is the cross-target grading contract.

## What was measured

| # | receipt | result |
|---|---|---|
| 1 | staged diff as rows, tree byte-unchanged | `edit_add` 3 rows / `edit_del` 1 row, file digest identical before and after |
| 2 | unarmed vs armed | 0 write spawns unarmed; one `armed('fnlist')` row rewrote the zone, markers and surrounding text intact |
| 3 | payload cost | 3 staged lines = **3 spawns** = 3 whole-file read-modify-writes |
| 4 | disk reverted behind the engine, demand retracted and re-asserted | **zero respawns**, file stays reverted |
| 5 | tick vs in-flight effect | ticks 1 and 2 returned in **48 ms** with two 3 s effects outstanding; the answers landed at **+6.3 s** |
| 6 | `kill -9` between disk write and answer | write **replayed** on restart, 1 line -> 2 lines |
| 7 | byte-span addressed write | span reached the program as flat ints `(2, 4, 77, 86)`; the byte range was replaced, markers survived |
| 8 | host column named `ordinal` | host answers 7 and 8; the program derives **nothing**; the stored response row is `["","","s1",1,"eight"]` |

Sabotage receipt (run, reverted): deleting the two file-writing lines from
`zone.py cmd_put`, so it still answers `{"wrote": 1}` and still counts its spawn,
turns phase 2 red with exactly `FAIL phase 2: armed program did not change the
file` and moves nothing else. Phase 2 grades the disk, not the answer.

---

## 1. What exists

The marker half is done. `plans/2026-07-29-comment-node-verdict.md` proved seven
`comment_node` techniques at 745/745 byte-exact against v5, and the comment rails
that landed this session include `v6/dl/fixtures/comment-zone-rail.dl6`, whose
whole job is locating `BEGIN: gen <name>` / `END:` pairs and exposing the owned
range. Nothing about finding a zone is missing.

What this lab adds is the other half of the sentence: *compute replacement text
and emit it*. `1-stage.dl6` does it, and reads top down with no mode flag:

```
rel file(path: text, digest: text).

sh zone_now(path: text, digest: text) -> (zone: text, slot: int, line_text: text) =
  `: {digest}; python3 "$LAB_ZONE" body {path}`.
rel have(path: text, zone: text, ordinal: int, text: text).
have(path, zone, slot, line_text) <-
  file(path, digest), zone_now(path, digest, zone, slot, line_text).

sh gen_fns(path: text, digest: text) -> (slot: int, line_text: text) =
  `: {digest}; python3 "$LAB_ZONE" fns {path}`.
rel want(path: text, zone: text, ordinal: int, text: text).
want(path, 'fnlist', slot, line_text) <-
  file(path, digest), gen_fns(path, digest, slot, line_text).

rel edit_add(path: text, zone: text, ordinal: int, text: text).
edit_add(path, zone, ordinal, text) <-
  want(path, zone, ordinal, text), not(have(path, zone, ordinal, text)).

rel edit_del(path: text, zone: text, ordinal: int, text: text).
edit_del(path, zone, ordinal, text) <-
  have(path, zone, ordinal, text), not(want(path, zone, ordinal, text)).
```

Its rx lowering, per the standing law:

```js
const have$ = file$.pipe(mergeMap(f => zoneNow(f)));
const want$ = file$.pipe(mergeMap(f => generator(f)));
const add$  = combineLatest([want$, have$]).pipe(
                map(([w, h]) => differenceWith(w, h, isEqual)));
const del$  = combineLatest([have$, want$]).pipe(
                map(([h, w]) => differenceWith(h, w, isEqual)));
```

`not(...)` over a plain relation atom is the `NOT EXISTS` the emitter writes,
which is `differenceWith` at the set level. Both directions are needed because a
diff is antisymmetric and the language has no single operator for it. **That is a
small named gap**: every staged-write program will write these two rules, and
they are one concept.

### Where it stops, exactly

**The payload is a relation and a host takes a row.** The registry's aggregate
inventory is `count/1 sum/1 min/1 max/1 avg/1` live, `json_array/1
json_object/2` refused (`v6/prolog/compile/registry.pl:89-102`). There is no
string aggregate. So the N lines of a zone cannot be folded into one column and
handed to one command. The only shape that compiles is one invocation per line:

```
sh put_line(path: text, zone: text, slot: int, text: text) -> (wrote: int) =
  `python3 "$LAB_ZONE" put {path} {zone} {slot} '{text}'`.

rel applied(path: text, zone: text, ordinal: int, wrote: int).
applied(path, zone, ordinal, wrote) <-
  armed(zone),
  edit_add(path, zone, ordinal, text),
  put_line(path, zone, ordinal, text, wrote).
```

Receipt: 3 staged lines, **3 spawns**, 3 separate read-modify-writes of the same
file, serialized by the runner's `concatMap`. The file came out correct here only
because `zone.py put` addresses by ordinal and re-reads each time; an append-
shaped helper would have produced delta-order garbage. The ORDER of a multi-line
write is currently decided by delta order, not by the rules, and nothing says so.

Compare v5, which solved this by putting the fold in the engine:
`src/engine/gen.rs` renders body rows through a row template and joins them
(`groups[&p].join("\n")`, `:188`), accumulating across every gen rule in the tick
and applying **once per file** (`apply_splices`, `apply_cursors`, `apply_zones`,
`apply_appends`). The v6 language has no equivalent because it has no way to say
"these rows, in this order, are one value".

**Second stop, and it is a live defect this lab found.** `6-ordinal.dl6`
declares a host output column named `ordinal`. The compiler notices the clash
with its own runtime column and renames *its* columns rather than refusing:

```sql
-- colliding host
CREATE TABLE "__host_response_two_rows" ("col1" TEXT, "col2" TEXT, "id" TEXT,
  "ordinal" INTEGER, "payload" TEXT, PRIMARY KEY ("col1","col2")) WITHOUT ROWID
-- ordinary host
CREATE TABLE "__host_response_slow" ("witness_digest" TEXT, "ordinal" INTEGER,
  "id" TEXT, "secs" TEXT, "ok" TEXT, PRIMARY KEY ("witness_digest","ordinal"))
```

`serve/1_hosts.ts` `project()` fills that row **by literal name**:

```js
if (column === "witness_digest") return witnessDigest;
if (column === "ordinal") return ordinal;
const input = demand.inputs.get(column);
```

so `col1`/`col2` fall through to an input lookup that misses, then to
`outputRow[findIndex(...) === -1]`, then to `?? ""`. Measured consequences, all
silent: the witness column is empty so the demand-to-response join is dead and
the program derives nothing; the primary key degenerates to `("","")` so every
row of a multi-row answer collapses to the last one; and the declared `ordinal`
column receives the runtime's row counter instead of the host's value. Two halves
of one system disagreeing about a column name, with no refusal and no trace.

This is a write-plane hazard specifically because a write host's answer is the
only thing that says the write happened. The disk is already changed and the row
that reports it is wrong. It applies to `witness_digest` identically. **Owner:
unassigned. The fix is a load-time refusal (`host_column_shadows_runtime`), not
a rename** — the compiler already detects the collision, it just picked the wrong
response.

---

## 2. Is `sh` enough for a write plane?

No, and the reasons are not the ones the question implies.

`sh` is enough for *invocation*: phases 2 and 7 wrote real files through the
shipped path. What `sh` does not carry is the set of properties v5 spent four
apply functions on, every one of which is missing here:

| property | v5 (`src/engine/gen.rs`) | v6 today |
|---|---|---|
| convergence (skip write when bytes match) | four sites, `:191 :375 :420 :532`, each emits `gen_write {wrote:false}` | none. `zone.py` writes unconditionally; the *demand* dedupe hides this, badly (see below) |
| one target claimed by one rule | `claimed` map, loud bail `:178` | none. Two hosts writing one file race under `concatMap` |
| region overlap gate | `apply_splices:350`, `apply_cursors:485`, both bail | none |
| coordinates stay valid across a batch | bottom-up / right-to-left apply, per file | none. Every write re-reads |
| whole-tick atomicity | accumulate across all gen rules, one write per file | none. N writes per file |
| rollback | `journaled_write` (`src/engine/query.rs:179`) records pre-write bytes; `run_verify` (`src/lib.rs:444`) restores on checker failure | none |
| dry run | never under `--check`/`--lsp`; one-shot needs `--apply` | **better than v5**: `armed` is a row (below) |
| ordering | output-text order within a rule, program order across rules | delta order, unstated |

The `sh` seam can host all of these, because they are properties of the *helper*,
not of the language. That is the honest read: `zone.py` could implement
convergence and overlap gating tomorrow. What `sh` cannot host is the two
properties that are genuinely engine-level:

**Idempotence against the disk, which the content-addressed cache actively
prevents.** Phase 4: apply the zone, then revert the file on disk behind the
engine, then retract and re-assert the demand. **Zero respawns, file stays
reverted.** Effect identity is content-addressed over the demand
(`identity|put_line|path=...|zone=...|slot=0|text=...`), and the in-process
`claimed` Set plus `__host_witness` both key on that. So an identical demand is
a cache hit forever, and the world it wrote to is not part of the identity. This
is exactly right for a read and exactly wrong for a write: a read's answer is a
function of its inputs, a write's *necessity* is a function of the target's
current state. The demand digest would need the target's digest in it, which
means the write host must take the file digest as an input — and then the engine
must learn the new digest after writing, which it currently never does.

**Knowing that it wrote.** Nothing feeds the post-write state back. The write
host answers `wrote: 1` and the `file(path, digest)` row still carries the old
digest. Under the live watcher (`bind watch`) the new digest does arrive as an
ordinary arrival, which closes the loop — and closes it through the *world*,
which is the ruled `spine_residency` position and is correct. Under a
POST-fed program it never closes. Neither case is refused or warned.

---

## 3. The back-pressure rule: what it costs

### What ships today, measured

Two lanes, joined only by causation:

```
arrivals$ ──concatMap(runBatch)──▶ ticks$ ──▶ demand deltas
                                                  │
                                    concatMap(spawn)   serve/1_hosts.ts
                                                  │
                                       engine.submit(response rows)
                                                  └──▶ back into arrivals$
```

`LiveEngine.runBatch` (`serve/3_engine.ts`) stops on exactly two conditions:
`!deltas.carryPending || this.queuedBatches > 0`. Neither mentions an effect.
Phase 5 measures the consequence directly: two jobs, each a 3 s `sleep`, posted
back to back. Both `POST /arrivals` returned inside **48 ms** carrying tick 1 and
tick 2; the answers landed at **+6.3 s** (serialized, per the teardown lab's R5).
**The tick does not wait.**

### The rule, and where it would go

The user's rule is "the tick where the write happens does not let the next tick
happen until we have the response". As rx, that is one `concatMap` inserted
between the two lanes:

```js
// today
arrivals$.pipe(concatMap(batch => tick(batch)))

// with the rule
arrivals$.pipe(concatMap(batch =>
  tick(batch).pipe(concatMap(deltas =>
    settledFor(deltas).pipe(toArray(), map(() => deltas))))))
```

where `settledFor(deltas)` completes when every host demand row the tick produced
has reached `done` or `error` in `__host_witness`. That site is
`LiveEngine.runBatch`'s `expand`. It is a small change. Its costs are not small.

**Cost 1, the one that matters: the tick log stops being deterministic.** The
tick log is currently a pure function of (program, arrival schedule), which is
what makes it the cross-target grading contract (ruling
`json_ticklog_encoding = canonical_json_text`, and every `oracleLog` /
`servedLog` comparison in `tests/serveHelpers.ts` rests on it). Under the rule,
a host response lands *inside* the tick that demanded it rather than as a later
submit, so which tick number carries which rows becomes a function of effect
wall time. `engine.pl` has no effect lane at all, so the oracle cannot grow the
same behaviour; the byte grading for any program containing a write simply
ends. The runtime bridge's already-recorded crack ("served drain numbering
differs when programs carry") is the same class one notch milder: that one is
deterministic and merely different, this one is not deterministic.

**Cost 2: one slow write freezes the whole engine.** Hosts are already
serialized by `concatMap` with no concurrency knob and no timeout anywhere in
`1_hosts.ts`. Today that costs the *effect* lane only. Under the rule it costs
the tick lane, so an unrelated watcher event, an unrelated bind tick, and an
unrelated HTTP query all wait behind a `git push` in someone's `sh` template.
Phase 5's 6.3 s is what that stall would have been.

**Cost 3: a reachable, undetected deadlock.** The most natural workaround for
the payload-is-a-relation problem is a host that reads the rows back out of the
engine (`curl $BASE/idb/edit_add | apply.py`). Today that composes, because the
lanes are independent. Under the rule it is a hard deadlock: the tick waits for
the effect, the effect waits for a server that is inside the tick. Nothing would
detect it; `DRAIN_CAP` counts drain ticks, not wall time.

**Cost 4, stated as a non-cost: crash safety is unchanged.** This is the result
worth carrying forward. Phase 6:

```
WitnessCache.claim(...'pending')   -- durable, BEFORE the spawn
  spawn -> the file is appended    -- the disk effect commits here
  answer printed, then sleep       -- kill -9 lands in this window
WitnessCache.settle(...'done')     -- durable, AFTER the answer
```

On restart `clearDeadLocks` deletes every `pending` row, the demand row is
durable, boot replay re-runs it, **and the line is appended a second time**
(measured: 1 -> 2 lines). Back-pressure does not narrow that window by one
instruction, because it is not a window in the tick lane at all. A durable record
written before the spawn cannot assert the effect happened; one written after
cannot be reached if the process dies. **The rule buys ordering, not
durability.**

### Can a write ride the durable-witness story?

Conditionally, and the condition is precise: **only a write that is idempotent
under replay.** That is not a hopeful framing, it is exactly what v5 built. Every
one of `gen`'s four apply arms is a *whole-region replace* guarded by a
bytes-equal check, so replaying it is a no-op. Nothing in v5's write plane
appends. My `4-crash.dl6` appends on purpose, which is why it doubles.

So the shape that works today, with no engine change:

- a write host must be **replace-shaped**, never append-shaped
- it must **converge** (compare, then skip), so a replay is not even a write
- and that convergence must live in the helper, because the engine has no
  `wrote: false` concept

and the shape that cannot work today, and is not refused:

- any accumulating write (append, insert, counter bump, `git commit`, `POST`)

The endurance goal ("kill -9 mid-delay, reboot, value lands exactly once") is
therefore satisfiable for replace-shaped writes and structurally unsatisfiable
for accumulating ones, on this seam, with or without back-pressure. A named
refusal is not available (the compiler cannot know a template's shape), so this
is a documentation-and-convention obligation, the same class as the
`decodeObjectItems` zero-row silence that `1_hosts.ts` already routes to the
trace surface.

### The hypothesis I would rather test than the rule

Stated as a hypothesis, hedged, because it is a design claim and not a
measurement: **the rule is trying to buy read-after-write consistency, and the
thing the next tick needs is not the write's answer but the target's new
digest.** The file is already an input to the program through
`file(path, digest)`. If the write host's declared outputs included the
post-write digest —

```
sh apply_zone(path: text, zone: text, digest: text, ...) -> (wrote: int, new_digest: text) =
  `...`.
rel file(path: text, digest: text).
file(path, new_digest) <+ applied(path, zone, wrote, new_digest).
```

— then the write's completion feeds the file rel as an ordinary edge arrival, the
next tick that touches that file is computed against the new digest, and any
tick computed against the stale digest merely mints a demand that is superseded
(and, per the teardown lab, superseded demands already produce a `del` that
nothing reads, so this composes with the flattener work rather than fighting it).
No back-pressure, no new construct, one extra output column. Phase 4 is the
receipt that this loop is currently open; I did not build the closed version.

---

## 4. Dry run, as rows

This part already works and is, I think, better than what v3/v4/v5 had, because
the staged edit is not a preview *rendering*, it is a *relation*. `2-apply.dl6`
in full, the way you would type it:

```
rel edit_add(path: text, zone: text, ordinal: int, text: text).
edit_add(path, zone, ordinal, text) <-
  want(path, zone, ordinal, text), not(have(path, zone, ordinal, text)).

rel armed(zone: text).

sh put_line(path: text, zone: text, slot: int, text: text) -> (wrote: int) =
  `python3 "$LAB_ZONE" put {path} {zone} {slot} '{text}'`.

rel applied(path: text, zone: text, ordinal: int, wrote: int).
applied(path, zone, ordinal, wrote) <-
  armed(zone),
  edit_add(path, zone, ordinal, text),
  put_line(path, zone, ordinal, text, wrote).
```

With no `armed` row the program is a pure analyzer: phase 2 measured **zero write
spawns** and a byte-identical tree while `edit_add` held all three rows. The human
reviews `GET /idb/edit_add`. Saying yes is one row:

```
POST /arrivals {"batch":[{"rel":"armed","sign":"add","row":["fnlist"]}]}
```

Its rx lowering:

```js
const apply$ = armed$.pipe(
  withLatestFrom(editAdd$),
  concatMap(([zone, edits]) => from(edits).pipe(concatMap(run))));
```

Four properties fall out that `--fix` does not have. The consent is *scoped* (one
zone, not the run). It is *retractable* — `sign: "del"` removes it, and the
retraction propagates through the ordinary IVM. It is *auditable*, since it
appears in the tick log like every other row. And it *composes*: `armed(zone) <-
approved_by(user, zone), not(protected_path(path))` is an ordinary rule, so
policy about what may be auto-written is written in the same language as the
edits.

With the ratified arm arrows the two outcomes read in one block, top down:

```
match edit_add(path, zone, ordinal, text) (
  ; armed(zone)      |-> pending(path, zone, ordinal, text)
  ; not(armed(zone)) |-> staged(path, zone, ordinal, text)
)
```

I did not compile that variant; `match` arms plus `not/1` in an arm guard is the
combination `plans/2026-07-28-match-frontier-lab-verdict.md` flagged as
unstratified in `+>` arms, and it is a level rule here, so it probably compiles.
Unverified, stated as unverified.

**The zero-construct variant, worth naming because it needs nothing at all.**
The apply step does not have to be inside the engine. `edit_add` is readable at
the boundary, so:

```
dl q edit_add | zone.py apply-all
```

is a complete staged-write tool today, with every property of v5's `--fix` and
none of the tick-lane questions: one process, whole-file writes, real ordering,
real atomicity, and a trivial rollback (`git checkout`). If the goal is shipping
marker regeneration this month rather than settling the effect plane, this is
the recommendation, and the in-engine apply arm is the thing to build after the
payload construct exists.

---

## 5. Span and CST writing

Better than the ledger says, in both directions.

**Reading spans is done.** Two independent paths, both live:

1. *Flat ints.* `5-span.dl6` declares `-> (zone: text, begin_line: int,
   end_line: int, start: int, end: int)` and phase 7 read `(2, 4, 77, 86)` off a
   real file. `scripts/comment_node.py`'s `lines` projection already flattens
   `span.start`/`span.end` into top-level int fields for exactly this reason, and
   `v6/dl/fixtures/flagship-flow.dl6`'s `sig_at` host uses the same shape
   (`owner_start: int, owner_end: int`).
2. *The struct plane.* `rel span(start: int, end: int).` used as a column type is
   the ruled `compound_storage = struct_as_rows` shape, and **it now compiles on
   host outputs**. Phase 7 recompiles `flagship-flow.dl6` and finds
   `unsupportedExecution: readonly string[] = []` with
   `"span" INTEGER NOT NULL` in the response DDL (the dictionary id). The
   `host_struct_output_type` named stop written into that fixture's own header
   (`unsupported_surface(column_type_wrapper(...))`) **is stale**, as is the
   flagship arc's "byte spans cannot enter programs" note. Worth a header fix on
   the next fixture-touching arc.

**Writing by span is done too, at the seam.** Phase 7's apply arm is
`splice_span(path, start, end, text, wrote)` over a half-open byte range; the
range was replaced and the surrounding text survived. Nothing about it knows the
word `BEGIN`. Any producer of `(start, end)` drives it, which is what makes it
the CST write path and not a second marker path.

**So the span gap is not addressing. It is the same three gaps as everything
else, plus one:**

- payload is still one text per invocation, so a span write can replace a region
  with one value and not with a relation of lines
- coordinates go stale the instant any earlier write lands, and there is no
  overlap gate and no bottom-up ordering. v5 needed both (`apply_cursors:485`,
  `:500`) and got them by accumulating the whole tick's regions before applying
  any. A v6 program emitting two span writes into one file today is a silent
  corruption, because each host re-reads a file the previous host already grew.
  **This is the sharpest unowned hazard in the write plane** and it is worse than
  the marker path, where `zone.py` can re-resolve the marker by name. A byte
  offset cannot re-resolve itself.
- and the CST question proper: `sprefa-extract` emits spans, so a program can
  say "replace the span of this function's body". It cannot say "replace this
  node with a *tree*" — there is no rendering side. Every write is text.
  v3/v4/v5 were the same, so this is a boundary of the design and not a
  regression.

---

## Named slots

| slot | question | this lab's reading |
|---|---|---|
| `SLOT-WRITE-PAYLOAD` | how does a relation become one command's input? | a host input declared as a rel, passed on the child's stdin, identity digest over the rows. One construct, fixes ordering, spawn count and convergence at once. Not built. |
| `SLOT-WRITE-CONVERGENCE` | who owns skip-if-equal? | the helper today; the engine in v5. If the write host also outputs `new_digest`, the engine can own it |
| `SLOT-WRITE-OVERLAP` | two writes, one file, one tick | v5 bails loudly. v6 corrupts silently. Needs an answer before any span-write rail ships |
| `SLOT-WRITE-SHAPE` | replace vs append | replace-shaped writes ride the durable witness; accumulating ones cannot. Undecidable at load time; convention plus trace |
| `SLOT-BACKPRESSURE` | do-not-advance-until-answered | costs tick-log determinism, buys ordering only, does not touch crash safety. Recommend testing the `new_digest` feedback loop instead |
| `SLOT-DIFF-OPERATOR` | `edit_add` + `edit_del` are one concept written twice | every staged-write program will write both rules |
| `SLOT-ARM-CONSENT` | is `armed` the right spelling for the human yes? | it is a row, so it is scoped, retractable, auditable and composable. Better than `--fix`. Needs a name |

## Live defect found, unowned

**`host_column_shadows_runtime`.** A host declaring an input or output column
named `ordinal` or `witness_digest` compiles clean and produces a response table
the runtime cannot fill: witness empty, primary key collapsed to `("","")`,
multi-row answers reduced to the last row, and the demand-to-response join dead
so the program derives nothing. The compiler detects the clash (it renames its
own columns to `col1`/`col2`) and chooses to rename rather than refuse;
`serve/1_hosts.ts` `project()` fills by literal name and never learns. Fail-first
fixture: `v6/tsv2/labs/staged-writes/6-ordinal.dl6`, phase 8. Fix is a load-time
refusal. This one bit this lab twice in one sitting, in both directions, and each
time the only symptom was rows quietly not appearing.

## Lab contents

```
v6/tsv2/labs/staged-writes/
  receipts.sh        STAGED WRITES LAB HOLDS, 25 assertions, exit 0
  zone.py            the policy-free marker/span helper the hosts call
  1-stage.dl6        staged diff as rows, read-only
  2-apply.dl6        + the second explicit demand
  3-backpressure.dl6 slow host, tick timing
  4-crash.dl6        write-then-sleep, kill -9 window
  5-span.dl6         byte-span addressed write
  6-ordinal.dl6      fail-first receipt for the collision defect
```

Per the lab protocol this dies on landing; the durable output is this document
plus whatever fixtures are promoted from `6-ordinal.dl6` and `5-span.dl6`.
