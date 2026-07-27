# Stream modes: (cardinality, lifetime) with dominance

Source: 2026-07-27 session (v6 prolog compiler tier). Status: design locked,
lab unbuilt (`mode_lab` in ARCH.pl). The compile-time analysis that answers
"does asking terminate" and "is the result 1 value or many" for every ask.

## The mode type

Every stream/ask gets a two-column mode:

```
mode(Ask, card(det | semidet | multi), lifetime(finite | until(S) | never))
```

Cardinality is prolog/Mercury's determinism system, reused verbatim:

| prolog/mercury | rx name | our case |
|---|---|---|
| det (exactly 1) | Single | one `fetch` call: exactly one envelope. The `Error` arm makes it det rather than "semidet + can throw": failure is a value, the call cannot NOT produce a row |
| semidet (0 or 1) | Maybe | keyed read `cache(endpoint, S)` with `endpoint` bound: register key is a primary key, so 0-or-1. Functional-dependency mode analysis over rules |
| multi / nondet | Observable | unkeyed subscribe, `change_log` tail |

Lifetime is a 3-point order by "guaranteed to end":

```
finite  <  until(Signal)  <  never
```

| upstream | lifetime |
|---|---|
| `fact` rows / snapshot ask | finite |
| `= sse(route)` connection rows | until(disconnect) |
| `= every(300s)` timer | never (ON ITS OWN — see dominance) |
| `external = shell {...}` | finite per request (det: 1 next + complete) |
| derived rule | join of body inputs |
| register | lifetime of its `over` stream |

## The dominance rule (the switchMap math)

```
lifetime(inner stream) = min( lifetime(own binding), lifetime(enclosing scope) )
```

`switch_map` is a SCOPE CONSTRUCTOR: the inner subscription's scope is "until
the outer emits again or completes". So:

```
every(300s) alone:                    never
every(300s) inside switch_map(outer): min(never, until(outer_next)) = until(outer_next)
```

A timer that can never complete becomes conditionally-terminating the moment a
switch_map dominates it, and the condition is NAMED (the outer stream). The
analysis therefore runs over the static_subs graph (scope edges from
switch_map sugar), not the bare rule graph. Propagation stays a fold.

Runtime identity: dominance is not an rx callback. The inner stream's demand
rows live under a subscription path (runtime_subs forest); the outer's next
value range-DELETEs the path prefix; the timer's rows stop being demanded.
Unsubscribe, laziness, and dominance are all the same mechanism: demand rows.

## Ask modes at the CLI

| ask | mode | terminates |
|---|---|---|
| `? cache(endpoint)` bound key | (semidet, finite) | yes: one row or none |
| `? change_log(...)` snapshot | (multi, finite) | yes: current rowset, then complete |
| subscribe `change_log` | (multi, never) | provably no (chain bottoms at `every`); CLI can warn before blocking |
| subscribe inside a scope | (multi, until(outer)) | conditional, condition named |

Snapshot asks need no analysis (a SELECT is finite). Only tail asks consult
the mode table. Completion analysis is NOT a separate lab: lifetime subsumes
it (an earlier draft had completion.pl; folded in here).

## mode_lab grading cases (the contract for the lab)

`books/v6/algos/determinism.pl`, fold species, initialization.pl style, graded
on the ghcacher program:

1. `fetch` request: (det, finite) — envelope makes it det.
2. `cache(endpoint, S)`, endpoint bound: (semidet, finite).
3. `change_log` tail: (multi, never) — chain: change_log <- stars <-
   cache_body <- cache <- fetch <- poll <- every_300, bottoms at the timer.
4. A switch_map example: same timer flips never -> until(outer_next).
5. Register never completes unless its `over` completes (cache over fetch:
   fetch per-request finite, but requests recur on the poll clock -> never;
   dominated poll clock -> until()).

## Cross-references

- ARCH.pl callouts: LIFETIME IS DOMINATED, THE REGISTER ROW IS pre.
- ARCH.pl rows: algorithm(mode_analysis, static_subs, fold, unbuilt);
  task(mode_lab); technique(ask_modes, snapshot_vs_subscribe, ...).
- Prior art: Mercury determinism categories; RxJava Single/Maybe/Completable
  (cardinality-in-types; rxjs itself does not type this).
- retention.pl / initialization.pl: the sibling fold analyses.
