---
name: sprf-control-flow-as-ops
description: [v4 planning] Why next/await/if are op calls in sprefa, not keywords. Gen-only events with auto-retract, within-time fan-out, branching as if(${COND}, ${THEN_PIPE}, ${ELSE_PIPE}). Load when designing the parser, lower, or any "control flow" feature in the language.
---

# Control flow as ops, not keywords

## The thesis

`next` and `await` are entries in the op registry. `if` is too. No keywords are added to the grammar. Every "control flow" feature falls out of join/antijoin/inspect_batch on a gen-only collection.

## Why no keyword

The JS/Python reason for `yield` being a keyword is the continuation transform: yield mid-function turns the function into a generator, and the parser must know this to compile differently.

In DD's model there is no suspending control. Rule body is a linear pipe of ops. Each op is a transformation `Collection → Collection`. An `await`-shaped op is just a join against a transient bag, gated by an antijoin against a resolved bag.

```rust
// await(:e, ${A?}, ${B?}) lowers to:
c.map(|row| (row[deps_key], row))
 .join_map(events_e.map(|ev| (ev[deps_key], ev)),
           |k, prefix, ev| merge_captures(prefix, ev))

// tag?(:r, ${A?}, ${B?}) lowers to (the same shape):
c.map(|row| ...).join_map(tag_r_arranged, ...)
```

Same parser. Same lowering. Two more entries in the registry.

## Events as gen-only collections

Two implementations.

**Impl A — one DD timestamp per gen (default)**

```
runner per gen:
  T = current_gen
  for each event_emit at T:
      input_event.update_at((row, kind), T,    +1)
      input_event.update_at((row, kind), T+1,  -1)   // scheduled retract
  advance_to(T+1); flush; step_while(probe < T+1)
```

All consumers at time T observe the event. DD's within-time fan-out guarantees this regardless of arrival order at operators. A rule that branches into a yield for E **at the same gen where E is emitted** sees E because both rows are at time T and the antijoin(pending, event) consolidates within T.

Event-emits-event chains: take **one gen per hop**. Termination trivial. Each gen runs once.

**Impl B — iterate scope per gen (escape hatch)**

```rust
scope.iterative::<u32, _, _>(|inner| {
    let event = Variable::new(inner, ...);
    let pending = Variable::new(inner, ...);
    let resolved = pending.antijoin(event);
    // ... fixed-point logic ...
});
```

Within one outer gen T, the inner iterates to fixpoint. Termination requires either a parse-time acyclicity check on the event subgraph, or a monotone counter on cyclic edges, or a bounded inner iteration.

Default to Impl A. Reserve B for explicit `cascade { ... }` blocks.

## Ordering within a gen

Solved by DD's within-time fan-out: every consumer sees every emit at time T regardless of arrival order. The runner's only contract is "all input rows for T must be inserted before advance_to(T+1)."

## Branching — `if` as op with Pipe values

```
if(${COND}, ${THEN_PIPE}, ${ELSE_PIPE})

lower:
    let then_op = row[THEN_PIPE].as_op_ref();
    let else_op = row[ELSE_PIPE].as_op_ref();
    let cond = c.filter(|r| r[COND].is_truthy()).pipe(then_op);
    let alt  = c.filter(|r| !r[COND].is_truthy()).pipe(else_op);
    cond.concat(&alt)

sugar:
    if(${SIZE > 80}) {
        publish_diag(${PATH}, "too long")
    } else {
        tag(:ok, ${PATH})
    }
```

Parser sees `{ ... }` blocks, lowers each block to a Pipe value captured under a synthetic name, passes them to `if` op. `if` lowers as above. No new control-flow concept.

## Aborting a gen on event timeout

```
T=10   user typed.  rule body emits pending_yield(key, deadline=15)
T=10   inspect_batch: SubjectRegistry.subscribe(key) parks future
...
T=15   runner sees deadline=15, t_now=15.  injects:
         timed_out(key) +1 at T=15
         timed_out(key) -1 at T=16   // scheduled retract
       antijoin(pending_yield, timed_out) fires:
         pending_yield(key) effective weight goes to 0
       inspect_batch on pending_yield fires -1:
         SubjectRegistry.unsubscribe(key)
       tokio future resolves Err(Unsubscribed)
```

The `timed_out` row is itself queryable. Other rules can react to timeouts the same way they react to any other event.

## When a keyword would become warranted

Only if you ever wanted continuation-after-yield to have nonlinear control flow (try/catch, loop, race, multiple awaits in arbitrary positions with different scopes). Then a keyword starts paying for itself because the parser has to track scopes.

Sprefa rule bodies are linear pipes of ops. Every op consumes from a Collection and produces to a Collection. The linear shape gives all the suspension semantics needed without grammar machinery.

## One-line summary

`next` and `await` are ops. No keyword. Every "control flow" you want falls out of join/antijoin/inspect_batch on a gen-only bag.

## Sources

- chat_log/20260501.1.dd-effects-control-flow-types.md (events gen-only + keywords case)
- v3/crates/effect_runtime/src/subjects.rs (SubjectRegistry primitive)
