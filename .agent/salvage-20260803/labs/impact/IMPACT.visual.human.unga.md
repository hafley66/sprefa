# IMPACT, plain words

What it costs to make the engine lazy. No citations here; the receipts live
in IMPACT.md.

## The one-line version

Making level rules lazy is easy and mostly mechanical. Making edge rules
lazy is a real design choice with three answers, because an edge rule's
input is thrown away every tick and there is nothing to recompute from.

## How it works today

```
  world pushes                          the engine
  ------------                          ----------
  POST /arrivals  ─┐
  interval timer  ─┼──> one queue ──> TICK ──> run EVERY rule
  file watcher    ─┤                            run EVERY host
  host answers    ─┘                            write EVERY table

  the "?" line ......................... goes nowhere
```

A `?` line parses, travels through the compiler, gets written into the
emitted file as `{ rel: "foo", arity: 2 }`, and is then read by nobody. Its
arguments are thrown away before it even gets that far.

Nothing waits to be asked. Boot computes the whole program from scratch.
Every tick runs every rule. Every declared timer starts ticking the moment
the program loads. Every declared file watcher opens an OS watch and shells
out to git. Every host with a probe spawns subprocesses.

## The two arrows are not equally lazy

```
  <-  (level rule)          <+  (edge rule)
  ----------------          ----------------
  reads TABLES              reads THIS TICK'S CHANGES
  tables are on disk        changes are wiped at tick start
  wake up whenever          wake up late = you missed it
  recompute = same answer   nothing to recompute from
```

This is the whole problem. A level rel can sleep for a thousand ticks and
wake up correct, because its inputs are still sitting in SQLite. An edge rel
that sleeps through an event has lost that event forever, exactly like an
rxjs Subject with no subscriber.

## Three answers for the edge plane, and the price of each

```
  A. DROP IT
     event arrives, nobody is listening, event is gone.
     price:  free
     buys:   nothing
     hurts:  a git commit that happens before your first query is invisible.
             for a hook that fires once, that is usually the wrong answer.

  B. BUFFER IT
     hold events in memory until someone asks, then replay.
     price:  memory with no bound, a new word in the language,
             a new place the engine keeps state outside SQLite
     hurts:  replayed events land on the wrong tick number

  C. LET THE LOG REL BE THE BUFFER   <-- recommended
     a "log" rel already keeps its history in a table, already bounded by
     the program's own keep(...) clause. that IS a replay buffer with a
     declared size.
     price:  one new statement per waking rel
     buys:   no new word, no new storage, no new memory
     honest: a "set" rel only keeps its latest value, so for set rels C
             behaves like A, which is what "set" already means
```

C is the recommendation because B is building a queue the language already
has. The size of the buffer is already something the author writes down.

## Once warm, never cold again

The user's rule: after the first demand, a thing stays subscribed. It does
not go cold when the last reader leaves.

Found while checking this: the served engine currently does the opposite.
Its tick stream uses rxjs `share()` with the default settings, which resets
when the last subscriber leaves and then refuses every later submit with
"engine is not running". Today that is hidden because the server holds a
permanent subscription. Under a design where the query IS the subscribe, it
stops being hidden. One-line fix, and it can land before anything else.

## The composition the user asked for

Plain English: wait for the first pre-commit. After that, wake up every
second AND on every later pre-commit. Share it. Never go cold.

```
        pre_commit ──> [first one only] ──> switch to ─┬─> every 1 second
                                                       └─> pre_commit again
                                              (shared, never resets)
```

In dl that is three rules: one that opens a gate on the first pre-commit,
and two rules with the same head, one reading the timer and one reading
pre-commit directly. Two rules on one head is what "merge" means in datalog.
No new construct needed.

**And it already compiles today.** The clock checker walks it and finds no
conflict. That is the good news.

**The hazard is what happens if you write it slightly wrong.** Delete the
second leg (the direct pre-commit read) and the shape silently becomes
"first one only, forever":

```
   pre_commit #1 ──> gate opens ──> scan fires        good
   pre_commit #2 ──> gate ALREADY open ──> nothing    <-- silent
```

The gate is a set rel, so writing the same row twice is a no-op by design.
Nothing complains. The program compiles, the clocks line up, and the second
commit does nothing.

This is checkable. The engine already knows which rels are sets, which are
edge-written, and what the paths are. It just never asks the question.

Recommendation: make it a WARNING first, not an error, because "first one
only" is sometimes exactly what an author wants and the language has no
other way to spell it yet.

## Getting events in from outside

Right now:

```
  sh hosts        ── anyone can declare one, any name, fully typed
  binds           ── ONLY two exist: interval and watch. closed set.
                     you cannot declare a third one.
  POST /arrivals  ── works for any input rel, fully typed at the door
```

So the transport is already generic. Two things are missing:

1. Nothing says "this rel is fed by the git pre-commit hook". The engine
   guesses which rels are event sources by noticing nothing feeds them.
2. Nothing says "this query stands forever" vs "this query is a snapshot".
   There is a field for it in the emitted file with exactly one possible
   value.

Three ways to fix #1:

```
  A. one generic "event" bind, payload as json, decoded in-language
     price: small.  problem: every event source shares one clock identity,
            which is the exact thing that was missing. does not fix it.

  B. a new "source" declaration, typed like an sh host but with no command
     price: parser work, a coverage row, a new word in the language
     buys:  typed at load time, named error for a bad push, and a real
            declared clock identity

  B'. write B, but have it quietly expand into A plus a decode rule  <-- rec
     price: B's authoring surface, A's engine cost
     buys:  nothing new reaches the engine core
```

One thing to flag: there is a standing rule that says wall-clock cadence
enters as a bind, never as a new language word. A git hook is arguably not
a cadence. That argument is not settled here. **It is the single biggest
open question in this analysis and it is a user call.**

## What breaks if laziness turns on

```
  281 test fixtures.  FIVE of them have a query line.
  31 program files.   THIRTEEN have a query line.
```

If "no query means nothing runs", then 276 fixtures compute nothing and
every test goes red at once.

So: **default should be "no query line means everything is demanded"**.
Laziness becomes opt-in. The whole test battery stays green while the
machinery lands. The cost is that the eager default is now a thing that has
to be deliberately turned off some day, and that day is one big change
instead of 281 small ones.

Other things that move:

- 69 tests assert an exact tick COUNT. Pruning rules can remove ticks.
  These are the fragile ones.
- The big composition test (golden-flex) would keep passing while quietly
  testing less, because its coverage gate reads the source text, not what
  actually ran. Needs a second gate saying "this program is fully demanded
  on purpose".
- Tick logs stop being byte-identical for any program that prunes. The
  format is fine; the contents get shorter. That is the gate that will
  notice first.

## Things that do NOT change

- What the two arrows mean, inside a cone that IS demanded. Pruning removes
  whole entries from a list; it never edits one.
- The database schema. Table shapes are per-rel and unchanged.
- The retention (`keep`) behavior.
- All 60 "this program should be refused" tests. Refusals run before any
  pruning could apply.
- Term-door vs text-door byte-identity, because the pruning lives in a stage
  both doors go through.

## Do NOT prune tables

Statements are per-tick and worth pruning. `CREATE TABLE` runs once and is
cheap. Two existing rulings already say the table set is fixed at compile
and nothing creates tables mid-tick. Follow them.

One trap: two parts of the tick loop walk the RELATION list rather than the
statement list, and they issue several DELETEs per relation per tick. If you
only prune statements, those keep running over everything and the whole
exercise shows no measurable improvement.

## Where the code goes

```
  analyze.pl     <- the graph walk. every other rel-graph predicate is
                    already there. ~15 lines.
  compile.pl     <- one call, right after the existing "collect every rel"
                    step. ~4 lines. everything downstream narrows for free.
  0_graph.pl     <- already has the reachability walk. reuse it.
```

(The earlier recon put `program_plan` in `analyze.pl`. It is in `compile.pl`.
The line numbers it gave were right, the filename was not.)

There is also a 33-line working reference implementation of the harder half
(demand with bound arguments, the "magic set" transform) already sitting on
the shelf in this repo, executable, with three passing checks.

## The order to do it in

```
  0  keep the query's arguments alive through compilation      no behavior change
  1  compute the demand cone, use it for nothing               no behavior change
  2  prune statements, behind a flag                           first real win
  3  teach the reference engine the same cone                  keeps the referee honest
  4  prune the two relation-walking loops                      makes step 2 measurable
  5  fix the share() reset                                     one line, can go first
  6  timers subscribe per demand row, not per literal          the big one, ~80 lines
  7  the typed event source                                    needs a user ruling
  8  the second-event warning                                  small, high value
```

Steps 0, 1 and 5 are safe today. Step 7 is blocked on a decision. Step 6 is
the one that actually stops the machine from subscribing to everything, and
it is also the one most likely to leak an OS handle, so it goes late and
with the leak tests watching.

## Five questions for you

1. A typed external event source: new word, or sugar that expands to a bind?
2. "No query line" means everything runs, or nothing runs?
3. Second-event hazard: warning, or hard error?
4. Log rel wakes up and replays its history: which tick number do those
   events carry?
5. Eighteen program files have no query line at all, and several of them are
   rails whose whole point is a side effect. What does "demand" mean for a
   program that exists to make something happen rather than to answer
   something?
