# the lazy engine, in plain words

## today: everything runs, always

```
   world pushes a row
        |
        v
  +-----------+     every rule fires, every tick,
  |  one tick  | -->  whether anyone is looking or not
  +-----------+
        |
        v
   every table updated

   "? my_rel(x)."  --->  compiled into a note nobody reads; it runs nothing.
```

The query line is decoration. Timers tick, watchers watch, shells spawn,
all rules run. Nothing waits to be asked.

## the two arrows are not equally lazy

```
  <-  (level rule)          <+  (edge rule)
  ----------------          ----------------
  reads TABLES              reads THIS TICK'S CHANGES
  tables are on disk        changes are wiped at tick start
  wake up whenever          wake up late = you missed it
  recompute = same answer   nothing to recompute from
```

This is the whole problem. A level rel can sleep for a thousand ticks and
wake up correct, because its inputs are still sitting in the store. An edge
rel that sleeps through an event has lost that event forever, exactly like
an rxjs Subject with no subscriber.

That is why making level rules lazy is easy and mostly mechanical, while
making edge rules lazy is a real design choice.

## the goal: the query is the subscribe

```
   ? gate_fire(repo, bucket)          <- the ONLY on-switch
        |
        | walk backwards through the rules
        v
   +-------------------------------+
   | the cone: just the rules that |
   | can feed the thing you asked  |
   +-------------------------------+
        |
        v
   only THOSE rules run. only THOSE timers start.
   only THOSE shell commands spawn.
```

Rules outside every cone: compiled away. Timers outside: never start.
A rel nobody asks for is an empty table, sitting there, costing nothing.

## the sticky part: events that happen before anyone asks

Three choices for a row that arrives when no query wants it yet:

```
  DROP it        forget it happened        (pure rx, but the engine
                                            forgets world events it was told)

  BUFFER it      hold in memory, replay    (memory grows forever and
                 later                      timestamps come out wrong)

  WRITE it       always land the row in    (recommended. storage is cheap,
                 its table, just don't      the history is there, and only
                 COMPUTE anything yet       thinking is lazy.)
```

The rule behind this: getting an event through the door is a write, and a
write is not the same as evaluating anything. Thinking starts where a rule
reads the row, not where the row lands.

With WRITE: when a query finally shows up, level rules can read the whole
stored history. Edge rules (the "when X arrives, do Y" kind) only fire on
new arrivals from that moment on. Old events are readable, they just don't
re-trigger reactions. That matches how the language already talks about
late subscribers.

## the pre-commit machine

What was ruled: nothing happens until the first pre-commit. Then a pulse
every second, plus an extra pulse on every later pre-commit. One global,
lazily started, never resets.

```
  pre_commit ----+---- first one flips a latch: "armed"
                 |
                 |            armed?
                 v              |
             [ latch ] ---------+
                                |
   every 1s  -------------------+---> gate_fire
   later pre_commits -----------+---> gate_fire
```

In the language: pre_commit is just a declared typed relation the outside
world posts rows into. The latch is one edge rule. The pulses are two more
edge rules reading the latch.

## three ways to write the compose, and what the checker does to each

The shape: wait for the first pre-commit, then a pulse every second AND on
every later pre-commit, shared, never re-cold. Three spellings get you
there, and the checker grades them differently:

```
  pre_commit ----------------> gate_fire        (same tick)
  pre_commit --> armed ------> gate_fire        (next tick)
                 two paths, two speeds, ONE target

  A. keep it on the level plane (arm a gate, two level rules merge)
        -> compiles clean

  B. edge rules, read the latch bare
        -> REFUSED. two speeds into one head is a clock conflict.

  C. edge rules, read the latch as a snapshot, not bare
        -> compiles clean
```

Spelling B is the one the checker refuses today. The bare read of an
edge-written latch IS the extra step, so the two paths into the same target
arrive at different ticks and the checker calls that a clock conflict. Make
the same latch read a snapshot (C) or keep the merge on the level plane (A)
and the conflict is gone.

## the trap the checker has to watch

Write the gate as a set rel that only ever arms once, and a second pre-commit
can go silent with nobody complaining:

```
   pre_commit #1 ---> gate opens ---> fire        good
   pre_commit #2 ---> gate ALREADY open ---> nothing    <-- silent
```

Set rels swallow a row that is already there. The program compiles, the
clocks line up, and the second commit does nothing. That is usually the
wrong answer for a hook.

This is checkable, and the checker should make it a warning first, not an
error, because "only the first one" is sometimes exactly what an author
wants and the language has no other way to spell it yet.

## the open fork on going cold

What was ruled is the SHAPE of the example: one global, lazily started,
never reset. That was never a ruling about every relation in general. For
demanded sources generally, reset behavior is an open fork:

```
  NEVER reset         once a rel is asked for, it stays warm forever
  reset on last        go cold when the last reader leaves (the rxjs
  reader               default), re-warm on the next ask
  per-rel say-so       the author declares liveness per relation
```

No recommendation here; it still needs your word.

One real bug is visible through this fork, separate from it. The served
engine's tick stream uses bare share, which resets when the last reader
leaves and then refuses every later submit with "engine is not running".
Today that is hidden because the server holds a permanent subscription.
Under a design where the query IS the subscribe, it stops being hidden.

## what all the tests mean now

Old world: a test says "at the end, this table holds these rows", and
everything always ran, so that made sense.

New world: if nothing asked for the table, it's empty and the test lies.

Fix: the test harness treats every expectation as an ask. "You assert it,
you asked for it." Zero test files change, all 281 keep passing, and they
keep meaning exactly what they meant.

Plus one grandfather rule during migration: a program with no query lines
at all behaves like today, everything on. Laziness kicks in the moment
you write your first `?`. Later, once everything's proven, the default can
flip.

## what never changes

```
  - tables: all created up front, same schema, same store
  - the tick: one transaction, same protocol, same log format
  - arrivals: always accepted, always validated, always written
  - the rules' meaning INSIDE a cone: identical, byte for byte
  - the referee: the prolog oracle learns the same cone trick,
    so runtime and oracle still have to agree to the byte
```

## the landing order

```
  0. keep the query's arguments alive through compilation   (invisible)
  1. compute the cone, use it for nothing                   (read-only)
  2. prune statements, behind a flag                        (opt-in)
  3. teach the referee + tests the same trick               (meaning locked)
  4. prune the two relation-walking loops                   (makes 2 real)
  5. fix the tick stream's share reset                      (one line, first)
  6. timers/watchers/shells start only on demand            (the machine quiets)
  7. the typed event source                                 (needs a ruling)
  8. the second-event warning                               (closes the trap)
```

Steps 0, 1 and 5 are safe today. Step 7 is blocked on a decision. Step 6 is
the one that actually stops the machine from subscribing to everything, and
it is also the one most likely to leak an OS handle, so it goes late and
with the leak tests watching.

## still needs your word

- pre-demand events: write-always (recommended) vs drop vs buffer
- new event sources: reuse bind (recommended) vs a new keyword
- the checker: label the two-speed shape (recommended) vs refuse it
- first pre-commit: does it pulse in its own tick or the next one
- no-query programs: everything-on (recommended, for migration) vs nothing-on
- reset behavior in general: never-cold vs cold-on-last-reader vs per-rel
- a typed external event source: new word, or sugar that expands to a bind
- second-event hazard: warning, or hard error
- log rel wakes up and replays its history: which tick do those carry
- eighteen program files have no query at all, and several exist to make
  something happen rather than answer something: what does demand mean there
