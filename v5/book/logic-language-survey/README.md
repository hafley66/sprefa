# Logic Language Survey

You already live in the Datalog corner of this space: bottom-up, terminating,
set-semantics, no backtracking. This survey maps the rest of the territory so the
neighbours stop being names and start being points on two axes.

Read [01 — the six languages](01-the-six-languages.md) for the per-language detail.
This page is the frame.

## The two axes that explain almost everything

Every difference below collapses onto two questions.

**Axis 1 — which direction does the engine run?**

- **Top-down (SLD resolution).** Start from the *query*, work backwards toward
  facts, try clauses in order, backtrack on failure. This is Prolog. It is
  goal-directed and lazy: it only does work the query demands. The price is that
  clause order and recursion shape can send it into an infinite branch.
- **Bottom-up (semi-naive fixpoint).** Start from the *facts*, apply every rule,
  collect what is newly derivable, repeat until a pass adds nothing. This is
  Datalog, and it is exactly the loop in chapter 7. It is set-oriented and
  order-insensitive. The price is that it can derive things no query asked for.

```
   top-down (Prolog)            bottom-up (Datalog)
   query ?- reaches(main, X)    facts: call(main,run), call(run,parse), ...
        │                            │  apply rules
        ▼  unify, pick clause        ▼  derive every reaches(_, _)
   subgoal reaches(...)          fixpoint reached
        │  backtrack on fail         │
        ▼                            ▼
   walk down to facts            answer = filter the closure
```

**Axis 2 — how much does the compiler know before it runs?**

Plain Prolog knows nothing: no types, no modes, no determinism. Mercury demands
all three and turns them into near-C codegen. Soufflé demands types and turns
Datalog into parallel C++. Ciao lets you add knowledge *gradually* and checks it
by abstract interpretation. The more the compiler is told, the faster and safer
the code — and the less it feels like poking at a REPL.

## Where the six land

| | Prolog | Scryer | Datalog | Soufflé | Mercury | Ciao |
|---|---|---|---|---|---|---|
| Family | logic prog. | a Prolog | deductive DB | a Datalog | typed LP | a Prolog |
| Evaluation | top-down SLD | top-down SLD | bottom-up fixpoint | bottom-up, compiled | top-down, compiled | top-down (+ tabling) |
| Terminates by design? | no (Turing-complete) | no | **yes** (decidable) | **yes** | no | no |
| Function symbols / nested terms | yes | yes | no (classic) | records + ADTs | yes | yes |
| Typing | none | none | none (classic) | **static** | **static (HM)** | optional (assertions) |
| Modes / determinism | none | none | n/a | n/a | **required** | optional |
| Purity | impure (cut, assert) | cleaner, still impure | pure | pure | **pure** | impure (opt. pure) |
| Negation | as-failure | as-failure | **stratified** | **stratified** | (pure, sound) | as-failure / tabled |
| Compiles to | WAM bytecode | Rust VM | C++ source | native | native / C / Java / C# | bytecode + native |
| Primary use | general logic prog. | rigorous logic prog. | deductive database | program analysis at scale | systems & applications | LP + static analysis |

## The one-sentence each

- **Prolog** — the general-purpose, top-down, backtracking, untyped, impure root
  of the whole family.
- **Scryer** — Prolog done *correctly*: ISO-conformant, Rust-implemented, clean
  libraries, the rigor crowd.
- **Datalog** — Prolog with the dangerous parts removed (no function symbols, no
  order sensitivity) so it always terminates; your home turf.
- **Soufflé** — Datalog compiled to parallel C++ with a type system, built to run
  whole-program static analysis at industrial scale.
- **Mercury** — Prolog's syntax with static types, modes, and determinism made
  mandatory, compiled to near-C native code; a *language*, not a database.
- **Ciao** — Prolog you can gradually push toward Mercury, adding checkable
  assertions (types, modes, determinism, cost) verified by abstract interpretation.

## Why this lives in the sprefa book

`dl` is a bottom-up, stratified-negation, set-semantics Datalog engine welded to
SQLite. On the table above it sits in the **Datalog / Soufflé** column. Reading
the others tells you what you deliberately gave up (function symbols,
goal-directed laziness, Turing-completeness) to buy what you have (guaranteed
termination, incremental maintenance, a fixpoint you can cache on disk). The
[onramp section](01-the-six-languages.md#an-onramp-from-datalog) at the end of
chapter 01 walks the bridge in both directions: tabling makes Prolog behave more
like your engine, and magic sets make your engine behave more like Prolog.
