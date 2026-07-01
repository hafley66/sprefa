# 2. Recursion and the fixpoint

**The question:** "who can `main` reach, following calls as far as they go?" is
not a fixed-depth join. How do you express and compute "as far as they go"?

## Reachability is a rule that uses itself

One direct call is reach of length 1. A reach plus one more edge is a longer
reach. Write exactly that:

```
reaches(X, Y)  :-  calls(X, Y).                  -- base: a direct call
reaches(X, Z)  :-  reaches(X, Y), calls(Y, Z).   -- step: extend by one edge
```

The second rule has `reaches` in both its head and its body. That is recursion.
It does not say "loop"; it says "this is also true." The engine's job is to find
all facts that make both rules simultaneously true.

## The fixpoint: keep applying rules until nothing new appears

Compute it by rounds. Start empty. Apply the rules. Repeat. Stop when a round
adds nothing.

```
round 0:  reaches = {}
round 1 (base):     main→run, run→parse, run→log, parse→lex, lex→run
round 2 (step):     + main→parse, main→log, run→lex, parse→run, lex→parse
round 3 (step):     + main→lex, run→run, parse→parse, lex→lex, parse→log, lex→log,
                      run→parse(already), ...
round 4 (step):     + main→main? no. nothing new.
STOP.
```

The set stopped growing. That stable set is the **least fixed point**: the
smallest relation that satisfies both rules. "Least" matters — it contains
exactly the facts forced by the rules, nothing invented.

Notice `run→run`, `parse→parse`, `lex→lex` appeared. Those are nodes that reach
themselves. That is the cycle `run→parse→lex→run` making itself visible, and it
is the subject of Chapter 3.

## Why it terminates

The set of possible facts is finite (pairs of known functions). Each round only
adds facts, never removes them. A set that only grows inside a finite universe
must stop. So a positive recursive datalog program (no negation in the cycle)
always reaches a fixpoint. Add negation inside recursion and this guarantee can
break, which is why engines require **stratification** (compute a negated
relation fully before the rule that negates it).

## Semi-naive: only chase the new frontier

The rounds above recompute known facts wastefully. The fix is the single most
important performance idea in datalog: each round, only join the facts that were
**new last round** (the frontier / delta) against the edges.

```
frontier₀ = base reaches
loop:
   new = { (X,Z) : (X,Y) in frontierₙ, calls(Y,Z) } minus already-known
   if new is empty: stop
   add new to reaches
   frontierₙ₊₁ = new
```

This is **semi-naive evaluation**. Work each round is proportional to what
changed, not to the whole relation. You already wrote this: your `dl` fixpoint
loop runs the recursive INSERT until a pass adds zero rows, and your reactive
`reaches` is a frontier expansion one hop at a time.

```
frontier expands outward, one hop per round, until it stops growing:

   {main} ─▶ {run} ─▶ {parse,log} ─▶ {lex} ─▶ {run*} ─▶ (seen) ─▶ done
                                                  *cycle re-enters, but
                                                   "minus already-known" halts it
```

## Intuition

> Recursion is a rule that is also true of its own output. The answer is the
> least fixed point: apply rules until a round adds nothing. It terminates
> because facts only accumulate in a finite universe. Compute it semi-naively:
> each round, chase only the frontier (last round's new facts), not the whole
> relation.

## Exercises

1. Compute `reaches("main", _)` fully. Which functions can main reach?
2. Which functions reach themselves (have `reaches(X, X)`)? What do they have in
   common in the picture?
3. `log` is a sink. What is `reaches("log", _)`?
4. Run semi-naive by hand for `reaches("parse", _)`: write the frontier at each
   round until it is empty.

## In your engine

`callgraph-ast.dl` has exactly these two rules for `reaches`. Your engine
materializes the fixpoint into a SQLite table by looping the derived INSERT until
`changes == 0` (that zero-change stop is the fixpoint test). The "minus
already-known" that makes the cycle terminate is your `INSERT OR IGNORE` on the
primary key: a re-derived row is silently dropped, so a pass over a cycle
eventually adds nothing and the loop ends.

## Answers

1. main reaches run, parse, log, lex (and via the cycle, itself is not added
   because no edge leads back into main). So {run, parse, log, lex}.
2. run, parse, lex reach themselves — exactly the three nodes on the cycle. A
   node reaches itself iff it sits on a directed cycle.
3. Empty. log calls nothing, so it reaches nothing.
4. f0 = {lex} (parse→lex). f1 = {run} (lex→run). f2 = {parse, log} (run→parse,
   run→log). f3 = {lex} but lex already known → dropped; {} new from log. Stop.
   reaches("parse",_) = {lex, run, parse(self, via parse→lex→run→parse), log}.
