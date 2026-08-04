# Morning report (overnight 2026-08-04)

Five opus agents ran overnight. Two fixes are merged into sprefa main, three
investigations ended in documents waiting on your calls. Each item below opens
with the problem as you would hit it, then what happened overnight.

## Shipped into sprefa main

### 1. The engine died when its last viewer left

The problem: the v6 served engine only turns its tick loop while something is
subscribed to `ticks$` (the stream of tick results). Close the last browser
tab or drop the last query, and the engine tears its pipeline down. The next
arrival that gets submitted is then refused with "engine is not running", even
though the process is alive and the data is fine. In app terms: walk away from
the dashboard, come back, and submits made while nobody watched were rejected.

Why it existed: the pipeline is shared with rx `share()`, whose default
disconnects the source when the reader count hits zero. The engine's alive
flag was riding that reader count.

What shipped: one option on that share, `resetOnRefCountZero: false`, so the
lane stays connected once anything has ever subscribed. Readers now come and
go freely; ticks keep processing; a late reader sees everything that happened
while nobody watched; tick numbers do not restart. A 2-test fail-first file
pins it (both tests red before the fix, green after). All gates green.

One neighboring behavior deliberately kept: submitting before ANYTHING has
ever subscribed is still an error, because the pipeline has never connected at
that point and the batch would silently vanish. Making that work needs an
eager-connect at construction; separate card if you want it.

### 2. Laziness step 2: the engine can now skip work nobody asked for

The problem this ladder exists for: a compiled program evaluates every rule on
every tick even when no query reads the results. All the cone work so far
(query columns, subscribe cone, the strict "no query subscribes to nothing"
ruling) computed WHICH rels a program's queries actually need, but nothing
consumed that answer yet. The engine still did all the work.

What shipped: the emitted module now filters its tick work down to the
subscribe cone when the env flag `SPREFA_TSV2_SUBSCRIBE_PRUNE=on` is set. OFF
(the default, and the only mode anyone should run today) is proven identical
two ways: the filter literally returns the original arrays by reference, and
the byte-identity gate over all 202 compiled programs still passes. ON is
graded by a 4-way test matrix: query-bearing and zero-query programs, each
with the flag off and on. Ingestion is never pruned (your keep()-is-the-replay
ruling: rows must land in storage even when nothing reads them yet).

Bonus that mattered: typecheck had been red for every lane all night (202
errors). Root cause was an earlier step declaring a field on the emitted
program's interface without emitting the field. This lane fixed it; the
typecheck leg of green-all is clean for the first time since the rename.

### 3. Safety rule that came out of it, needing your ruling

A program with a live host (tree-sitter, shell) breaks under the flag: the
cone knows a query needs the host's RESPONSE rel, but nothing tells it the
paired DEMAND rel matters too, and without demand rows the host never fires.
Under replay fixtures this is invisible (schedules inject responses directly),
which is why every gate passes. Under a live host, flag on = the subscribed
rel silently stays empty.

So: the flag stays OFF for host-bearing programs until the demand/response
pairing edge is added to the cone. That edge is exactly the host
co-subscription question you have owed a ruling on since the cone-wire lane
found it. It now has a measured consequence attached. The fix touches the
module the oracle shares, and it would also make the only pruning fixture
fully-subscribed (weakening the prune gate), so both halves want a decision
rather than a quiet patch.

## Investigations that ended in decision docs

### 4. Why the golden-flex fold stops at two links: root cause found

Backstory: the compose lab found a self-referential fold in golden-flex stops
at exactly two links no matter when the third arrives (defect 1b), and
separately that a whole chain arriving in one batch only closes one step
(defect 1a). The cause did not minimize that night.

The overnight lane minimized it, and 1a and 1b turned out to be ONE defect.
The trigger: if a program contains even one ordered edge rule (`seq` or
`pre`), the emitter routes the ENTIRE program onto its "ordered" tick path.
That path rebuilds level rels by running each rule clause once per tick, with
no loop to fixpoint. A self-referential rule with 2 clauses can therefore
never build a chain longer than 2; add a third clause and the ceiling moves to
exactly 3 (measured). golden-flex has ordered rules, so its fold hits this;
the two-rule test program had none, so it chained fine. Hand-editing the
generated file to use the incremental path makes the third link land on both
the minimal repro and real golden-flex.

The 8-line minimal repro, the falsification ledger, and the fix site
(emit_ts.pl's ordered-tick level plane) are in sprefa-lab-foldwall/FOLDWALL.md.
Two side-findings: the golden's MODE PARITY gate is meaningless for ordered
programs (the mode variable is emitted but never read, so the gate compares a
run to itself), and the naive referee door has its own separate one-round bug.

Your call: dispatch the fix lane. It is well-scoped now (add fixpoint to the
ordered path's level recompute), but it shares a file with the one() emitter
change from the duel plan, so the two lanes want sequencing.

### 5. The one()/merge design duel: verdict in, two spelling calls are yours

Backstory: three plan lanes (kimi, flash, opus) got the identical design
contract for the merge/one construct family. An overnight auditor read all
three, verified their file citations against the tree, and re-ran the opus
leg's compiler probes.

Verdict: build on the opus plan. It was the only leg whose claims all
survived re-execution, and it found the single emitter gate (one TriggerKind
test in lower.pl) that decides arm-order vs arrival-order, meaning your
"arrival time wins" ruling can be closed for the new construct AND for
existing negation-guard programs in one change, oracle untouched. Five
specific sections from kimi and flash get grafted in; three sections
(including both typed-merge designs, which assumed compiler machinery that
measurably does not exist) get dropped. Full grading table in
plans/2026-08-04-rxprim-duel-verdict.md.

Waiting on you, in that doc's tail: the reserved name for the one-per-tick
door (`throttle(1)` per opus, `zip(tick)` per the shelf sketch), and whether
the block sugar ships with the property or later.

## Instant

The reactive review you asked for is done: 12 findings, delivered to
instant-fable as REVIEW-reactive.md next to its brief. Highlights in plain
terms: clicking any waterfall bar resurrects a dead lane (same bug as the row
click, third call site); the brush selection you drag gets wiped every 8
seconds by an unrelated background refresh; after the first session loads,
later sessions' tick marks never paint (a one-character React bug); one failed
tmux query permanently downgrades liveness grading until reload; two of the
three mail views have no live feed at all and only update on a button.

The blocker: instant-fable spent the night frozen at a permission prompt,
verifying MY message to it was genuine (its anti-injection check doing its
job, then stalling on approval). It has 6 unread envelopes. One keypress:
`tmux attach -t instant-fable`, press 2. Nothing lands in instant until then.

## Loose ends nobody owns yet

- roundtrip.sh rewrites 126 checked-in dl_view files at base (stale printer
  output); every overnight lane stepped around it.
- flash-prolog worktree fate and the 9 unmerged sprefa branches still await
  your word (inventory in the 20260803.9 session save).
- sprefa main is unpushed and greener than it has ever been; push+tag is
  yours.
