# v6 reactive datalog ↔ rx isomorphism (theory draft, NOT locked)

> **SUPERSEDED 2026-07-23 — see `chat_log/20260723.2.*.md`.** This is THEORY-
> CRAFTING, not a locked spec. Three claims below are retracted: (1) NOT locked.
> (2) the "3 orthogonal primitives" claim was disproven by lowering to JS/RxJS/Go
> (state is data, not a primitive; BehaviorSubject extends Subject; Go =
> goroutine + chan + data). Corrected primitives = the RxJS trinity
> Observable / Subject / BehaviorSubject + a BufferPolicy knob. (3) json-rx is
> now EXTRACTED from an actual-rxjs graph (round-trip proof), not a lowering
> target. The rel=delta-stream / mixing=merge / @next=scan / yield=Put-Take-Call
> ideas below still hold. Additionally retired 2026-07-23 (owner): the
> Put/Take/Call coroutine + durable-saga model. FRP unidirectional only;
> Subject/BehaviorSubject are the imperative (CSP-style) hatches.

Status: **THEORY DRAFT, NOT locked.** Runtime is now chosen (TS on actual rxjs);
this doc is the theory, not the spec. v5 is a lab and is
not viable long-term; nothing here inherits its grammar.

## Thesis

Every relation is a named delta-stream over a Z-set. Datalog IS rx over
incrementally-maintained collections. The materialized table is the shared
replay; laziness is the opt-in. v5's `@next` / `@async` / `@in` / `@out` and
`clock` / `http` / `lsp` / `mcp` are not primitives; they desugar to the small
algebra below.

## The one foundation: resumable coroutine + `yield`

A body with `yield` pause points whose locals are spilled to a struct between
resumes (that struct IS the durable state). `async`/`await` = generator + `yield`
+ a driver that resumes on completion; the Babel/regenerator switch-reducer
transform proves it is one mechanism, not two. So `@async` is sugar, not a peer
primitive.

`yield` exchanges descriptors with the runtime:

- `Put(ch, value)` — send to a channel/rel  (the `@next` carry write; `@out`)
- `Take(ch)` — receive from a channel/rel   (`@in`; the tick-pull of `@next`)
- `Call(kind, args)` — invoke a host effect, resume with result  (`@async`,
  `http`, `lsp`, `mcp`, `clock`)

## Rel kinds (the type system encodes the rx lifecycle)

| kind | rx twin | storage | seed? | shared? |
|---|---|---|---|---|
| rel (default) | materialized hot Subject, `shareReplay(all)` | a table | yes (INSERT) | yes, all readers share the table |
| lazy rel (opt-in) | cold Observable, re-derived per demand | none (computed) | no | no, the exception |
| port `in` | host-fed Subject | a table | yes (host INSERT) | yes |
| port `out` | host-tapped Subject | a table | derived | yes (host SELECT / watch) |

`share` is the DEFAULT for a materialized rel: the table is the replay buffer.
The cold/lazy rel is the EXCEPTION. This inverts rx and removes the "share is
hard to sell" friction: rels ARE the global hot singletons; you opt INTO
laziness, not into sharing.

## Constructs

| datalog | rx | storage |
|---|---|---|
| `rel a(b,c).` declaration | a named delta-stream | a table |
| `a(1,2).` fact / arbitrary insert | `a$.next(+a(1,2))` emission | INSERT +weight |
| `a(x,y) <- b(x),c(y).` rule | `combineLatest(b$,c$).pipe(map(join))` | derived delta stream |
| many rules → one `a` | `merge(rule1$, rule2$, …)` | UNION, weights add |
| `@next` / carry | `scan(reducer, seed)` / BehaviorSubject | argmax-by-gen row |
| read `a` in a body | subscribe to shared `a$` | SELECT |
| `? a(...)` query | `a$.pipe(take(1))` | one-shot SELECT |
| `@in` / `@out` | host-fed / host-tapped Subject | external INSERT / watch |
| `@async`, http, lsp, mcp | `concatMap(args -> Call(args))` | a durable-effect row |

## Rule model (mixing is allowed)

A rel's value = `merge(seedFeed, rule1Feed, rule2Feed, …)`. Each feed is a delta
stream; weights add in the Z-set. Multiple rules heading one rel = merge/union,
column-type-aligned (SQL UNION). v5's "one rel = one rule kind" ban was a
full-rebuild artifact and is gone under incremental deltas. Facts are emissions
(+weight); arbitrary inserts are allowed, like SQL.

## Time / carry

`@next` = `scan(reducer, seed)` = a 1-slot, tick-clocked channel read by
`argmax(gen)`. Latest-wins is that channel's overwrite policy. The policy is a
per-rel knob: `latest` / `all` / `next-1`. The "I rely on the next limit-1
match" join is a `Take(ch, limit=1)` over time: mapping over values as they
arrive, not over the current set.

## Effects

`@async` / `http` / `lsp` / `mcp` = a coroutine that `yield`s `Call(kind, args)`;
the interpreter runs it and resumes. `await` = `yield Call`. Durable: the
spilled-locals struct is a row; a crash loses nothing; the switch-reducer step
resumes from exactly those bytes. `kind` lives in the interpreter's host hole
(http/lsp/mcp/timer), NOT in the language grammar. Adding IO = teach the
interpreter a new `Call` kind.

## Queries / sinks

`? a(...)` = `take(1)` cold read. `@out` / a change feed = a tap the host drains
(or a `port out`).

## The RAM discipline (holds on EITHER runtime)

Data lives in SQLite (the materialized tables). Streams/observables carry DELTAS
and KEYS, never rows. RSS = SQLite page cache (a knob). This is the property
that kills v5's 36GB swap, and it is a discipline, not a language property. Rust
enforces it via ownership; TS requires it by hand. Either pays it.

## Lowering target (runtime-independent)

```
datalog source
  -> rx graph (json-rx shape: sources / flows / outputs)
  -> Put / Take / Call descriptors
  -> interpreter (runs descriptors, drives the cascade, persists state)
```

`json-rx`'s compiler already does declarative-doc → rx-graph. The datalog
lowering reuses that shape with rules as flows.

## DECIDED (do not re-derive)

- rel = named delta-stream Z-set; materialized = shared replay (default); lazy =
  opt-in. `share` is free.
- mixing rules on one rel = merge/union; facts = inserts; both allowed.
- `@next` = scan / argmax-by-gen channel. `@async` = `yield Call` (sugar).
  ports = host Subjects.
- `yield` exchanges `Put` / `Take` / `Call`; the interpreter is the host hole.
- RAM bound = SQLite page-cache discipline, not a language property.

## OPEN (resolve before or during the port)

1. Channel buffer policy as a first-class knob (`latest` / `all` / `next-1`):
   syntax and default.
2. Lazy rel: grammar (`rel lazy` / `@lazy`?) and demand-materialize semantics.
3. Share across scripts/namespaces: is a rel global to one program, or shared
   across the daemon? (the singleton-scope question)
4. The exact datalog grammar for ports and effects (declarative forms).
5. Subscription lifecycle in types: can the rel-kind type encode
   subscribe-start → next → subscribe-end (rx's biggest problem)? Candidate: the
   kind (materialized / lazy / port) IS that encoding.
