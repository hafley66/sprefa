# Reactive Fixpoint Marbles

This file runs one world through Prolog, tabling, Datalog, semi-naive
evaluation, incremental tabling, DBSP, and DL7.

## 1. The shared problem

```prolog
reachable(X, Y) :- follows(X, Y).
reachable(X, Z) :- reachable(X, Y), follows(Y, Z).
notify(X, Z)    :- reachable(X, Z), online(Z).
```

Names used in the marbles:

```text
AB = follows(ada, bob)
BC = follows(bob, cy)
OC = online(cy)

RAB = reachable(ada, bob)
RBC = reachable(bob, cy)
RAC = reachable(ada, cy)

NBC = notify(bob, cy)
NAC = notify(ada, cy)
```

The outside world changes in this order:

```text
time       t0             t1             t2             t3
           |              |              |              |
input  ----[+AB]----------[+BC]----------[+OC]----------[-BC]--->
```

The mathematically expected derived changes are:

```text
time       t0             t1             t2             t3
           |              |              |              |
reach  ----[+RAB]---------[+RBC,+RAC]------------------[-RBC,-RAC]->
notify ----------------------------------[+NBC,+NAC]----[-NBC,-NAC]->
```

Every system below computes some view of this same result.

## 2. The common machine

```text
                 read dependency
                       |
                       v
base facts ---> [ rule body ] ---> [ rule head ] ---> derived facts
     ^                                                    |
     |                                                    |
     +---------------- recursive feedback ----------------+

Stop condition: one complete pass derives no previously unseen fact.
```

The systems differ in when this machine runs and what state survives between
runs.

## 3. Plain Prolog: queries pull through the world

`Q` means somebody asks `notify(X, cy)`.

```text
world  ----[+AB]----------[+BC]----------[+OC]----------[-BC]--->
query  ----------Q--------------Q--------------Q--------------Q-->
result ----------{}-------------{}-------------{NBC,NAC}------{}->

work            search          search          search          search
state kept      choice stack    choice stack    choice stack    choice stack
```

Each query starts goal-directed proof search against the facts present at that
moment. Backtracking enumerates zero or more proofs. Completed query search
does not retain a reusable recursive answer relation.

## 4. Tabled Prolog: queries pull and answers remain cached

```text
world  ----[+AB]----------[+BC]----------[+OC]----------[-BC]--->
query  ----------Q--------------Q--------------Q--------------Q-->
table  ----------{}=============stale==========stale==========stale>
                create       base changed   base changed   base changed
manual  -----------------[abolish/requery]--[abolish/requery]------>

dependency table:

notify/2 ---> reachable/2 ---> follows/2
    |                              ^
    +---------- online/1           |
                   reachable/2 ----+
```

Tabling stores calls and their answers. Recursive calls that repeat become
dependencies on the existing table. A strongly connected recursive group is
complete when it produces no new answers.

Ordinary tables require invalidation or abolition after mutable base facts
change. Incremental tabling supplies that maintenance path.

## 5. Incremental tabling: changed facts wake dependent tables

```text
world  ----[+AB]----------[+BC]----------[+OC]----------[-BC]--->
dirty  ----reachable------reachable------notify---------reachable-->
         \___________________|______________|_______________/
                             v              v
tables ----[+RAB]---------[+RBC,+RAC]----[+NBC,+NAC]----[-RBC,-RAC]->
notify ----------------------------------[+NBC,+NAC]----[-NBC,-NAC]->

wake path at t3:

-BC -> reachable/2 -> notify/2 -> table completion
```

The dependency graph determines which answer tables need maintenance. The
implementation may invalidate and rederive affected answers rather than
algebraically transform every operator by a signed input delta.

## 6. Datalog: one snapshot enters a global fixpoint

Assume the database snapshot at `t2` contains `{AB, BC, OC}`.

```text
fixpoint round   r0             r1             r2          r3       r4
                 |              |              |           |        |
new facts     AB,BC,OC ---> RAB,RBC ---> RAC,NBC ---> NAC --->     {}
all rules        scan           scan           scan        scan     stable
```

The relation dependency graph determines legal evaluation order:

```text
follows ----> reachable ----> notify
                  ^              ^
                  |              |
                  + recursive    +---- online
```

The fixpoint clock `r0, r1, r2` is internal evaluation time. The outside-world
clock `t0, t1, t2, t3` is a separate timeline.

## 7. Semi-naive Datalog: each round reads only the new marbles

The final closure matches ordinary Datalog. The work schedule carries deltas:

```text
round       r0             r1             r2          r3       r4
delta   +AB,+BC,+OC ---> +RAB,+RBC ---> +RAC,+NBC ---> +NAC ---> {}
            |               |              |           |
            +------ old state joins with each new delta --------+
```

`delta` here means newly discovered facts inside one fixpoint computation.
Retractions across outside-world time require an incremental maintenance
system around this evaluator.

## 8. DBSP: outside-world changes remain signed changes

```text
time        t0             t1             t2             t3
input   ----[+AB]----------[+BC]----------[+OC]----------[-BC]--->
             |              |              |              |
             v              v              v              v
closure ----[+RAB]---------[+RBC,+RAC]------------------[-RBC,-RAC]->
             |              |              |              |
             v              v              v              v
filter  ----------------------------------[+NBC,+NAC]----[-NBC,-NAC]->
             |              |              |              |
state       {RAB}       {RAB,RBC,RAC}  {RAB,RBC,RAC}     {RAB}
```

Each marble carries an integer weight:

```text
+AB means weight +1
-AB means weight -1
```

Operators maintain indexed state and transform weighted input changes into
weighted output changes. Insertions and retractions use one algebra.

## 9. Current DL7 comptime: a Datalog closure per compilation

```text
source text
    |
    v
reader facts + type edges + rules
    |
    v
compile round r0 ---> r1 ---> r2 ---> stable
    |                                  |
    |                                  +--> frozen compiler rows
    +-------------------------------------> checked runtime program
```

Current V7 corresponds to one Datalog snapshot and its internal fixpoint
rounds. Editing a source file starts another compilation closure.

```text
edit time    t0                 t1                 t2
source   ----edit---------------edit---------------edit---->
compile  ----[r0 r1 r2 |]-------[r0 r1 |]----------[r0 |]-->
output   ------------C0----------------C1---------------C2-->
```

## 10. Durable DL7 runtime target: DBSP outside, fixpoints inside

```text
outside time       t0              t1              t2
                    |               |               |
input diff      ----+rows-----------rows-----------+rows---->
                    |               |               |
                    v               v               v
tick queue      ===[ batch 0 ]=====[ batch 1 ]=====[ batch 2 ]===>
                    |               |               |
                    v               v               v
recursive work     r0-r1-|         r0-r1-r2-|      r0-|
                    |               |               |
                    v               v               v
output diff     ----+rows-----------rows-----------+rows---->
```

Two clocks coexist:

```text
t = durable outside-world tick
r = internal recursive fixpoint round within one tick
```

The tick may publish only after its recursive groups become stable. That gives
downstream rules one coherent delta batch.

## 11. One cross-system legend

```text
Prolog             pull proofs now
Tabled Prolog      pull proofs, retain calls and answers
Incremental table  wake retained answers through dependencies
Datalog            close every rule over one snapshot
Semi-naive Datalog close one snapshot using new-fact deltas
DBSP               maintain results from signed changes over outside time
DL7 comptime        run one checked Datalog closure per compilation
DL7 runtime target propagate durable changes, stabilize, then publish
```

The shared center is:

```text
facts + relations + dependencies + recursion + completion
```

The principal time distinction is:

```text
query time       when an answer is demanded
fixpoint time    rounds required to complete recursive consequences
update time      when the outside world inserts or retracts facts
```
