# Reactive / effectful datalog — what it takes to usurp ghcacher

Question: ghcacher (`~/projects/ghcacher`) is a polling cache. It shells `gh api` with
etag/304 conditional requests, normalizes JSON into a SQLite schema, and emits an
append-only `change_log` that consumers tail. It is effectful, stateful, time-driven,
and talks to an external service. v5 is a declarative, file-reactive, SQL-fixpoint
datalog over sqlite. What must the language gain to subsume ghcacher, what did v4 already
try, how do other systems/datalogs solve the same tension, and what does each path commit
us to down the road.

Cross-refs: [ext-dbsp-incremental.md](ext-dbsp-incremental.md),
[research-portable-reactive-core.md](research-portable-reactive-core.md),
[data-model.md](data-model.md), v4 `docs/v4-ghcache-integration.md` +
`docs/v4-runtime-batching.md`.

---

## 1. ghcacher decomposed into primitives

From `~/projects/ghcacher/src/schema.sql` + `src/gh.rs` + `src/sync/`:

| ghcacher mechanism | source | language primitive it needs |
|---|---|---|
| `gh api <ep> -H If-None-Match:<etag>` → status+headers+body | `gh.rs` GhRequest/GhResponse | shell call **fanned over relation rows**, capturing structured (status, header map, body), not stdout lines |
| `poll_state(endpoint, etag, last_modified, poll_interval, ...)` | schema.sql | a relation the program **reads from last tick and writes back this tick** (carry state) |
| `call_log(... rate_remaining, rate_reset)` + `throttle_if_needed` | gh.rs | a **guard** that suppresses the call when a predicate over prior state holds |
| 304-vs-200 branch → upsert normalized tables | sync/*.rs | filter on a captured column + **upsert into a stable named output table** |
| `change_log(entity_type, entity_id, event∈ins\|upd, ...)` tailed by `id > :last_seen` | schema.sql | the **delta stream surfaced as a relation/sink** |
| `ghcache watch` interval loop, per-endpoint backoff | sync/mod.rs | a **clock/interval source** (reactivity on a timer, not just FS events) |
| `--paginate`, graphql cost | gh.rs | free — `cmd` flags |
| checkout `git fetch`/`pull --ff-only` per repo | checkout.rs | row-driven `cmd` again + the existing spine |

Seven adds, sequenced: row-driven `cmd` → structured capture → carry relation → guard
give a one-shot `sync`; clock → delta-as-relation → named output tables give `watch` +
the consumer contract. Only the **carry relation** changes evaluation semantics; the rest
are op/surface additions over machinery v5 already has (cmd, json, sqlite, incremental
tick, the `rev` spine).

---

## 2. v4 already drove at this (the rxjs arc)

v4 built an rxjs-shaped consumer of ghcacher and then v5 dropped the runtime that made it
work. From `docs/v4-language-vector.md` + `v4-runtime-batching.md`:

| v4 concept | rxjs analog |
|---|---|
| **cursor** (value + term bag + source ref) | the emission / `next(x)` |
| **`>`** pipe operator (`gh.prs(...) > open_prs(...)`) | `.pipe()` |
| generation / tick | scheduler frame; commit = flush |
| **streaming op vs barrier op** | `map`/`filter` vs `buffer`/`scan`/`toArray` |
| dirty-bus subscription by key-space | keyed `Subject` multicast |
| **support-count retraction** (mounted_query_support) | `refCount` / unsubscribe |
| `gh.prs(...)` reading ghcacher sqlite, parked on `git/pr`, woken by `change_log` | hot Observable over an external source |
| `effect_runtime`: queue, component, pipe, node, generation, bus, **timers**, next/wake | Scheduler + Subject layer |

v4's `gh.*` + mounted-query + `change_log` wake (`v4-ghcache-integration.md`) was a working
slice: it read PR rows, parked a query on a dirty domain, diffed on wake, emitted only new
cursors, retracted by support count. v5 deliberately removed that DD/runtime_graph/support
machinery for the SQL-fixpoint engine. The pipe/cursor reactivity is exactly what v5 traded
away and now must re-acquire (differently) to eat ghcacher.

---

## 3. How other systems solve the tension

The tension is constant: a timeless declarative engine meets stateful, time-driven,
external, effectful IO. Every family reifies the impure part as data and threads or
sequences it explicitly.

### 3a. Time-as-attribute datalogs (elegant, cheap)

| System | Mechanism | Solves ghcacher's... | Cost |
|---|---|---|---|
| **Dedalus / Bloom** (Hellerstein, Berkeley) | every fact carries a logical timestamp; three rule kinds: **deductive** (same tick), **inductive `@next`** (carry a fact to next tick = state), **async `@async`** (fact lands at some later nondeterministic tick = network/IO) | poll_state forward = `@next`; `gh:get` poll = `@async`; change feed = deduction across consecutive ticks | you own CALM/confluence semantics for async nondeterminism. Pure *model*, not a fast engine |
| **Datomic / Datascript** | time is a `tx` column; DB is an immutable value (`as-of`/`since`/`history`); built-in `tx-report-queue` change feed | exactly Path B below | "incremental" = re-query a newer value; Datascript `listen!` only diffs. Append-only growth. Strong time model, no view maintenance |

Same idea (time is data) at two altitudes: Bloom adds the rule *modifiers* that name
state-vs-IO; Datomic ships the storage + change feed. ghcacher's `change_log`+`poll_state`
is a hand-rolled tx-report-queue + as-of index.

### 3b. Incremental-maintenance engines (heavy, principled)

| System | Mechanism | Cost / commitment |
|---|---|---|
| **Differential Dataflow / Timely → Materialize** | tuples are `(data, time, diff)`; multidimensional timestamps; recursion incrementalized; views maintained under arbitrary deltas | the exact DD/runtime_graph/support machinery v5 **exorcised**. Heavy runtime, arrangement memory |
| **DDlog** (VMware, archived) | *compiled* incremental datalog: input rels take insert/delete deltas, output rels emit deltas; the whole program is a delta transformer | relations static at compile time (no dynamic assert), unmaintained, Rust-codegen build step |
| **DBSP / Feldera** (newest, cleanest) | incremental computation as a **circuit** over z-sets with `integrate`/`differentiate`; strictly simpler than Timely's partial orders | still a delta-circuit runtime under everything. If we ever want real IVM, this is the minimal formalism to copy, not Timely. See [ext-dbsp-incremental.md](ext-dbsp-incremental.md) |
| **LogicBlox / LogiQL** (defunct) | datalog as a transactional DB: ACID txns, delta rules, maintained views, aggregation/lattices | commercial, gone; design lesson only ("the program is a DB with maintained views") |
| **Noria / ReadySet** | partial-state materialized views over SQL with **eviction** | partial materialization + eviction; relevant if we can't fully hold state (133MB kernel). SQL not datalog |

Any of these makes the change feed and reactivity correct by construction, but re-commits
us to a delta-propagation runtime — the thing the v5 rewrite walked away from. DBSP is the
one worth re-reading if that call is ever reversed.

### 3c. Demand / tabling (what v5 already half-is)

| System | Mechanism | Tradeoff |
|---|---|---|
| **XSB / SWI incremental tabling** | SLG-resolved memo tables auto-invalidated when dynamic facts change; **monotonic tabling** streams additions | mature, in-engine, but Prolog-shaped (term unification), not set/SQL-shaped |
| **Salsa / Adapton / self-adjusting computation** | demand-driven: a query pulls, deps tracked, re-fire only the dirtied path (rust-analyzer's model) | great on-demand (v5's LSP path already works this way); no temporal/streaming model, no change feed |

v5 as built is the realist continuation here: `--changed` incremental tick + LSP-on-demand
is a hand-rolled Salsa-over-SQL. Extending it to wake on a clock and diff the touched set
yields a change feed with no delta algebra. Lowest risk, least elegant theory.

### 3d. The why-it's-safe theory layer

| System | Idea | Relevance |
|---|---|---|
| **Flix** | Datalog constraints are first-class values in an ML; **lattice** semantics | "program/effect as a value you compose then solve" — Path A's reification without Mercury's token |
| **Datafun** | datalog as a functional language with **monotonicity types** | the theory under DD/Bloom CALM: which rules are safely incrementalizable. Read before deciding what `delta` may range over |

### 3e. Logic/functional escape hatches (for completeness)

| Family | State / effect mechanism | Note |
|---|---|---|
| **Prolog** | `assert/retract` (mutable dynamic db), DCG difference lists (thread state as arg-pair), engines (pull cursors), `freeze/when` (suspend until bound), `library(http)`/pengines | assert/retract is the raw carry-relation; non-logical, order-dependent, the escape hatch everyone warns about |
| **Mercury** | `io.state` **unique world token** threaded through every IO pred (`main(!IO)`), enforced single-threaded by `di`/`uo` modes; determinism categories type the solution count | principled Path A; compiler forces effect ordering |
| **Haskell** | `IO` (hidden `RealWorld` token), `STM`/`TVar` (transactional carry + guard), **conduit/pipes** (pull streams w/ backpressure = the `>` pipe), **FRP** (`Behavior`=poll sample, `Event`=change_log), **Haxl** (batched/cached/deduped remote fetch = the gh-api fan), **Shake** (`need` + dirty) | Haxl is already ghcacher's "fan a call over rows, batch, cache-by-etag" solved. See skill `sagas:applicative-batching` |
| **Clojure** | core.async (CSP channels = the dam), missionary (functional dataflow/FRP), **Datomic** (3a) | Datomic is the single closest whole-system prior art |

---

## 4. The two non-DD paths, at zoom-1

Both avoid reintroducing the DD machinery v5 dropped. Same job: poll open PRs for a repo
set, conditional on etag (skip 304), normalize into a queryable table, expose a change feed.

### Path A — threaded world token (Mercury `io.state` / IO monad)

State and effect order carried by a linear `W` token threaded through every effectful call.
Same `W` in, same rows out: replayable, statically ordered.

```
poll_target("repos/me/app/pulls").
poll_target("repos/me/lib/pulls").

# gh:get is a function of (world, request) -> (response, world').
# Threading W0 -> W1 -> W2 forces a sequence over an otherwise unordered set.
step(EP, STATUS, BODY, ETAG1, W1) <-
    poll_target(EP),
    etag_in(EP, ETAG0),
    gh:get(W0, EP, ETAG0) -> (STATUS, BODY, ETAG1, W1).

pr(REPO, NUM, TITLE, STATE) <- step(EP, 200, BODY, _, _), json(BODY, q:{ ... }).
etag_out(EP, ETAG1)         <- step(EP, _, _, ETAG1, _).
```

| Adds to engine | Why |
|---|---|
| a linear `World` value + consume-once (`di`/`uo`) checking | order + single-thread guaranteed by the type, not luck |
| effect ops typed `(W, req) -> (resp, W')` | every gh/git/write call takes and returns the token |
| a sequenced "main" spine beside the relational store | **the leak**: a token is single-threaded, a relation is a set. One `W` through N repos serializes them; N independent worlds lose global order. Mercury has a functional thread-of-control; datalog has none, so you bolt an imperative driver onto the side of the set engine |

Buys: deterministic replay, no mutable cell, effects can't reorder. Costs: reintroduces
imperative sequencing into a set language. This is the v4 cursor/pipe direction in a purity
jacket.

### Path B — tx-report / immutable value (Datomic)

Time is a column, not a token. A clock advances `tx`; facts are stamped with the writing
`tx`; "current" is as-of `max tx`; the change feed is a **derived relation** (diff `tx`
against `tx-1`), not a maintained side-effect log.

```
clock(TX) <- every(30s).                       # interval source; each fire bumps TX

poll_target("repos/me/app/pulls").

resp(EP, STATUS, BODY, ETAG, TX) <-
    clock(TX), poll_target(EP),
    etag_asof(EP, ETAG0, TX - 1),
    gh:get(EP, ETAG0) -> (STATUS, BODY, ETAG).

pr(REPO, NUM, TITLE, STATE, TX) <- resp(_, 200, BODY, _, TX), json(BODY, q:{ ... }).
etag(EP, ETAG, TX)              <- resp(EP, _, _, ETAG, TX).

# as-of view: latest row per key — the public queryable table.
pr_now(REPO, NUM, TITLE, STATE) <- pr(REPO, NUM, TITLE, STATE, TX), TX = max_tx(REPO, NUM).

# change_log, DERIVED: appeared this tx, absent last tx.
delta(:pr, REPO, NUM, :ins, TX) <- pr(REPO, NUM, _, _, TX), not pr(REPO, NUM, _, _, TX - 1).
delta(:pr, REPO, NUM, :upd, TX) <-
    pr(REPO, NUM, T, S, TX), pr(REPO, NUM, T0, S0, TX - 1), (T, S) != (T0, S0).
```

| Adds to engine | Why |
|---|---|
| `every(secs)` interval source (the clock) | reactivity on a timer, not just FS events; v4's `effect_runtime` already listed timers |
| a `tx` poll-coordinate shaped like the existing `rev` | rides `type_edge_rev`/`module_edge_rev`/content-addressed `_files`; we already dedupe-across-rev and keep history |
| `as-of` / `max tx` selection sugar | "current" is a query; history is free |
| `delta` as a derived rel + a tail cursor | the change feed is the incremental diff we already compute, surfaced. ghcacher's `change_log` becomes a *rule*, not a maintained table |

Buys: queries stay timeless (pure function of `tx`), `change_log` is the diff we already
compute, fits set semantics, no imperative spine. Costs: the effect (`gh:get`) still needs
a side-effect seam, and the tx-ordering of effectful rules must be defined (what fires
before the clock advances).

### Contrast

| | Path A (token) | Path B (tx-report) |
|---|---|---|
| state = | a value threaded through rules | a column on facts |
| time = | implicit in token order | explicit `tx` coordinate |
| change feed = | logged as an effect | derived by diffing `tx` vs `tx-1` |
| poll loop = | a sequenced main | a clock source row |
| fights datalog set semantics? | yes | no |
| reuses v5 today? | little | most |
| prior art | Mercury, Haskell IO/conduit | Datomic, SWI incremental tabling |
| v4 lineage | the cursor/`>` pipe | the `change_log` wake + mounted query |

---

## 5. The cheap elegant upgrade: Bloom `@next` / `@async` on the v5 grammar

v5 today is Path B executed by hand (SQL fixpoint + `rev` + `--changed` + on-demand LSP =
Salsa-over-SQL with a Datomic-ish rev column). The smallest *named, declarative* step is to
steal exactly two rule modifiers from Bloom and spell them against the `<-` grammar:

- **`<-@next`** — the head fact is asserted at the *next* tick, not this one. This is the
  carry relation (poll_state/etag survives a tick) without a mutable cell and without a
  threaded token. State = a fact that re-derives itself forward until something stops it.
- **`<-@async`** — the body fires an effect (`gh:get`, `git fetch`, `write`) whose result
  lands as a fact at some later tick. This is the poll itself: the rule does not block; the
  response arrives as a future fact keyed by the request.

Sketch against the real grammar (`<-`, `rel`, facts end `.`):

```
rel poll_target(ep: text).
poll_target("repos/me/app/pulls").

rel etag(ep: text, val: text).

# CARRY: etag survives to the next tick unless a 200 overwrites it. (@next)
etag(EP, V) <-@next etag(EP, V), not got_200(EP).

# POLL: an async effect; its response lands as resp(...) at a later tick. (@async)
rel resp(ep: text, status: int, body: text, etag: text).
resp(EP, S, B, E) <-@async clock(_), poll_target(EP), etag(EP, OLD), gh:get(EP, OLD) -> (S, B, E).

got_200(EP)   <- resp(EP, 200, _, _).
etag(EP, E)   <- resp(EP, _, _, E).                  # this tick overwrites the carry
pr(REPO, N, T, ST) <- resp(_, 200, B, _), json(B, q:{ ... }).
```

This gives ghcacher's poll loop and carry-state a declarative form with no DD machinery,
and it composes with the `tx`/`rev` time column already maintained (Path B). The cost is the
semantics homework: `@next` needs a stratification rule (a fact and its `@next` negation
must not form a temporal paradox — Dedalus' answer is that `@next`/`@async` live in a later
stratum by construction), and `@async` introduces nondeterministic arrival order (CALM:
confluent only if downstream is monotone in the response). Datafun's monotonicity typing is
the tool for deciding which `delta`/aggregate rules stay safe.

Lex/grammar collision check — DONE (`src/lex.rs`, `src/parse.rs`):

- `@` (0x40) is handled nowhere; the lexer's default arm is `_ => bail!("unexpected
  char")` (lex.rs:223), so `@` is free and currently errors loudly. No silent clash.
- `<-` lexes as `Tok::Arrow` via the `b'<'` match (lex.rs:108-115) regardless of what
  follows; `<-@next` lexes the same with or without a space. The neck handling does not
  change.
- No collision with the colon-literals: `, :rust` is `Colon`+`Ident` (space before colon,
  lex.rs:210), and `scheme:body` / `q:{}` are the scheme-adjacency arm (lex.rs:210-218).
  `@` is orthogonal. Inside `"..."` / `/.../` / scheme bodies, `@` is a literal byte
  consumed by those arms and never reaches punctuation dispatch.

Resolution: do **not** fuse with `<-`. Add a standalone modifier token after `Arrow`:

```
b'@' => {
    i += 1;
    let start = i;
    while i < b.len() && (b[i] == b'_' || b[i].is_ascii_alphanumeric()) { i += 1; }
    if i == start { bail!("lone '@' (expected @next/@async after a rule neck)"); }
    out.push(Tok::At(src[start..i].to_string()));
}
```

Then `parse.rs::rule()` peeks for an optional `Tok::At(_)` right after it consumes `Arrow`
(parse.rs:~194) and stamps `Rule { temporal: Option<Temporal> }` (`Next | Async`). An
unknown modifier name is a parse error there, not a lexer concern.

---

## 6. The fork, for down the road

| Bet | Adopt | Pays off if | Costs |
|---|---|---|---|
| **Time-as-data** | Bloom `@next`/`@async` + Datomic `tx` on the `rev` spine | want state + polling + change feed *soon*, on sqlite, no engine rewrite | recompute stays "re-tick touched"; you own async confluence |
| **Real IVM** | DBSP z-sets (not Timely) | reactivity/consistency at scale becomes the product, not a feature | reintroduces a delta runtime v5 deleted; months not days |
| **Demand-tabling** | extend `--changed` + LSP-on-demand with a clock + touched-set diff | want the smallest step from today, never need cross-the-board view maintenance | no principled delta algebra; change feed is "diff what I happened to recompute" |

Recommendation (when asked): the Time-as-data bet is the cheapest that is still principled.
It is Path B plus two Bloom modifiers, both additive over the `rev` spine and the
incremental tick already shipped. It does not resurrect the support/runtime_graph machinery
v5 exorcised, and it leaves the Real-IVM (DBSP) door open as a later swap of the evaluation
core, not a rewrite of the surface language.

---

## 7. Next concrete step

Lex/grammar check: DONE (§5). Zoom-2 spec of the carry relation + effect seam: §8 below.
After that, the first buildable slice: the `@next` double-buffer + the stratifier change
(treat carry reads as EDB), with `gh:get` stubbed as a synchronous source op, before the
async executor exists.

---

## 8. Zoom-2 spec — `@next` carry + `@async` effect seam

Grounded in `src/engine.rs` `tick()` (1916), `src/ast.rs` `Rule` (203). Follows the
four-layer protocol: signatures, pseudo-body, instance lifetimes, then storage / read-write
order / uniqueness.

**Status: built.** `@next` carry (commit bd7eb6e) and `@async` effect seam (89fc8d7) land the
`pending_effect` queue, `EffectExec` trait, and off-tick `drain_effects`. The daemon clock +
real-IO executor follow: `ShellEffectExec` (kind -> `sh -c` template, `{var}` filled from the
request args, stdout split into the output slots) and a `DL_POLL_SECS=N` poll loop that
advances the tick and drains effects every N seconds. The command map is declarative: the
daemon reads `effect_cmd(kind, template)` rows from the program each poll. So the ghcacher
poll loop is: `effect_cmd` facts name the shell call, an `@async` rule fans it over the
targets, and the clock drives the drain. Tests: `tests/it/temporal_carry.rs`,
`tests/it/temporal_async.rs` (incl. a real subprocess). Still open: a DSL `every(secs)` clock
source (lets the program own its cadence vs the env knob), the `delta` change-feed, and the
etag/304 conditional-request guard.

**One simplification vs the §8.2 sketch below.** The sketch wrote the effect as a body op
`gh:get(EP, OLD) -> (S, B, E)`. The built form has no effect op: an `@async` rule body is an
ordinary conjunctive query that binds the *request args* (every positive-atom variable), and
the head's columns are the response. A head term the body binds echoes from the request; a
head term the body does NOT bind is an *output slot* the executor fills. The effect `kind` is
the head rel name; the daemon's `EffectExec` maps kind -> the actual shell call. So
`resp(key, status, body) <- @async want(key, url).` queues one request per `want` row with
args `{key, url}`, and the executor returns `[status, body]` (the two unbound head slots).
This keeps the grammar at zero new body ops and reuses the source-op "unbound out var"
mental model.

### 8.1 Type signatures

```rust
// ast.rs — Rule gains one optional field; absent = today's behavior unchanged.
pub enum Temporal { Next, Async }
pub struct Rule { /* head, body, aggs, origin, */ pub temporal: Option<Temporal> }

// lex.rs — one token (the @-modifier after the Arrow neck).
Tok::At(String)

// engine.rs — tick partitions gain two buckets parallel to source/derived.
fn next_rules<'a>(rules: &[&'a Rule])  -> Vec<&'a Rule>;   // temporal == Some(Next)
fn async_rules<'a>(rules: &[&'a Rule]) -> Vec<&'a Rule>;   // temporal == Some(Async)

// the monotonic poll/tick coordinate; one row in a meta table.
fn current_tx(&self) -> i64;          // read _carry_meta.tx
fn advance_tx(&mut self) -> i64;      // tx += 1, returns new

// effect executor (lives in the daemon, NOT in tick()).
fn drain_effects(&mut self, exec: &dyn EffectExec) -> Result<usize>;
trait EffectExec { fn run(&self, kind: &str, args: &[Value]) -> Result<Vec<Value>>; }
```

### 8.2 Pseudo-bodies

```rust
// @next: evaluate the body over THIS tick's converged state, stage rows for tx+1.
// carry_<rel>(cols.., tx) is read as EDB at tick start (WHERE tx == current_tx),
// written at tick end (rows stamped tx == current_tx + 1). Double-buffer by tx, not
// two tables — sidesteps the source/derived collision bail (engine.rs:1972) because the
// HEAD writes carry_<rel> while same-tick rules read it as a plain source rel.
fn rebuild_next(&mut self, next_rules) {
    let tx = self.current_tx();
    for r in next_rules {                       // body lowers to a SELECT like a derived rule
        let rows = self.eval_body_as_select(r); // over the converged tick-T relations
        self.db.insert_rows(&format!("carry_{}", r.head.rel), cols, rows.stamp(tx + 1));
    }
}

// @async: the body is NOT executed for effect inside the tick. It emits a request row.
// The clock-driven daemon runs drain_effects BETWEEN ticks; the response lands as a
// normal source fact before a later tick reads it.
fn rebuild_async(&mut self, async_rules) {
    let tx = self.current_tx();
    for r in async_rules {
        let reqs = self.eval_body_as_select(r);            // bindings minus the effect call
        for req in reqs {
            let id = effect_id(r.head.rel, &req);          // stable: rel + arg digest
            self.db.upsert("pending_effect",
                (id, r.effect_kind(), req.args(), tx, /*done=*/0));   // idempotent on id
        }
    }
}

fn drain_effects(&mut self, exec) {                        // daemon loop, off-tick
    for (id, kind, args) in self.db.select("pending_effect WHERE done = 0") {
        let out = exec.run(kind, args)?;                   // the actual `gh api` shell
        self.db.insert_rows(resp_rel_for(kind), out_cols, [out.with_key(id)]);
        self.db.execute("UPDATE pending_effect SET done = 1 WHERE id = ?", id);
    }
}
```

### 8.3 Instance lifetimes

| State | Holder | Lifetime | Durable? |
|---|---|---|---|
| `carry_<rel>` (cols, tx) | `Db` (sqlite) | across ticks AND process restarts | yes — state survives a daemon bounce |
| `_carry_meta.tx` (the clock coordinate) | `Db` | monotonic for the db's life | yes |
| `pending_effect` (id, kind, args, tx, done) | `Db` | row lives until executed + GC'd | yes — an interrupted poll resumes |
| `EffectExec` (the gh/git/http runner) | daemon component | the daemon process | no — pure executor, no state |
| converged relations (`resp`, `pr`, ...) | `Db`, rebuilt each tick | one tick (derived) or persisted (source) | per existing rules |

The carry/effect tables are the *only* new persistent state. Everything else is the
existing tick. The daemon (`src/daemon.rs`) already owns a warm `Engine`; the clock and
`drain_effects` slot into its loop.

### 8.4 Storage layout, read/write order, uniqueness

**Layout** (one table per carry rel + two shared meta tables):

```
carry_<rel>(<rel cols...>, tx INTEGER)          -- per @next-headed rel
_carry_meta(k TEXT PRIMARY KEY, tx INTEGER)     -- single row k='tx'
pending_effect(id TEXT PRIMARY KEY, kind TEXT, args_json TEXT, req_tx INTEGER, done INTEGER)
```

**Read/write sequence inside one `tick()` (insertions vs engine.rs:1916):**

```
1. tx := current_tx()
2. partition rules: + next_rules, + async_rules   (after the existing source/derived split)
3. stratify: carry_<rel> reads count as EDB for THIS tick  (see uniqueness below)
4. reconcile_sources()        — unchanged; also surfaces carry_<rel> WHERE tx==current as a source rel
5. rebuild_derived()          — unchanged
6. rebuild_next()             — stage carry rows at tx+1   (AFTER derived converges)
7. rebuild_async()            — emit pending_effect rows    (AFTER derived converges)
8. commit
   --- off-tick, daemon: drain_effects(); on response insert -> dirties -> schedules next tick ---
9. advance_tx()  (on the NEXT clock fire, not here)
```

**Uniqueness / correctness conditions:**

1. *No same-tick temporal paradox.* A `@next` head lands at `tx+1`; a same-tick rule that
   reads/negates `carry_<rel>` reads `tx`. Head-gen ≠ read-gen, so `p <-@next not p` is
   stratified by construction. Enforce by: the stratifier classifies `carry_<rel>` as EDB
   (a leaf), never as a derived dependency of the `@next` rule that writes it. This is what
   keeps it out of the engine.rs:1972 source/derived bail.
2. *Carry idempotence.* `carry_<rel>` rows for a given `tx` are written exactly once
   (step 6, after convergence). Re-entry (a re-tick at the same `tx`) must `DELETE FROM
   carry_<rel> WHERE tx = tx+1` before staging, so a re-run is not additive. Uniqueness key
   = (all rel cols, tx).
3. *Effect idempotence.* `pending_effect.id = digest(rel, args)` is the upsert key, so the
   same request emitted on two ticks before execution does not double-fire. `done` flips
   once; GC deletes `done=1` rows older than N tx.
4. *Response ordering (CALM).* `resp` arrives at an unspecified later tick. Downstream of
   `resp` must be monotone (a `pr` derived purely by selection/projection is; an aggregate
   like `max_tx` is monotone in tx). A non-monotone consumer of `resp` (e.g. a `not resp`)
   is the one unsafe shape — flag it at stratification time, the Datafun-monotonicity check.

**The one engine change with teeth:** step 3. Today `is_source()` is syntactic (the body has
a `scan`/`match`/...). `carry_<rel>` has no such body item, yet must read as a source for the
current tick. So `reconcile_sources` (or a sibling) must surface `carry_<rel> WHERE tx =
current_tx` as a readable relation before `rebuild_derived`, and the stratifier must treat
it as EDB. That is the whole semantic delta. Everything else (tokens, the effect queue, the
daemon loop) is additive plumbing over machinery that exists.

## 9. Zoom-2 spec — the effect runtime: shell-fn templates, a job table, the reconcile drain

§8 built the minimum: `pending_effect` as a flat queue, a per-row serial `sh -c` in
`drain_effects`, the command map as an `effect_cmd(kind, template)` relation, one global
`DL_POLL_SECS` clock. That ships. This section is the next layer, driven by five questions
the flat version does not answer well: per-stream cadence, native HTTP, streaming responses,
parallel/batched shell, and process control. Four of the five collapse into one structural
move (the drain becomes a reconcile loop over a job table); the fifth (shell as a first-class
language thing) is the surface that makes the whole effect layer readable.

### 9.1 The reframe: `@next`/`@async` are the lazy-state + unidirectional-stream core

`@next` = carry a value to the next tick = `BehaviorSubject` (lazy state that survives a
generation). `@async` = a body fires, the response lands later = the one-shot unidirectional
stream (request -> single response). Together they are the rxjs spine minus the operator
zoo: state that persists and effects that resolve out-of-band, both as data on the `tx`
spine. What they do NOT yet cover is the *generator* shape — one effect that yields MANY
rows over time (pagination, `tail -f`, SSE, a websocket). That is the coroutine/`yield`
direction (§9.5); it is a third rule kind, not a config of the two.

### 9.2 Shell as a language thing: the `sh` template declaration (not a relation)

Today the command lives in an `effect_cmd(kind, template)` *row*. The dream is to make a
templated CLI call a declared, named, typed *callable* — neither a relation nor a magic op,
a third top-level item next to `rel`/`rule`:

```
# A templated bash function. Params fill {holes}; the arrow names the outputs.
# Backtick body = the shell line; the engine never parses it, it runs it.
sh gh(repo, path) -> (status, body) {
    gh api "repos/{repo}/contents/{path}" -w "\n%{http_code}"
}

# Terser single-line form.
sh sha(file) -> (digest) = `git hash-object {file}`.
```

Used as the effect in an `@async` body (this is §8.2's `gh:get(...) -> (...)` sketch pulled
back, but *declared* instead of magic):

```
resp(repo, path, status, body) <- @async want(repo, path), gh(repo, path) -> (status, body).
```

The `sh` decl replaces the `kind`-is-the-head-rel-name convention: the effect kind is the
shell-fn name, args are its params (explicit), outputs are its return tuple (explicit). The
§8 built form (head-rel as kind, unbound-head-terms as outputs) stays valid as the
zero-ceremony path; the `sh` decl is the typed, reusable, named upgrade.

Signatures:

```rust
// ast.rs — a new Item variant.
pub struct ShellFn {
    pub name: String,            // call name = effect kind
    pub params: Vec<String>,     // {hole} names, the request args
    pub outs: Vec<(String, Type)>, // the response tuple, typed
    pub body: String,            // raw shell, {param} holes filled at run
    pub streaming: bool,         // §9.5: `sh*` yields many rows over time
}
pub enum Item { Rel(..), Rule(..), /* ... */ Shell(ShellFn) }
```

This is the piece that makes the effect layer *read* like the rest of the language: the
`@async` body names a function, not a string, and the function is checked (arity, output
columns, hole coverage) at parse time, not at drain time.

### 9.3 Per-stream cadence: `every(secs)` as a clock relation

Retire the global `DL_POLL_SECS` as the *only* knob. `every(N)` is a built-in source
relation, non-empty only on the wall-clock boundary where the daemon's base tick crosses a
multiple of N:

```
fast(k, s, b) <- @async want(k, u), gh(k, u) -> (s, b), every(30).
slow(...)      <- @async other(...), gh(...) -> (...),  every(900).
```

Cadence becomes data, per-rule, composable (`every(30), weekday()` is a throttle AND a
filter). The daemon poll loop runs at the base resolution; each `every(N)` gates its own
rule. `DL_POLL_SECS` degrades to "base scheduler tick." This honors the file's rule that
derivable things are facts that flow, not global statements.

### 9.4 The structural move: `pending_effect` becomes a job table, the drain a reconcile loop

The flat queue + per-row serial `sh -c` has two faults: it is a subprocess **N+1** (the
exact per-row-write anti-pattern the repo bans, in spawn space), and it has no handle on a
running child (no kill, no timeout, no liveness). Both fix with one change: make the table a
job table and the drain a reconciler.

```
pending_effect(
  id TEXT PRIMARY KEY,     -- = blake3(kind, args): the IDENTITY and the interrupt key
  kind TEXT, args_json TEXT, req_tx INTEGER,
  state TEXT,              -- queued | running | done | failed
  pid INTEGER,             -- the live child, for kill/reap/reattach
  started_at INTEGER
)
```

The drain is no longer "loop rows, run each." It is a reconcile of **desired** (the request
ids the `@async` rules currently derive this tick) against **running** (rows with a live
pid), the k8s-controller shape the daemon already uses for discovery mode:

| desired | running | action |
|---|---|---|
| yes | no | spawn (`.spawn()`, record pid + started_at, state=running) |
| no | yes | SIGTERM (input changed or retracted — kill by digest id) |
| yes | yes, over timeout | kill, state=failed |
| (exited) | — | reap, insert response into the head rel, state=done |

The content-hash interrupt the design wants is free: `id = blake3(kind, args)` already *is*
the hash. When an input row changes, the id changes, the old id is no longer in the desired
set, so the reconcile kills its child. No new key needed; the digest is the interrupt key.

`nohup`/detach matters only for jobs that must outlive a daemon bounce (the streaming case);
for a poll cache the daemon owns the children and a plain spawn + kill-on-undesired suffices.
Persisting `pid` is what lets a restarted daemon reattach or reap orphans.

### 9.5 Parallel and batch: the `xargs` combinator, the subprocess collect-then-flush

Two ways to stop the N+1, not exclusive:

- **(a) Engine-owned pool.** Reconcile spawns the desired-not-running set across a bounded
  rayon pool (the `cmd` op already parallelizes across files). The engine sees and budgets
  every spawn, like the existing `CMD_COUNT` guard. Metering stays honest.
- **(b) Shell-owned batch — the `xargs` shape.** Group the desired set by kind, hand the
  whole arg set to ONE process that fans internally:

  ```
  sh* gh_batch(urls) -> (url, status) {
      printf '%s\n' {urls} | xargs -P8 -I{} sh -c 'echo "{} $(gh api {} -w %{http_code} -o /dev/null -s)"'
  }
  ```

  This is `insert_rows` for spawns: the engine already collects the set (collect-then-flush
  is the whole ethos), so handing it to one `xargs -P` is the literal subprocess analog. The
  shell owns the parallelism knob (the user's muscle memory); the engine sees one logical
  job.

Tension: (a) keeps the budget/metering visible to the engine, (b) matches the N+1 ban and
the user's shell idiom but the fan-out is opaque to the engine. Likely both: batch by kind,
let `xargs -P` parallelize inside, cap the batch count with a per-poll spawn budget that
mirrors `cmd_budget()`.

### 9.6 Streaming / SSE / pagination: the `sh*` generator (the yield/coroutine direction)

`@async` resolves to exactly one response then flips `done`. A stream is one effect that
yields rows over time: `gh api --paginate`, `tail -f`, an SSE endpoint, a websocket. That is
a generator/coroutine, and it is the genuinely new primitive (Bloom does not have it either;
it is Dedalus' "async with multiplicity").

The `sh*` form (star = yields) is a shell-fn whose child stays alive and whose every stdout
line appends one row to the head relation as it arrives:

```
sh* events(repo) -> (kind, at) {
    gh api "repos/{repo}/events" --paginate --jq '.[] | "\(.type)\t\(.created_at)"'
}

event(repo, kind, at) <- @stream events(repo) -> (kind, at).
```

Runtime: a `sh*` job stays `state=running` (never auto-`done`), its child reparented and fed
through a reader thread that batches lines into the head rel between ticks. This is `@next`
(carry the read cursor / last-seen line) married to a long-running `cmd`. SSR-as-render
(projecting relations to HTML/text) is unrelated — that is an output sink, a derived rel
through a template, orthogonal to the effect layer.

### 9.7 Native HTTP: a sibling `EffectExec`, not a rewrite

`EffectExec` is already the seam. Keep `ShellEffectExec` (built, zero-dep, `gh`/`curl` own
auth+etag). Add `HttpEffectExec` (ureq/reqwest) returning `(status, headers_json, body)` for
the cases that want connection reuse, structured headers, and in-process 304/etag. Dispatch
by the shell-fn's body scheme or an attribute. The etag/304 guard (an `@async` reading last
tick's etag carry to skip an unchanged fetch) is clean native, awkward in shell (`curl -D -`
blob you regex).

### 9.8 Build order (smallest dup-free increments)

1. **Parallel/batch drain** (§9.5a) — smallest, kills the live subprocess N+1, no new
   surface. Reconcile spawns across rayon with a per-poll budget.
2. **`every(N)` clock relation** (§9.3) — retires the global-only knob, small source op.
3. **`sh` template decl** (§9.2) — the language surface; makes effects read like rules.
   Subsumes `effect_cmd` rows. Parse-time arity/hole/output checks.
4. **Job table + reconcile** (§9.4) — `state`/`pid`/`started_at`, kill-by-digest. Unlocks
   process control AND is the precondition for streaming.
5. **`sh*` streaming generator** (§9.6) — the third rule kind (`@stream`), the coroutine
   shape, rides the job table.
6. **`HttpEffectExec` + etag/304** (§9.7) — the conditional-request cache, the last ghcacher
   piece.

## 10. What it unlocks, and what to guard

The structural shift behind §8/§9: v5 stopped being a pure function of the code at an instant.
A tick used to re-derive everything from the current files/revs and forget. The temporal +
effect layer adds three things it never had: **memory** (`@next` carry), **senses** (`@async`
reach outside the repo), and a **heartbeat** (the clock). That is the move from a *linter*
(answers a question about a snapshot) to a *control plane* (remembers the answer, watches for
the conditions to change, acts when they do). Most of the future-world payoff is downstream of
that one shift.

### 10.1 Doors it opens (each rides an asset already in the tree)

| existing asset | + temporal/effect | unlock |
|---|---|---|
| breaking-change detector, blast radius | `@async gh` | auto-comment / request-review on a PR whose blast radius hits a watched symbol — the deterministic *actuator* half of the LLM-brain / dl-executor split |
| dispatch-flow (cross-lang, real SCIP) | poll N repos on a clock | a *live* org-wide call graph, not a one-shot scan |
| `type_edge_rev` / `module_edge_rev` (git history) | the `tx` clock + a `delta` feed | a **second time axis** — observation-time history, not git history ("what did the detector say last Tuesday" without re-running) |
| SCIP importer (batch) | `sh*` stream | ingest a long-running indexer / LSP as it emits, not after it finishes |
| anim atlas (already reads `rel_*` from sqlite) | clock-driven re-tick | a live dashboard off the same tables, no new pipe |
| `--move` refactor | `@async` across repos | propose-and-apply at fleet scale |

### 10.2 Two unifications worth naming now

1. **Derivation keyed by input hash.** The effect cache (`id = blake3(kind, args)`, recompute
   on change), the `cmd` op cache (re-run when file content moves), and `_files` content
   addressing are the *same idea*. etag/304 is that idea aimed at the network. Name it once —
   "a derivation is keyed by its input digest, recomputed only when the digest moves" — and the
   source cache, the effect cache, and memoization collapse into one mechanism. This is the
   conceptual core of the reactive story, not three coincidentally-similar caches.
2. **The fixpoint escape hatch.** Datalog fixpoint is monotone-to-convergence; genuinely
   sequential work (paginate, retry-with-backoff, multi-step negotiation) never fits it.
   `@next` + `@stream` are the controlled imperative seam for exactly those, kept on the `tx`
   spine so they stay inspectable. This is where `yield`/coroutines earn their place: not
   syntax sugar, but the only way to express the sequential algorithms a pure fixpoint can't.

### 10.3 Guards (cheap to design in now, painful to retrofit)

| guard | why | the design move |
|---|---|---|
| **Quarantine nondeterminism** | pure v5 is reproducible, and the RA precision/recall snapshots, oracle diffs, and agg dogfood gates depend on it; `@async` injects network/time/arrival-order | effects live in their own stratum; effect-tainted relations are a *marked subspace*; downstream-pure derivations stay reproducible (the CALM guard, §8.4 cond 4, is the enforcement point) |
| **Mutating effects need exactly-once** | the `done`-flip is at-least-once — a crash between run and `done=1` re-fires; fine for GET, wrong for "open a PR" | an idempotency key / two-phase mark on the job table, decided *before* the first mutating `sh`-fn lands |
| **No DD creep** | the repo deliberately dropped runtime_graph / support-counting; the desired-vs-running reconcile is incremental-maintenance bookkeeping by another name | keep it a flat sqlite reconcile (a `SELECT` diff per poll), never a reactive dependency graph |
| **Capability scoping** | a `sh`-fn is `rm -rf` waiting to happen, and the daemon takes `load <script>` over a socket | decide whether `sh`-fns are declared-and-trusted-at-the-root vs callable-by-any-loaded-snippet |
| **Policy vs mechanism stays split** | if decisions move into rules, you rebuild an agent framework inside a datalog engine (wrong place) | rules express *mechanism* (how to act); the trigger/candidate comes from outside (the refactor-detection ruling: LLM proposes, dl executes) |

The sharpest early decision is the first two: the determinism quarantine and the mutating-effect
idempotency key. Both are nearly free to build into the job table now and expensive to bolt on
once something depends on the loose version.
