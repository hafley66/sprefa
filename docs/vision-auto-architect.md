# Auto-architect: the vision doc

Written 2026-07-18, capturing a planning morning. This is the human-readable
statement of where the tool is going, so future sessions (and the author,
who keeps not looking at this codebase) can find the thread again. Pointers
to the concrete plans and instruments are at the bottom.

## Thesis

`dl` should be a tool you point at ANY repo — including this one, which is
where everything gets validated first — and it tells you, from extracted
facts and measured structure, things like:

- "this folder really loves this library" (affinity)
- "this folder really loves that folder all the way over there" (misplaced
  coupling; a refactor seam)
- "these functions are transitively effectful — SQL, sockets, syscalls,
  prints — and here is the path from an entrypoint that makes them so"
- "this lock is held across that blocking call, and these two locks form an
  ordering cycle"
- "these Go channels are unbounded and reachable from an entrypoint"

None of these are mandates. Every finding is advisory; some measured
"problems" will not matter at all, and the user's whim is the final
authority. The tool's job is to make the structure VISIBLE and QUERYABLE so
the whim is informed.

## The program is its own DAG

An Airflow-style DAG is the right mental anchor: a `dl` program literally
declares one. Derived rules are the pure nodes; the impurity is explicit and
typed — `@async` effects, `@next` carry, `clock`/`every` — impure edges we
deliberately added to the language. So the engine does not need to be TOLD
the dataflow of an analysis pipeline; the program IS the dataflow, and the
scheduler/staging work (jobq, cold staging, resource-aware scheduling) is
about executing that DAG without lying about physics (CPU, IO, memory,
locks).

## The capability ladder

Each tier builds on the ones below. Tiers 1-2 are landed; 3-5 are the arc.

1. **Facts** — extraction families (type/call/df/module/comment/scip...).
   Completeness matters here: every callable kind in every language
   (EntityKind::Lambda, constructors — "a ctor is philosophically always a
   fn call, behind the parens"), because taint and reachability are only as
   good as the callable registry. Two tiers of truth: the index-free "diet"
   AST tier and the compiler-truth scip tier, pairable by (file, line,
   name).
2. **Measures** — fan_in/fan_out/blast/cycles/reach (`std/measures.dl`,
   top-K views). Kept ALL of them: they are research instruments; overlap
   between them is data.
3. **Coloring** — effect classification as transitive reachability: seed a
   per-repo table of effect roots (SQL calls, std::net/fs/process, prints,
   `.lock()`, channel ops — see `docs/effect-inventory.md`) and close over
   `calls_reach`. Generic across codebases: swap the seed table, keep the
   closure. "Any usage of a library" is itself an effect surface — the
   crate-usage map is part of the same inventory.
4. **Temporal/interval analysis** — the "left-right parentheses over time":
   - **Locks (Rust-unique win)**: RAII makes the parens static — guard
     binding is `(`, scope exit is `)`. Two queries: a guard held across a
     blocking/awaiting call (clippy's `await_holding_lock` does the local
     case; ours can be interprocedural from entrypoints), and the lock
     ORDERING graph (A held while acquiring B) whose cycles are deadlock
     candidates — kernel lockdep's model.
   - **Channels (Go)**: capacity at `make(chan T, n)` sites + send/recv
     reachability; unbounded/unbuffered reachable from an entrypoint
     without a bounding select is a finding.
5. **Auto-architecture** — suggestions synthesized from 1-4: refactor seams
   ranked by measured coupling AND library affinity (instantiation sites x
   call cardinality x locality); "this code only matters while this thing
   is alive" expressed as struct-scoped modules (measured, no DI ceremony);
   entrypoint-rooted construction topology (ctor tree + assignment tree =
   projections of df_edge once ctor call_defs exist).

## Not inventing this (SOTA anchors)

| Ours | Established |
|---|---|
| rels + datalog over code | CodeQL (relational db + QL), Glean (facts + Angle) |
| ctor-is-a-call | CodeQL `ConstructorCall` |
| ctor/allocation relations | Doop allocation-site-sensitive points-to |
| entrypoint-rooted taint | Pysa models, FlowDroid, CodeQL taint configs |
| all families joinable | Joern code property graph (AST+CFG+PDG) |
| lock ordering cycles | kernel lockdep |
| diet tier + scip tier | Glean mixed indexers; stack-graphs over SCIP/LSIF |
| reactive incremental loop | differential dataflow / Glean incremental (our least-commodity part) |

## Validation law

Dogfood first: every capability validates on sprefa itself before it is
trusted anywhere else. Then the fleet: the tool is meant for "a fuckload of
repos". A capability that only works here is not done.

## Where everything lives (as of 2026-07-18)

- Callable completeness arc (Lambda, ctors, @callable self-verifying
  fixture rail, scip pairability): branch `callable-lambda-ctor` in flight;
  audit matrix `docs/callable-coverage.md`.
- Dataflow coverage per language: `docs/df-coverage.md` (TS class-method
  fix landed 60f0847a).
- Measures: `std/measures.dl` + verdicts in `docs/arch-measures-review.md`.
- Effect/lock/channel/crate-usage seed inventory:
  `docs/effect-inventory.md` (kimi, in flight).
- Resource-aware scheduler plan ("physically correct": jobs declare
  read/write scopes; frontier selection; conflict-serializability):
  `plans/2026-07-18-resource-aware-scheduler.md` (in flight).
- Decomposition/normalization plan (typegraph split, affinity axis,
  no-file-over-1500-lines): `plans/2026-07-18-decomposition-normalization.md`
  (in flight).
- Cold staging + work-chunking (landed):
  `plans/2026-07-17-cold-start-staging.md`.
- Effects/locks/channels ANALYSIS plan: queued (next planning slot),
  consumes the seed inventory.

## Non-resident reactivity (the memo-graph question, 2026-07-18)

Goal at hundreds-of-repos scale: the reactivity/memo graph must not assume
RAM. The canonical split: the DERIVATION GRAPH (rel/task-level trigger
edges — tiny, hundreds of nodes, persistable metadata) vs the MEMO TABLE
(values — big, lives in SQLite). Row-level lineage is never stored:
semi-naive deltas + either DRed (delete-rederive, pure SQL) or
counting/Z-set weights (DBSP algebra: retraction = weight -1, weights live
in the table). Differential dataflow supplied the math, not the runtime —
its arrangements are RAM-resident; the DB-hosted lineage is DBToaster/IVM.

Sprefa's engine is ALREADY the non-resident shape: SQL fixpoint (values on
disk), drv:/src: digests (persisted trigger graph, coarse grain), dirty-rel
scoping (delta propagation), scoped rebuild (coarse DRed),
_derived_complete (crash-safe frontier). The arc is a SPLIT, not an
invention: extract **derive-core** (rel graph + digests + fixpoint +
retraction + jobq/staging) as a generic library; code extraction becomes
its first client. Grain upgrade inside the arc: per-rel weight column buys
row-grained retraction with zero residency.

The honest tradeoff (per-rel knob, not global): zero stored view + correct
retraction + no recompute — pick two. Sharding law: one SQLite per root,
attach/detach = LRU non-residency for free; cross-repo edges are one more
rel layer in a coordinator db.

## Open threads

- Lock interval analysis needs guard-lifetime df edges (drop points) —
  design question for the effects plan.
- Affinity thresholds: when is "folder loves library" a finding vs noise —
  probably top-K views like the measures, never absolute cutoffs.
- Cross-repo operation (the fleet use) leans on the multi-root daemon;
  boot-cost work (staging/chunking) was the prerequisite, now landed.
