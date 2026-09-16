# 1. The six languages

The [README](README.md) gave you the two axes (run direction, compiler
knowledge) and the master table. This chapter is the per-language detail: what
each one actually is, the feature that defines it, and what it costs.

A running example to keep them honest — the same `reaches` question from the rest
of the book, "who can main call, transitively?", over the call edges
`main→run→parse→lex`, `lex→run`, `run→log`.

---

## 1.1 Prolog — the root

**What it is.** The general-purpose logic language. You write Horn clauses (facts
and rules), you ask a query, and the engine answers by **SLD resolution**:
top-down, depth-first, left-to-right, with backtracking on failure.

```prolog
reaches(X, Y) :- call(X, Y).
reaches(X, Z) :- call(X, Y), reaches(Y, Z).

?- reaches(main, What).
```

**The defining trait.** It is goal-directed and *Turing-complete*. That power is
also the trap: clause order and recursion shape matter. Write the recursive
clause first, or carry an unbound variable into a left-recursive call, and the
same logic loops forever. Plain Prolog over the example above **does not
terminate** — `lex→run` is a cycle, and SLD will chase it down infinitely unless
you add tabling (see 1.3 and the onramp).

**The sharp edges.**

- **Cut (`!`)** — prunes the search tree. A control-flow operator inside a
  declarative language; necessary in practice, corrosive to purity.
- **`assert/retract`** — mutate the clause database at runtime. Side effects
  dressed as logic.
- **Negation as failure** — `\+ Goal` means "I could not prove Goal", which is
  only sound when `Goal` is ground. Easy to get wrong.
- **Unification without occurs-check** by default — fast, occasionally unsound.

**Who it is.** SWI-Prolog (the batteries-included pragmatic default), SICStus
(fast, commercial), GNU Prolog, YAP, Trealla. Use it when you want interactive,
exploratory, general logic programming with a real library ecosystem.

---

## 1.2 Scryer Prolog — Prolog done correctly

**What it is.** A Prolog. Same SLD-resolution evaluation model as 1.1. It earns a
separate entry not for a different engine but for a different *attitude*: ISO
conformance, a Rust implementation, and libraries designed so you rarely need the
impure escape hatches.

**The defining trait.** Rigor. Strings are lists of characters (not an opaque
type), constraints come through `library(clpz)` instead of low-level arithmetic,
DCGs are first-class, and the culture (the "Power of Prolog" / Markus Triska
orbit) pushes you toward pure, terminating, mode-general predicates and
three-valued reasoning with constraints rather than `!` and `assert`.

**What it costs.** Smaller ecosystem than SWI, fewer turnkey libraries (no
batteries-included HTTP stack of SWI's scale). You trade breadth for correctness
and a cleaner mental model. Pick it to learn what *clean* Prolog feels like, or
when correctness of the logic matters more than library count.

---

## 1.3 Datalog — Prolog with the dangerous parts removed

**What it is.** A syntactic *restriction* of Prolog evaluated **bottom-up**. Drop
function symbols (no nested terms, so the Herbrand base is finite), drop clause
ordering, require rules to be range-restricted, and you get a language that
**always terminates** and is **decidable**. Evaluation is the semi-naive fixpoint
from chapter 7, not backtracking search.

```
reaches(X, Y) :- call(X, Y).
reaches(X, Z) :- call(X, Y), reaches(Y, Z).
```

Identical clauses to the Prolog version — but run bottom-up, the `lex→run` cycle
is no problem at all: the fixpoint just stops adding facts once the closure is
saturated. This is the whole reason your engine handles cycles where naive Prolog
hangs.

**The defining trait.** Termination and set semantics, bought by giving up
function symbols and goal-directed laziness. Data complexity is PTIME. Negation is
**stratified** — you may only negate a relation computed in an earlier stratum, so
there is no `\+`-style unsoundness (chapter 3).

**What it costs.** No structured terms (classic Datalog), no laziness — it
computes the *whole* relation whether or not the query needs all of it. (Magic
sets, in the onramp, claw some of that laziness back.)

**Who it is.** The academic core, plus Soufflé (1.4), plus your `dl`, plus LogicBlox,
Datomic-style stores, and DDlog. This is your column.

---

## 1.4 Soufflé — Datalog compiled for scale

**What it is.** A Datalog. Same bottom-up semi-naive fixpoint and stratified
negation as 1.3 — but it **compiles the program to parallel C++** (or runs a fast
interpreter), and it adds back exactly the features industrial static analysis
needs without breaking termination.

**The defining trait.** It makes Datalog fast and typed enough for real
whole-program analysis. Over plain Datalog it adds:

- a **static type system** (primitive types plus **records and ADTs** — structured
  data, reintroduced carefully),
- **components** (parametric modules) for reusable rule sets,
- **aggregates** (count / sum / min / max),
- **subsumption** rules (delete dominated tuples — lattice-like reasoning),
- **user-defined functors**, **choice domains**, and magic-set / index auto-tuning.

**Who uses it.** The points-to / Doop community; it is the de-facto engine for
declarative program analysis at scale. It is the closest existing system to what
`dl` is reaching for — read it as the mature sibling of your own engine. The
difference: Soufflé compiles to a static binary; `dl` is reactive and incremental
over SQLite, keeping the fixpoint on disk and maintaining it under edits.

---

## 1.5 Mercury — Prolog's face, a systems language's body

**What it is.** It *looks* like Prolog. It is not a database and not interactive in
the REPL-driven sense. It is a purely declarative language that compiles to native
code (also C, Java, C#) at **near-C performance**, and it demands three kinds of
declaration Prolog never asks for:

```mercury
:- pred reaches(string::in, string::out) is nondet.   % mode + determinism
:- pred main(io::di, io::uo) is det.                  % I/O threads a unique state
```

**The defining trait.** Mandatory static knowledge, in three layers:

- **Types** — Hindley–Milner style; every predicate's arguments are typed.
- **Modes** — which arguments are input (`in`) and which are output (`out`) at a
  call. The compiler uses this to reorder goals into an efficient execution order,
  so *you* are freed from Prolog's clause-ordering tyranny.
- **Determinism** — `det`, `semidet`, `multi`, `nondet`, `failure`, and the
  committed-choice variants, declaring how many solutions a call can have. The
  compiler verifies your claim and optimizes against it.

It is **pure**: no `assert/retract`, no cut. I/O is done by threading a unique `io`
state, so side effects are tracked in the type system instead of escaping it.

**What it costs.** You lose the "type a clause, ask a query" exploration loop. You
write a program, declare modes and determinism, and compile. The ecosystem is tiny
and slow-moving (the famous real deployment is the Prince XML / YesLogic
HTML-to-PDF engine). Treat it as a language to **steal ideas from** — modes and
determinism are a genuinely good way to think about *any* relation, including your
Datalog rules — more than one to bet a project on.

---

## 1.6 Ciao — Prolog you can gradually make safe

**What it is.** An ISO Prolog with the same top-down SLD core (optionally tabled),
distinguished by an **assertion language** and a static analyzer (**CiaoPP**) built
on abstract interpretation.

**The defining trait.** Gradual guarantees. You start with ordinary untyped Prolog,
then *optionally* add assertions about types, modes, determinism, non-failure, or
even **cost** (resource bounds), and CiaoPP either proves them at compile time by
abstract interpretation or inserts runtime checks where it cannot. It is the bridge
between 1.1 (no static knowledge) and 1.5 (all static knowledge, mandatory): Ciao
lets you dial the knowledge up where it pays and leave it off where it does not.

```prolog
:- pred reaches(X, Y) : (var(X), var(Y)) => (atm(X), atm(Y)).   % an optional assertion
```

**What it costs.** The analysis is powerful but the system is academic — steeper
than SWI to onboard, smaller community. Pick it when you want Prolog *plus*
machine-checked properties without rewriting into a different language.

---

## An onramp from Datalog

You arrived from the Datalog side. Two bridges connect your world to the Prolog
world, and both are worth knowing because they show the engines are the same idea
run in opposite directions.

**Bridge 1 — tabling makes Prolog behave like your engine.** Add `:- table
reaches/2.` in SWI or XSB and Prolog stops re-deriving and stops looping: it
memoizes each subgoal's answers and detects when a recursive call is already in
progress. With tabling, the cyclic `reaches` example that hung in 1.1 now
terminates with exactly the closure your bottom-up fixpoint computes. Tabled
resolution with well-founded semantics (XSB's specialty) is, in effect,
bottom-up evaluation wearing a top-down interface — stratified negation and all.
If the Datalog model is what you like, **XSB and SWI-with-tabling are the Prologs
that think the way you do.**

**Bridge 2 — magic sets make your engine behave like Prolog.** Pure bottom-up
computes the *entire* `reaches` relation even when you only asked
`reaches(main, X)`. The **magic-sets** transformation rewrites the rules so the
fixpoint only derives facts relevant to the query — reintroducing top-down
goal-direction into a bottom-up engine. Soufflé does this; it is the standard way
deductive databases buy back the laziness they gave up in 1.3.

The picture to keep:

```
            top-down                         bottom-up
   ┌──────────────────────────┐   ┌──────────────────────────────┐
   │ Prolog, Scryer, Mercury, │   │ Datalog, Soufflé, your `dl`   │
   │ Ciao                     │   │                               │
   └──────────────────────────┘   └──────────────────────────────┘
              │    tabling  ───────────▶  (memoized = bottom-up-ish)
              ◀───────────  magic sets  │  (goal-directed = top-down-ish)
```

The two families are not rivals. They are the same least-fixpoint computation
approached from the query end or the fact end, and each has learned to borrow the
other's best trick.
