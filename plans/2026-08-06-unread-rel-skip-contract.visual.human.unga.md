# THE UNOBSERVED-REL SKIP, in plain words

## The waste, in one picture

Every derived rel writes its new rows THREE extra times: once into an event
log, twice into mailboxes. Downstream rules read the mailboxes. Live viewers
read the event log.

```
        derived rows this tick
                 |
        +--------+--------+--------+
        |        |        |        |
      the rel  event    mailbox  mailbox
      table     log      (now)   (next tick)
                 |         |        |
              nobody    nobody   nobody      <-- in the benchmark
```

In the benchmark program `reachable` has no downstream rule and no viewer.
The three copies still get written, then thrown away.

## What it costs

| program | event log copy | mailbox copy | total wasted |
|---|---|---|---|
| chain 10,000 | 8.9% | 5.8% | **14.7%** |
| grid 10,000 | 15.0% | 10.1% | 25.1% |
| layered 10,000 | 14.9% | 9.8% | 24.7% |

Roughly a seventh to a quarter of the whole fixpoint, spent writing rows that
nothing ever reads.

## Who could know "nobody reads this"

Two different people know two different halves, and neither knows both.

```
  THE COMPILER knows:            THE CALLER knows:
  which RULES read the rel       whether a HUMAN or program
  (it can see every rule body)   is watching the rel's output
        |                                |
        +--------------+-----------------+
                       |
              both empty -> skip the copies
              either full -> write them
```

The compiler cannot know the second half. The caller cannot know the first.
So the decision is made where they meet: at startup.

## The four options, and why three lost

```
1. COMPILER DECIDES ALONE
   -> loses. The only thing the compiler can measure is the program's
      "?" queries, and 206 out of 211 real programs have none at all.
      It would mark almost everything as unwatched and delete output
      the test suite checks.
      Also: it would edit the exact file another lane is rewriting
      right now.

2. COMPILER MARKS, CALLER DECIDES AT STARTUP        <-- WINNER
   -> the compiler ships a little list on each rel ("these rules read me").
      The caller says which rels it watches. Empty on both sides, skip.

3. CHECK EVERY TICK WHO IS WATCHING
   -> loses. There is nothing to check. The server hands the whole tick
      report back to whoever sent the input. No per-rel subscriber list
      exists to count.

4. SKIP, THEN TURN BACK ON AND REPLAY THE MISSED HISTORY
   -> loses on the replay. The event log is wiped clean at the start of
      every tick, and it records "+row" and "-row", not just what exists
      now. A row that appeared and vanished while we were skipping leaves
      no footprint anywhere. You cannot reconstruct it. Ever possible?
      No.
```

## What a latecomer gets instead

Not the missed history. The current contents.

```
   viewer shows up late
          |
          +--> "what happened while I was gone?"   -> cannot answer, refused
          +--> "what is true right now?"           -> full snapshot, always available
```

And "showing up late" only really happens one way: the server loads a new
program, which tears the old one down completely and starts fresh. That
restart IS the moment the caller re-declares what it watches. So there is no
mid-flight switch to manage.

## The trap that almost broke it

The engine decides "do I need another tick to settle?" by peeking at the
next-tick mailbox. If it is empty, stop ticking.

Skip the mailbox write, and the engine thinks it has settled early. Whole
ticks disappear from the output.

```
   BEFORE:  wrote 5 rows to mailbox -> mailbox not empty -> keep ticking
   NAIVE:   wrote nothing           -> mailbox empty     -> STOP. WRONG.
   FIX:     wrote nothing, but we already counted 5 rows on the way in
            -> remember "5" -> keep ticking. Same answer, no write.
```

The count is free: the statement that fills the staging table already reports
how many rows it filled. Nobody has to count anything twice.

## How we catch it if the analysis is wrong

The scary failure is quiet: we decide nothing reads a rel, something does,
and the program just produces fewer answers with no error at all.

Three nets, cheapest first:

```
NET 1 (free, runs on all 420 test programs, executes nothing)
  For every rel marked "unread", read the generated program as TEXT and
  search for anything mentioning that rel's event log or mailbox.
  Found one that isn't a writer? Red. The analysis missed a reader.
  This works even if we forgot a whole category of reader, because it
  checks the analysis against the actual generated code, not against a
  second copy of the same reasoning.

NET 2 (landing gate)
  Run every test program twice, once with the skip off, once with it on.
  Same output, same number of ticks. Any difference, red.

NET 3 (standing test)
  Count the statements per tick. If the skip is on and the count did not
  drop by exactly the expected amount, the skip silently isn't happening.
```

## Does this break the 420 golden test programs

No, and not by luck.

The test suite never declares which rels it watches, and "not declared" means
"I watch everything". So for all 420 programs the skip is switched off,
nothing changes, and the output is identical because it is literally the same
code path running the same statements. The optimization only wakes up when a
caller explicitly says "I am not watching these".

## The other lane

Another lane is rewriting how recursive rules maintain themselves, and it owns
the file where these copy statements are built. This design deliberately does
not touch that file. It removes copies by NAME ("the mailbox write"), not by
what table they read from, so whatever the other lane changes underneath, the
skip still points at the right thing.

One handshake needed: if that lane deletes the staging table, the free row
count from the trap-fix above needs a new source. Named, not hand-waved.

## The work, in order

```
  step 1  compiler: work out which rules read each rel, ship the list
            |
  step 2  runtime: skip the writes when both halves are empty,
          plus the tick-count fix
            |
  step 3  rails: the three nets above
```

Three separate people can do these. Step 1 and step 2 touch no shared file.
Step 3 waits for step 1.

## Name

Was called "unreadRels". The rxjs word for a stream with no subscribers is
`observed`, so: **the unobserved-rel skip**. A rel nobody observes does not
get its rows copied anywhere.
