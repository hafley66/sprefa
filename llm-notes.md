# llm-notes

User-dictated. Do not add unprompted.

---

## Nesting collapses to one operator

`nest : Cursor → Pipe → Stream<Vec<Cursor>>` = `flat_map_unordered`.

Scalar / stream / reactive are not nesting modes. They are emission profiles of the inner pipe itself:

| pipe shape                  | emits         | completes when            | observed at call site |
|-----------------------------|---------------|---------------------------|-----------------------|
| `term(:X)`                  | 0 or 1 row    | immediately               | scalar                |
| `Fs(...) \| AstNm(...)`     | N rows        | walk done                 | stream                |
| `Select("rule")`            | N + deltas    | gen seal                  | reactive              |

Exactly-one is `... | first` *inside* the inner expression, not a call-site flag.

Caching is intrinsic to the op (pure? deterministic?), not the call site. Key = `blake3(outer_cursor || inner.ident())`.

Reactive cancellation = abort inner task on outer lineage drop (rxjs switchMap analog).

---

## Wire is `Stream<Vec<Cursor>>` end-to-end

Per-op batching. Each op rebatches as it sees fit. mpsc(8) of `Vec<Cursor>` between ops.

```
Pipe : Stream<Vec<Cursor>> → Stream<Vec<Cursor>>
nest : Cursor → Pipe → Stream<Vec<Cursor>>
         inner.run( once([outer]) )
         per emitted batch: batch.map(c => merge(outer, c))
```

Natural batches:
- Fs: 256 paths
- AstNm: rayon chunk size
- Fact: 1024 (amortize lock + diff broadcast)
- Select: 1 per tick or coalesce window
- Print: 1

Cohort identity invariant: an op may NEVER split a regex/pattern capture cohort across cursors. One cursor = one cohort. Document on `Op` trait.

---

## Soundness assessment

**Solid:**
- Pipes-only with batched stream — matches differential dataflow shape
- nest = flat_map — matches rxjs mergeMap
- Two-axis cursor (captures + value-rung ladder, byte_range anchored)
- Rules as dynamic SQL tables, three call modes

**Sharp edges:**
1. Capture cohort identity through `flat_map_unordered` — invariant on Op
2. Lineage propagation must be religious (cancel, cache invalidation, gen seal)
3. Datalog recursion / fixpoint for self-referential rule bodies — name it, defer or implement

---

## LSP trait design

Decomposes into four slots, each maps to existing v4 machinery:

| LSP traffic                                 | v4 home                              |
|---------------------------------------------|--------------------------------------|
| inbound notifications (didChange, diags)    | inward effect → fact bag write op    |
| outbound notifications (publishDiagnostics) | outward effect (drain after seal)    |
| outbound requests (definition, refs, …)     | scalar/stream nest on a request op   |
| inbound responses                           | hidden — completed by request op     |

### `LspMethod` trait (typed per-method)

```rust
trait LspMethod {
    const METHOD: &'static str;
    type Params:  Serialize     + Send + Sync + 'static;
    type Result:  DeserializeOwned + Send + Sync + 'static;
    type Direction: LspDirection;     // ClientToServer | ServerToClient
    type Kind:      LspKind;          // Request | Notification
}
```

Ops parameterized: `LspRequest<M: LspMethod>`, `LspListen<M: LspMethod>`.

### `LspTransport` trait (dynamic JSON)

```rust
trait LspTransport: Send + Sync {
    fn send_request(&self, method: &str, params: Value, id: RequestId) -> BoxFuture<Result<Value>>;
    fn send_notification(&self, method: &str, params: Value);
    fn subscribe_notifications(&self) -> BoxStream<(String, Value)>;
    fn cancel(&self, id: RequestId);
}
```

One transport per `(server_kind, root)`. Lives in Hooks registry. Per-method serialization hidden in `LspRequest<M>::run`.

### LSP state in the store

```
lsp.doc_version    URI → version
lsp.diagnostics    URI, RANGE → severity, message
lsp.symbols        URI → SymbolInformation
lsp.workspace_caps SERVER → capabilities
```

Pure data flow. Rules over LSP bags work without modification.

### Cancellation

Outer lineage drop → in-flight `LspRequest<M>` send `$/cancelRequest` via `transport.cancel(id)`. Same lineage machinery as reactive nest.

### What this fixes vs v3

- LSP wire ownership inside op contract
- LSP state lives in facts, not side channel
- LSP requests don't block ops (nest + switchMap cancel)
- Server lifecycle is Hooks resource, not embedded in op state

### LSP design open

- Capability negotiation handshake, gating ops on `M`'s capability bit
- textDocument sync mode (full vs incremental) — implementation choice per server
- Multi-root workspace: registry keyed by `(server_kind, root)`
- Server-pushed work invalidations (semantic tokens refresh, workDoneProgress) feed rule invalidation channel

---

## Matcher trait — unifying primitive across parser / tree-sitter / LSP / DSL

```rust
trait Matcher: Send + Sync {
    type In:  ValueRung;
    type Out: ValueRung;
    fn ident(&self) -> [u8; 32];
    fn run(&self, input: Self::In) -> BoxStream<'static, Match<Self::Out>>;
}

struct Match<R: ValueRung> {
    refined: R,
    captures: Vec<(Arc<str>, Arc<str>)>,
}
```

| matcher                          | In                  | Out         |
|----------------------------------|---------------------|-------------|
| regex::Captures                  | Bytes               | Bytes       |
| tree-sitter parse                | Bytes               | TreeRoot    |
| tree-sitter query (.scm)         | TreeNode            | TreeNode    |
| ast-grep pattern                 | TreeNode            | TreeNode    |
| LspRequestMatcher<GotoDefinition>| (URI, Pos)          | Vec<Location> |
| LspListenMatcher<PublishDiag>    | Server              | Diagnostic  |
| syntax-template DSL `fn $X`      | Bytes or TreeNode   | TreeNode    |

`MatcherOp<M>` adaptor lifts any Matcher to `Stream<Vec<Cursor>> → Stream<Vec<Cursor>>`. One adaptor, every matcher gets pipe behavior.

---

## Per-language resources

```rust
trait LangResources: Send + Sync {
    fn parser(&self, lang: Lang) -> Arc<Mutex<TsParser>>;
    fn query(&self,  lang: Lang, src: &str) -> Result<Arc<TsQuery>>;
    fn lsp(&self,    lang: Lang, root: &Path) -> Option<Arc<dyn LspTransport>>;
    fn ast_grep(&self, lang: Lang) -> Arc<dyn AstGrepBackend>;
}
```

Lives on Hooks. Ops never construct a parser — ask Hooks. v3 violation: ops + resources lived together.

---

## Syntax template DSL = compile target

`fn $NAME` is surface syntax. Lower-time: `(template_ast, lang) → Box<dyn Matcher>`.

```
template       lang has tree-sitter        lang lacks tree-sitter
"fn $X"        ast-grep pattern matcher    regex matcher (escape literals)
sexp(...)      tree-sitter query matcher   error
url-template   path matcher (no tree)      path matcher (no tree)
```

Slots: `$NAME` → capture name; `${expr}` → carveout, nested pipe; `&.X` → sprf-positional.

DSL is a frontend. Parsing delegated to backend matcher chosen at lower-time.

---

## Tree-sitter as value-axis transitions

| ts operation     | rung transition                    | matcher          |
|------------------|------------------------------------|------------------|
| Parser::parse    | Bytes → TreeRoot                   | TsParseMatcher   |
| Query::matches   | TreeNode → TreeNode                | TsQueryMatcher   |
| Tree::edit       | (TreeRoot, Edit) → TreeRoot        | TsEditMatcher    |

byte_range invariant holds: every TreeNode carries `Range<usize>` into source. Walk down preserves provenance. Walk up = pure projection `bytes[node.range()]`.

Incremental parsing = matcher with input `(TreeRoot, ChangeSet)`. Fact bag for parsed trees keys on `(URI, content_hash)`. Edit → new content hash → new tree → rule invalidation. Same machinery.

---

## Cache layering, one shape, multiple consumers

```
key                                       value           who
blake3(bytes_hash || matcher.ident())     Vec<Match>      pure matchers
blake3(tree_hash  || query.ident())       Vec<Match>      ts queries (tree_hash transitive on bytes)
(URI, version) → tree                     TreeRoot        parser cache
(URI, version) → diags                    Vec<Diagnostic> lsp listen
```

LSP request results NOT cached as pure matchers — server is authoritative. Reactive-nest treatment: subscribe to invalidations, re-fire.

---

## Trait stack

```
DSL surface
  ├─ "fn $X"             template_ast
  ├─ sexp(...)           query_ast
  └─ url-template-style  path_ast
            │  lower-time
Matcher (parametric on In/Out rungs)
  ├─ regex::Captures
  ├─ TsParseMatcher
  ├─ TsQueryMatcher
  ├─ AstGrepMatcher
  └─ LspRequestMatcher<M>, LspListenMatcher<M>
            │  MatcherOp adaptor
Op (Stream<Vec<Cursor>> → Stream<Vec<Cursor>>)
            │
Pipe runner (existing)
            │
Hooks resources (LangResources, LspTransport)
```

Three traits: `Matcher`, `LspMethod`, `LangResources`. One adaptor: `MatcherOp`. One registry: Hooks.

---

## Locks needed before coding

1. `ValueRung`: enum with `String | Bytes | TreeNode | AstNode | LspParams | LspResult`, or open trait. Likely enum (set is small/stable).
2. Backend selection at lower-time vs runtime. Static + `MultiLangTemplateMatcher` escape hatch.
3. `ident()` per matcher: regex hashes pattern string; ts query hashes query source; LSP request hashes method + params shape (not values).
4. Long-lived matchers' shutdown: tie to lineage drop, same as reactive nest cancel.
5. Matcher errors: side-channel diagnostic bag (queryable as facts).
6. Language detection: `Detect` matcher `(URI | Bytes) → Lang`, caches on URI or content hash.

---

## `[]` = implicit 1st arg (receiver), defaults to `&.value`

Bracket slot taxonomy finalized:

| slot | meaning                     | default               |
|------|-----------------------------|------------------------|
| `()` | primary args, DSL, params   | required by op sig     |
| `[]` | receiver — implicit 1st arg | `&.value` (per op sig) |
| `{}` | body                        | optional               |

`op(...)` lowers to `op[&.value](...)` unless op declares a different default receiver.

### Effect on `term` / `eq` / `re` (kills two-arg forms)

| sugar                | lowered                       | meaning                            |
|----------------------|-------------------------------|------------------------------------|
| `term(:X)`           | `term[&.value](:X)`           | unify capture X with value-axis    |
| `term[&.A](:X)`      | (already lowered)             | unify capture X with capture A     |
| `eq($Y)`             | `eq[&.value]($Y)`             | assert value-axis == bound Y       |
| `eq[&.A]($Y)`        | (already lowered)             | assert capture A == bound Y        |
| `re("^x$")`          | `re[&.value]("^x$")`          | regex on value-axis                |
| `re[&.path](".rs$")` | (already lowered)             | regex on `path` capture            |
| `AstNm("fn $X")`     | `AstNm[&.value]("fn $X")`     | match on current value rung        |
| `AstNm[&.body]("…")` | (already lowered)             | match on `body` capture's content  |

Two-arg `term(:A, $X)` form deleted. Receiver is LHS; arg is RHS.

### Bareword sugar chain (three stages)

```
TERM
  → term(:TERM)
  → term[&.value](:TERM)
```

```
TERM?
  → term?(:TERM)
  → term?[&.value](:TERM)
```

Op definitions only handle the lowered form.

### Receiver path grammar

```
receiver  ::= '&' '.' path
path      ::= ident ( '.' ident )*
```

Examples: `&.value`, `&.captures.NAME`, `&.lineage`, `&.gen`. Open: whether `&.X` is unambiguous shorthand for `&.captures.X`.

### Op signature names receiver shape

```rust
trait Op {
    fn receiver_default(&self) -> ReceiverPath;   // e.g. ValuePath, CapPath("path"), None
    fn receiver_rung(&self) -> Option<ValueRung>; // expected rung at receiver site
}
```

Receiver path must resolve to compatible value-rung at lower-time. Type error if not.

### Tradeoff

`[]` was "cursor refs"; now specifically "receiver = which cursor field this op operates on". Same slot, sharper meaning. Visual density rises slightly when both `[]` and `()` appear; default-elision keeps common case clean.

Parser/lowering only. Runtime untouched.

---

## Closure analysis — hoist pure subtrees to SQL

Discriminator is op effect class, not DSL holes. A capture-ref in a DSL is still relational.

| op class       | examples                                  | relational? |
|----------------|-------------------------------------------|-------------|
| Pure           | `eq`, `re`, `term`, `Filter`              | yes         |
| Param          | `eq[&.A]($X)` with $X bound at lower-time | yes (parametric) |
| Relational     | `Select`, `Antijoin`, `GroupCount`, `join`| yes         |
| IO             | `Fs`, `read`, `GitFetch`                  | no          |
| Effectful      | `Sh`, `LspRequest`, `Fact "..."` (write)  | no          |
| Stateful       | `LspListen`, broadcast subscribe          | no          |

Subtree is **relational-closed** iff every op ∈ {Pure, Param, Relational}, every nested sub-pipe is relational-closed, every free capture binds to ground at enclosing scope.

### Examples

**Closed — full SQL:**
```
Select "fns" | eq[&.MODULE]("std") | join "uses" on NAME
```
→ `SELECT fns.*, uses.* FROM fns JOIN uses USING(NAME) WHERE fns.MODULE='std'`

**IO source, relational sink:**
```
Fs(roots=["."]) | AstNm("fn $X") | Fact "fns"
```
Not closed (Fs is IO). Pipe runtime ingests; output is SQL table.

**Closed nest — correlated subquery:**
```
Select "fns" | nest( Select "uses" | eq[&.NAME]($outer.NAME) )
```
→ `SELECT * FROM fns f WHERE EXISTS (SELECT 1 FROM uses u WHERE u.NAME = f.NAME)`
   (planner rewrites to JOIN.)

**Mixed nest — escapes closure:**
```
Select "fns" | nest( Sh("git log $FILE") | parse_log | Fact "log" )
```
Outer Select compiles to SQL. Inner runs on pipe runtime per outer row.

**Closed inside non-closed:**
```
Sh("ls -la") | parse_lines | nest( Select "fns" | eq[&.PATH]($outer.line) )
```
Outer pipe runtime. Inner = prepared SQL stmt parameterized by `$outer.line`.

### AST tagging

```rust
struct PipeNode { op: Op, children: Vec<PipeNode>, closure: Closure }

enum Closure {
    PureRelational,
    ParamRelational(Vec<Cap>),
    Mixed,
}
```

Bottom-up: leaf = own class; internal = meet (Mixed if any child Mixed); Param promotes to Pure if pushed past binder.

### Backend selection

| closure         | backend                                            |
|-----------------|----------------------------------------------------|
| PureRelational  | SQLite / DuckDB / DataFusion / DD-plan             |
| ParamRelational | SQLite prepared stmt / DD parameterized arrangement|
| Mixed           | pipe runtime, with closed subblocks compiled       |

### Plan shape

```
sprefa source
    ▼ parse + lower
PipeNode tree (closure-tagged)
    ▼ planner
Plan {
    sql_subtrees: Vec<SqlPlan>,
    pipe_subtrees: Vec<PipePlan>,
    boundaries: Vec<(SqlOut, PipeIn)>,
}
    ▼ executor
SQL backend runs closed parts; pipe runtime runs IO/effects; boundaries wire across.
```

Fact bags = SQL tables. Rules = views (or DD arrangements). Ingestion writes tables. Closed queries go through SQL planner for free correlated-subquery-to-join etc.

### Locks needed

1. Closure tag on `Op` trait; default `Mixed` if unsure
2. Capture flow analysis (def-use over lowered tree) for Param promotion
3. SQL target: SQLite for v0, DataFusion or DD as alts
4. Fact bag schema typing — SQL needs typed columns; current `Arc<str>` captures need coercion or threaded types
5. Boundary batching: pipe → SQL via transaction; SQL → pipe via cursor stream
6. Recursion across boundary: CTE+materialize or fall back to pipe

---

## Session 20260503.3 — DD parked, sqlite POC, two-tick render/commit

### Decision

DD lab proved out (12× rule_work / 9× per-commit at medium scale, parity verified) but value disappears once the actual workload shape is laid out:

- File save / AI edit / ghcacher sync = bursty commits, not keystroke-frequency
- Content-hash on cursor + OpCache covers the parse cost (where 70% of wall lives)
- switchMap-on-burst (effect_runtime cancellation) replaces "incremental update under storm" with "abandon and restart"
- Sinks are write-through: pipeline holds no aggregate-while-running. Stores hold what their semantics require.

DD's only remaining win: incrementally update a *held in-memory aggregate* under writes. For sprf workload (debounced commits, indexed queries, settled-state reads), recompute on commit beats incremental.

DdStore stays in lib.rs as a parked proof point — not deleted, not on the active path.

### React mental model (per human-goals.md confirmation)

```
React                          sprf
─────────────────────────────  ────────────────────────────────────
component                      op
props                          cursor terms (the live ones)
fiber.key                      cursor.lineage_hash
React.memo / bailout           OpCache hit on (op_ident, lineage)
render phase                   pipeline drain (tick 1)
commit phase                   Fact / sink writes (tick 2)
useEffect                      effect_runtime PureEffect
useTransition / Suspense       Pause / yield / next?()
concurrent mode                switchMap-on-burst, abort-and-restart
state                          Store (mem | sqlite)
context                        Hooks
```

CI mode = single tick (render only, no commit-side reactivity).
Reactive mode = two ticks (render → commit → optionally re-fire dependent rules).

### Joins via brute-force per-batch IN(?) collapse (Haxl/DataLoader)

Naive shape:
```sprf
fact(:files, ${FID?}, ${PATH?})
  > fact(:refs, ${FID}, ${LO?}, ${HI?})   -- ${FID} bound from upstream
  > GroupCount(:FID, count="N")
```

FactRead op runs per-batch:
1. Collect bound key values from input batch
2. ONE `WHERE key IN (?, ?, ...)` query
3. Group result by key
4. Cross-product redistribute back to input cursors

NO SQL-prefix lowerer (the trait-method-compiles-to-multistatement-SQL idea was rejected as premature). Each fact-read is its own IN-query. SQLite plans nothing across pipe steps.

### Bound/unbound captures via cursor.terms

Already present:
- `${X}`  bound = X must be in cursor.terms = WHERE clause
- `${X?}` unbound = X not in cursor.terms = SELECT projection

FactRead takes Vec<TermSpec> at construction. No new types; reads existing cursor terms hashmap to drive the query shape.

### Memory budget (bounded per layer, no piece scales with corpus)

```
pipeline transient    ≤ workers × pipe_depth × BATCH cursors  (~3 MB)
store read            row-streaming Statement::query (1 row mat'd)
store write           prepared INSERT in txn, fsync per commit
cache (OpCache)       LRU bounded by total cursor count
held aggregates       sqlite-sink → 0; mem-sink → bounded by rule rows
```

WAL keeps reads from blocking writes. Connection pool for parallel rules.

### POC scope (what lands)

```
1. SqliteStore — :memory: first, write-through, prepared INSERTs    ~300 LOC
   ensure_schema → CREATE TABLE IF NOT EXISTS
   write_batch  → prepared INSERT, txn batches per commit
   read         → prepared SELECT WHERE col IN (?), row-streaming

2. FactRead op — bound/unbound term spec, IN-batch collapse         ~200 LOC

3. FactWrite op — schema decl on first run, batched txn flush       ~30 LOC

4. Schema on Rule — Rule { name, body, sink: { fact, schema } }     ~50 LOC

5. Cursor.lineage_hash — Fs writes content_hash, no cache yet       ~100 LOC

6. Two-tick driver — render → commit, CI stops at tick 1            ~150 LOC
```

~830 LOC total. NO DD on active path. NO SQL-prefix lowerer. NO liveness pass. NO OpCache decorator yet. NO switchMap-on-burst yet.

### What's NOT in POC (deferred, sequenced after)

- OpCache decorator with lineage-hash key
- Liveness analysis pass (drop dead cursor terms at op boundary)
- SQL-prefix lowerer (auto-collapse pure-fact rule body to one SQL stmt)
- switchMap-on-burst driver in effect_runtime
- Multi-store routing (Hooks.stores by-fact)
- Antijoin under DD parity
- Rule unification (RuleBody enum → `Rule { body: Vec<Op> }`)

### Why park rule unification (a)

Was top of next-session queue from 20260503.2. POC needs Schema-on-Rule, but the body lift adds churn without unblocking sqlite. Sqlite proves the shape; rule unification refactors after sqlite is sound.

### Open

- `:memory:` first, on-disk later. WAL pragma needed even for `:memory:`? No, irrelevant.
- Multiple connections vs single Mutex<Connection>: single until benchmark says otherwise.
- Schema migration: skip. POC re-CREATE on each run.
- Bench: parity check across mem + sqlite on the same pipeline (analog of mem/dd parity in 20260503.2).

---

## Op = Component. render returns CursorNode (= ReactNode).

The core insight from the 20260504 design jam. Renames `step → render`, `StepResult → CursorNode`. The trade is real, not cosmetic: it imports React's discriminated-union vocabulary as a mental model — Suspense, Fragment, Mount, key-based reconciliation — and pairs it with RxJS's temporal flow. The substrate that runs it is a queue. The queue backend is pluggable; sqlite is one impl, not a coupling.

### Why this rename earns its keep

1. **Pause stops being a unique primitive**. It's `Suspense{wake}`, one variant of the union. Every variant lowers to "write some queue rows, optionally with a non-IMMEDIATE wake condition." One INSERT shape, parameterized.
2. **Higher-order map is overrideable per render call**. The op author returns a structured CursorNode tree; the tree shape *is* the routing/concurrency policy. No global `pipe_merge_map` config knob. mergeMap / concatMap / switchMap are patterns, not primitives.
3. **External producers (interval, timer, fs-watch, lsp-event) collapse**. They're ordinary ops returning `Effect{...} + Mount{self}`. No separate `ProducerOp` trait.
4. **Cancellation = unmount**. React's reconciliation by key is the whole story: when a parent re-renders with a new mount key, the prior subpipe instance is reaped (queue rows deleted by `instance_id`). Works identically for parked and active children.
5. **List-monadic, not rxjs-monadic** (see signals section below). The bind operation is `Many` flatten, not `flatMap` over async streams. Composition is plain list semantics. The reactivity comes from the queue layer underneath, not from the type-level monad.

### The union

```rust
enum CursorNode {
    Emit(Cursor),                                        // leaf: cursor flows downstream
    Many(Vec<CursorNode>),                               // fragment: flatten, all flow
    Suspense { cursor: Cursor, wake: Wake },             // throw-promise: park, wake on condition
    Mount  { subpipe: PipeRef, key: Cursor, input: Cursor }, // switchMap: keyed subpipe instance
    Effect { eff: Effect, then: Box<CursorNode> },       // useEffect: side effect + continuation
    Done,                                                // null: consume cursor, no output
    // Portal { to: PipeRef, node: Box<CursorNode> },    // routing primitive — deferred, may not need
}
```

Mapping to prior art:

| variant | React | RxJS | what it does |
|---|---|---|---|
| `Emit(c)` | text node / leaf | `next(c)` | next op in the pipe consumes |
| `Many([..])` | `<>...</>` Fragment | `concat(o1, o2)` | flatten N siblings, all flow downstream concurrently |
| `Suspense{wake}` | `throw Promise` | `merge(...).pipe(skipUntil)` | park cursor, wake on relational condition |
| `Mount{subpipe, key}` | `<Comp key=k input=p />` | `switchMap(_ => sub$)` | instantiate subpipe; new key unmounts prior |
| `Effect{eff, then}` | `useEffect` | `tap` / `mergeMap(_=>side$)` | dispatch effect, response stitched into continuation |
| `Done` | `null` | `complete()` | consume cursor, no output |
| ~~Portal~~ | `createPortal` | route operator | deferred; default flow + Mount cover the cases we have |

### Why Portal is parked

The user landed on: "we don't need Portal." Reasoning:

- **Default flow is downstream**. `Emit(c)` lands at pc+1 of the same pipe. That's 90% of routing.
- **Mount covers cross-pipe**. `Mount{subpipe, key, input}` is the explicit "send this cursor into a different pipe instance." Has reconciliation built in.
- **Tag-based broadcast lives in op-land**, not as a node variant. An op writes to a tag-sink; subscribed pipes drain. Routing via the relational substrate, not via the node tree.
- **Tee = `Many([Emit(c1), Mount{...}])`** — fan a cursor to default-pipe + a named subpipe. No Portal needed.

Kept commented in the enum so the design space is visible. Reactivate if a real use case arrives that Mount can't express.

### Routing without Portal — three patterns that cover the space

```rust
// 1. Default forward: cursor continues this pipe
Emit(c)

// 2. Hand off to a named subpipe (with restart/key reconciliation)
Mount { subpipe: PipeRef::Named("diagnostics"),
        key: c.term(":diag_id").into(),
        input: c }

// 3. Tee — local continuation + sidecar
Many(vec![
    Emit(local_continuation),
    Mount { subpipe: PipeRef::Named("audit"), key: ..., input: ... },
])
```

### How variants lower to queue rows

| variant | queue rows produced |
|---|---|
| `Emit(c)` | 1 row, `wake=IMMEDIATE`, `pc=pc+1` |
| `Many([n1, n2, ...])` | recurse — flatten N children, each to its own row |
| `Suspense{c, wake}` | 1 row with the supplied wake_kind; same op or pc+1 (design call below) |
| `Mount{subpipe, key, input}` | 1 row in `subpipe`, `pc=0`; upsert in `mount_registry(key, instance_id)` |
| `Effect{eff, then}` | 1 row in `effect_queue`; on response, 1 row from `then` chained IMMEDIATE |
| `Done` | 0 rows |

One driver loop. One `enqueue()` call shape. The variants are different fillings of the same pasta.

### Render examples (the four classic Rx fan-out modes)

```rust
// mergeMap (concurrent, default) — Many of Emit
fn render(&self, c: &Cursor) -> CursorNode {
    Many(parsed_chunks(c).into_iter().map(Emit).collect())
}

// concatMap (sequential) — chain via Effect+await
fn render(&self, c: &Cursor) -> CursorNode {
    Effect {
        eff: Effect::AwaitSink(prev_done_for(c)),
        then: Box::new(Many(parsed_chunks(c).into_iter().map(Emit).collect())),
    }
}

// switchMap — Mount with key. New key = prior subpipe unmounts.
fn render(&self, c: &Cursor) -> CursorNode {
    Mount {
        subpipe: PipeRef::Named("inner_search"),
        key:     c.term(":query").clone().into(),
        input:   c.clone(),
    }
}

// interval / timer producer — Effect chained back to self
fn render(&self, _: &Cursor) -> CursorNode {
    Effect {
        eff: Effect::Tick(self.period_ms),
        then: Box::new(Many(vec![
            Emit(self.tick_cursor()),
            self.remount_self(),
        ])),
    }
}
```

The op author *picks the shape per call*. Two cursors flowing through the same op can take different shapes depending on input. That's the "higher-order map overrideable" property — there's no single `merge_strategy` field on the op; the policy is data the op returns.

### Pause does not block siblings

The 20260504 worry was: if one cursor in a batch pauses, does it block the rest? Answer: no. Each cursor is its own queue row, its own render call, its own state machine instance. A pause is a `Suspense{wake}` returned by *one* render call. The driver writes that one row with a non-IMMEDIATE wake_kind and moves to the next runnable row. Siblings at the same pc are independent.

The only thing a pause "blocks" is the cursor's own future — children that *would have* been produced by the cursor's later op steps don't exist yet, so there's nothing to block.

### Indexing scheme

Three-axis identity per queue row:

```
queue_row(
  id              -- monotonic total order (FIFO tie-break)
  parent_id       -- causal lineage: which cursor emitted this
  batch_idx       -- position within parent's emit Vec (for stable sibling order)
  pipe_hash       -- which pipe / which Mount instance
  pc              -- next op index in the pipe
  cursor_blob     -- serialized terms, content-addressed (no Arc identity, no mtimes)
  wake_kind       -- IMMEDIATE | TICK | SINK_GEN | EXTERNAL
  wake_sink, wake_key_pfx, wake_past_gen  -- predicate for SINK_GEN
  drive_tick      -- global gen at enqueue (DD frontier equivalent)
)
```

`drive_tick` advances on every sink commit. Scheduler picks runnable rows by `(wake_kind=IMMEDIATE, drive_tick ASC, id ASC)` — gives DD frontier ordering.

`(parent_id, batch_idx)` makes fan-out replayable: a cursor at `(pipe_hash, pc, parent_id, batch_idx)` is uniquely identified across runs. Effect responses keyed by this tuple are reusable on restart (the event-sourced hybrid from the 20260504 agent review).

### Backend is pluggable. Sqlite is one impl, not the contract.

Hard rule: **sprefa semantics don't depend on sqlite.** The state machine is "step one queue row, render produces a tree, flatten tree to N rows, write them, delete consumed." Any backend that can do `enqueue / pull_runnable / delete / wake_index_lookup` works.

```rust
trait QueueBackend: Send + Sync + 'static {
    fn enqueue(&self, row: QueueRow) -> Result<u64>;
    fn pull_runnable(&self, sink_gens: &[Gen]) -> Result<Option<QueueRow>>;
    fn delete(&self, id: u64) -> Result<()>;
    fn wake_index_lookup(&self, sink: SinkId, key_pfx: &[u8], past_gen: Gen)
        -> Result<Vec<u64>>;
    fn unmount(&self, instance_id: InstanceId) -> Result<u64>;  // delete-by-instance
}

struct MemQueue    { /* tokio mpsc + BTreeMap<wake_index, RowId> */ }
struct SqliteQueue { /* the table above + indexes */ }
// future: RedisQueue, PostgresQueue, FoundationDBQueue, ...
```

Driver code is identical against any backend. Same op author surface. The choice is throughput-vs-mem-vs-durability, not semantics.

Where the boundary leaks are forbidden:
- No `SELECT` in op code. Ops never touch the queue directly.
- No sqlite-specific types in CursorNode or Op trait. Cursors are content-addressed bytes.
- No assumption that wake-index lookup is exact-match — must accept key prefix range. Both Mem and Sqlite implement this; future backends will too.

The two-tier layout (RAM hot + disk cold) is one impl strategy; the trait stays single.

### Signals as the alternative bowtie

The user noted: *"a different bowtie, but we have all these linkages that are not monadic, not rxjs monadic, but just monadic like arrays/lists."*

This is the right frame. There are three reactivity bowties; sprefa's queue is one, signals is another, rxjs is a third:

| bowtie tied at | model | composition | what gets tracked |
|---|---|---|---|
| **render boundary** (sprefa current) | render fn returns CursorNode tree, queue flattens | list-monadic via `Many` flatten | queue rows + sink gens |
| **read site** (signals: Solid, Vue, Preact) | `useSignal()` registers dep on access; setter walks dep graph | implicit graph via tracker | per-value subscriber lists |
| **operator chain** (rxjs) | `.pipe(map, filter, switchMap)` produces a new stream type | type-level monad with effect coloring | observable subscriptions |

The sprefa shape is **list-monadic, not stream-monadic**. `Many([a, b, c])` is `[a, b, c].flat()`. `Mount` is a list with a continuation. There's no async monad in the type. The async-ness lives one layer down, in the queue's wake mechanism. Render is a pure function; reactivity is a property of the runtime, not of the return type.

A signals reformulation would tie the bowtie at the read site instead:

```rust
// hypothetical signals surface
fn render(&self, c: &Cursor, sigs: &SignalCtx) -> CursorNode {
    let active_repos = sigs.read::<Sink>("repos");  // registers dep
    Many(active_repos.iter().map(|r| Emit(c.with_repo(r))).collect())
}
// when "repos" sink commits, runtime re-invokes render for any cursor
// that read it. Dep graph is implicit, per-cursor.
```

This is appealing but loads the dep tracker with cursor identity (millions of subscribers per sink in a 500-repo run). The queue model amortizes this via the wake-index lookup — a single SELECT wakes N parkers. Signals would need an indexed dep graph to match, which is roughly what we already have in the queue.

**Verdict on signals**: keep as a possible re-skin of the surface, not the substrate. The render-returns-CursorNode shape can be backed by either a queue or a signal graph. We pick queue for restart-survival + scaling-by-disk; we'd pick signals for tighter UI-style reactivity if/when sprefa grows a UI subsystem (LSP code lens with live deps could be one).

The list-monadic core is the load-bearing thing. Both substrates can serve it.

### Atomicity rule

Render is atomic per cursor. An op can return `Suspense{wake}` to pause, but it can't pause *inside* a render call. Mid-render pause is impossible by construction.

To pause inside what feels like one logical step, the author splits it: op A renders a request cursor + Suspense; op B in the next pc renders the resumed work. The pause point lives between ops, at a queue boundary, not inside one op's stack. This rule is what makes "pure ops replay" trivial — every render call is a single function from cursor to CursorNode, with no hidden suspended state.

### What collapses (recap)

| before | after |
|---|---|
| `Yield` + `AwaitSink` + tick/restart bespoke logic | one `enqueue(row, wake_kind)` shape |
| `pipe_merge_map` op-config knob + per-op concurrency param | tree shape returned by render is the policy |
| separate ProducerOp trait for interval/fs-watch/timer | ordinary ops returning `Effect+Mount` continuation |
| pause-blocks-pipe worry | each cursor is its own row, siblings unaffected |
| custom cancellation propagation | `unmount(instance_id)` deletes-by-tag |
| O(active) + O(parked) RAM | O(1) with Sqlite backend, O(active) with Mem backend; same code |
| sqlite-bound state machine | trait-bound state machine; sqlite is one of N backends |

### Open design points

- **Suspense and pc**: when a render returns `Suspense{wake}`, does the resumed cursor re-enter at `pc` (same op renders again with awaited result threaded in) or at `pc+1` (op consumed the cursor, next op gets it)? Lean: `pc+1`, with `Effect{eff, then}` covering the "render again with response" case explicitly. Keeps Suspense a pure pause, not a re-render trigger.
- **Mount key hash**: `Mount.key: Cursor` lets the key be structured. Need a stable canonical hash. Lean: `blake3(canonical_encode(cursor.terms))` with bytes-not-ids interner discipline (per agent review failure-mode #4).
- **Many ordering**: `Many([a, b, c])` siblings are concurrent by default. Sequential is opt-in via `Effect{await prev_done, then}`. No implicit ordering, document loudly.
- **Effect.then evaluation timing**: closure inside `then` evaluated at queue-fold time (when response arrives) vs op's render called again with response in input cursor. Lean: re-render with response, since "render is a pure function of cursor" is the load-bearing invariant. Means `Effect.then` is a *cursor template*, not an arbitrary continuation.
- **Mem backend's parity story**: SqliteQueue gives restart survival for free. MemQueue can't. Either accept the asymmetry (Mem = ephemeral, like RAM) or build a periodic Mem→Sqlite snapshot. Almost certainly the former — Mem is the throughput backend; durability is what you switch to Sqlite for.
- **Pipe authoring surface**: today pipes are `Vec<Box<dyn Op>>`. With Mount, pipes can be cursor-term values (subpipe references). Need a registry: name → pipe definition. Probably `PipeRegistry` as a `Hooks.stores` entry, parallel to `mount_registry`.
