# v3 runtime shape — process tree, op-calls, saga interpreter

Captured from session 20260427.14. The architecture sprefa is converging on,
written as a single layered sketch so it survives context loss. Authoritative
until something here disagrees with code; then update both.

## Framings that lock the model

- **Job scheduler**: the runtime layer is a job scheduler. Ops are jobs, op-calls are scheduled invocations, the effect engine is the I/O dispatcher.
- **rxjs as process tree manager**: every Observable subscription is a process; share/merge/takeUntil are process composition; root subscribe is `init`, op-call is `fork`, takeUntil is `kill`.
- **Saga interpreter**: ops yield pure Effect descriptors; the runtime is the saga interpreter. `put` is an Observable constructor that emits onto a Subject; `take`/`call` is yielding to wait for another stream.
- **Const-golf userland**: zero `let`/`mut`/statement-level state in userland code. Every Pipeline IS a pure value (Clone + Send + 'static). Pure values ⇒ cloneable, sendable, persistable ⇒ saga durability is structural, not a serialization layer bolted on.
- **Set semantics, not sequence**: the language is orderless. Order is an IO concern (sh, render, write_file), not a graph concern. The four mapping ops are cousins, picked per-node by what the node's job is:

  | op | sweet spot | cost when misused |
  | --- | --- | --- |
  | `switch_map` | latest-wins / supersession; prior work becomes stale on new input. LSP per-request, watch reruns, parametric rule re-call, any "react to latest" node. The inner subscription IS the OpCall for that node — cancel-prior comes free. | drops emissions if outer faster than inner; wrong for accumulators |
  | `merge_map` | set accumulation, fan-out where every inner emission is wanted. Per-cursor expansion inside an op. | unbounded in-flight without concurrency cap; reorders |
  | `concat_map` | ordered side effects, sequential stages where ordering IS the semantic | latency stacks; deadlocks against perpetual inners |
  | `exhaust_map` | first-wins; ignore new outers until current inner completes. Debounce-by-completion. | drops everything during in-flight inner |

  No global "always X / never Y" rule. Each node picks the operator that matches its job. The set-semantics rule rejects only the case where ordering is borrowed for free without earning it.
- **Server event = `eval(sprf_text)` (theoretical).** LSP onChange / onDidOpen / onRequest are themselves OpCalls. A buffer's content is, conceptually, `eval(str(file))`. No actual `eval` primitive yet; the framing locks intent — server is a sprefa program, events are native to the language, any reachable .sprf file could be auto-eval'd into the runtime.
- **`put` is not a keyword, it's a method on PipeCtx.** The saga interpreter handle travels in-band with the chunk: `chunk.pipe_ctx.put(":fns", row)` is what `yield put(...)` would have been. Same for `take`, `call`, `fork`, `cancel`. No descriptor enum, no central interpreter switch — methods on a context handle that the runtime owns.

## Layers

```
LAYER 0 — EFFECT ENGINE                    not-rx; request-response with batching
  Batcher per EffectKind                   coalesces same-tick same-shape calls (Haxl)
  PureEffect descriptors                   ReadBytes, FsList, Print, WriteRange, ...
  ObservableTap (already lives in          taps every req/resp pair onto SharedSubjects
    effect_runtime/src/rx.rs)              — bridge from non-rx engine into rx land

LAYER 1 — PROCESS ROOT (RtCtx)             long-lived; durable state
  effects: EffectEngine                    the dispatcher
  relations: RelationStore                 tag bags as ReplaySubject<Row>
  rules: Map<RuleId, Pipeline>             lowered pure values, ready to subscribe
  fs_reader: ReaderCache                   memoized by (repo, rev, key)
  shutdown$: Subject<()>                   process-level kill signal
  pending_count$: BehaviorSubject<usize>   global pending (telemetry)

LAYER 2 — OP-CALL                         per-OP-ACTIVATION lifecycle envelope
  granularity                              one OpCall per op activation, NOT per
                                           pipeline subscribe. tree mirrors lowered
                                           Pipeline syntax. same op subscribed twice
                                           = two OpCalls (stateless gen-of-gens
                                           guarantee made operational).
  aliases                                  rxjs Subscription / PL activation record /
                                           saga Task / structured-concurrency scope.
                                           sprefa biases toward "scope" reading
                                           because parent chain matters.
  pending: AtomicCounter                   local to this activation
  sources_done$: BehaviorSubject<bool>
  cancel$: Subject<()>
  quiet$ = pending == 0 ∧ sources_done     local fold:
            ∧ ∀ child: child.quiet$        a node is quiet iff itself + all children
                                           are quiet
  takeUntil$ = merge(quiet$, cancel$,      outer kill = quiet OR cancel OR shutdown
                     parent.shutdown$)
  parent: RtCtx | OpCall                  stack containment for nesting
  scope_frame: Bindings                    args for parametric rules; bound once
                                           at construction, read by the activation

  one of these per:
    - each op activation in a lowered      fs / re / tag_write / etc. each get one
      Pipeline                             when subscribed
    - sprefa-run --once                    outer driver = root activation under RtCtx
    - sprefa-lsp request                   per-request root activation
    - rule call                            opens a sub-tree mirroring the rule body
    - dynamic rule call                    same as static rule call but resolution
                                           happens at activation time

  tree shape mirrors syntactic structure of the lowered Pipeline. drop a node →
  cascade unsubscribe through subtree. durable RtCtx state untouched.

LAYER 3 — PIPELINE                         pure value, NOT live until subscribed
  type Pipeline = Fn(OpCall, Observable<Chunk>) -> Observable<Chunk>
                  Clone + Send + Sync + 'static
  composition alphabet:
    seq(a, b)         a then b              switch_map shape
    fork(arms)        broadcast              share + merge_streams
    rule(name, body)  scoped op-call       opens child OpCall on subscribe
    call(name, args)  dynamic dispatch      stack-containing reentry

LAYER 4 — INDIVIDUAL OP                    Stream<Chunk> => Stream<Chunk>
  Chunk = { cursors: Arc<[Cursor]>,        pipe_ctx travels with the data;
            pipe_ctx: PipeCtx }            ops method-call effects in-band
  PipeCtx exposes:
    put(name, row)                         tag write (saga `put` reified)
    take(name)                             tag read / await stream emit
    call(rule, args)                       open child OpCall (rule call)
    fork(child_pipeline)                   detached OpCall under same parent
    cancel(opcall)                         cancel any reachable OpCall
    effect(eff)                            dispatch PureEffect, get Single<T>

  fs(glob)          source op               pipe_ctx.effect(FsListFilesEffect); emits Chunk
  re(pattern)       byte-reader op          loadContent → regex → bound captures
  tag_write(name)   Subject sink            chunk.pipe_ctx.put(name, row)
  tag_read(name)    Subject source          chunk.pipe_ctx.take(name); takeUntil(sub)
  ... 20 ops total, each is a Pipeline-shaped fn

LAYER 5 — PER-CURSOR REACTIVITY            inside the per-op mappers
  chunk arrives → from(chunk) expand →
    per-cursor merge_map →
      loadContent(c) (effect engine batches) →
        op body runs on hydrated content →
          emits 0..N child cursors with new bindings →
            toArray()/Arc → flows downstream as new chunk

LAYER 6 — TEAR-DOWN CASCADE                drop the op-call → bottom-up death
  sub.cancel$ fires
    → takeUntil$ unsubscribes pipeline
      → each op's mapper (switch/merge/etc) unsubscribes upstream
        → tag Subject subscriptions detach (Subject lives on at rt)
        → loadContent's pending Single drops → finalize fires
          → effect engine pending counter decremented
        → relation_store subjects: subscriber_count--
    → durable state at rt.* untouched
    → process root keeps running for next request
```

## Rules of the model

1. **Nothing completes inside the data graph.** All sources are Subjects (theoretically infinite). Termination is a consumer-side `takeUntil(intent$)` keyed on external intent (request lifecycle, --once flag, Ctrl-C, shutdown).

2. **Quiescence is the default termination signal.** `quiet$ ⇔ pending == 0 ∧ sources_exhausted`. Per op-call, NOT global. One-shot CLI = `takeUntil(quiet$)`. Long-lived watch = no takeUntil.

3. **Park = `Poll::Pending` in pull-shape.** Producer-side suspension is dual to consumer-side polling. Not a 4th primitive.

4. **Lowered-key sharing.** Two seeds with identical lowered filter share one Observable. `share()` keyed on `(repo, rev, lowered_op_args)`.

   **`share = Arc::clone of a single Subject.`** No further mystery. Multi-cast = one Subject behind an Arc; subscribers are independent; producer side writes once.

5. **Static BindingGraph rejects unbound `${X}` at lower-time** for statically-resolvable cases. Dynamic rule call sites get runtime Pending. Two-tier, not one.

6. **Userland never touches `let`.** Every binding is a `fn` item or a top-level static. Pipelines compose via fn calls and operator sugar (`>>` for seq, `|` for fork). The runtime internally uses whatever it needs; the constraint is the userland surface.

## Saga interpreter mapping

| redux-saga                   | sprefa runtime                                       |
| ---------------------------- | ---------------------------------------------------- |
| saga = generator             | Pipeline = pure Fn value                             |
| effect descriptor            | PureEffect (ReadBytes, FsList, Print, WriteRange)    |
| `call(fn, args)`             | dispatch effect → Single<T>                          |
| `put(action)`                | Observable ctor that emits onto a Subject           |
| `take(pattern)`              | yield to wait for another stream (Subject filter)    |
| `fork(saga)`                 | open child OpCall, doesn't await                    |
| `spawn(saga)`                | open detached OpCall, parent doesn't track          |
| `cancel(task)`               | op_call.cancel$.next()                              |
| saga interpreter             | the runtime itself                                   |
| middleware sees descriptors  | EffectEngine sees PureEffect; Batcher batches        |

The user-surfaced framing: `put` is an Observable constructor that takes a Subject; yielding `put` is "emit onto this Subject and don't wait". Yielding `take` is "subscribe to this Subject, complete when it next emits". sprefa's tag_write is `put`-shape; tag_read is `take`-shape (with replay).

## rxjs primitive mapping

| rxjs                              | sprefa                                            |
| --------------------------------- | ------------------------------------------------- |
| `BehaviorSubject<T>`              | Pending capture (single-cursor reactive var)      |
| `ReplaySubject<Row>`              | tag bag at relation_store                         |
| `share()`                         | Cursor lineage / forks share Subjects             |
| `switch_map`                      | Default at any process-tree node where input drives "latest reaction" — LSP per-request, watch reruns, parametric rule re-call. Inner subscription = OpCall for that node. |
| `merge_map`                       | Per-cursor fan-out inside an op; set accumulation; broadcast (`;`) where every arm is wanted |
| `concat_map`                      | Reserved for when ordering IS the semantic (rare; usually means a side-effect op, not a graph op) |
| `exhaust_map`                     | First-wins debounce-by-completion (rare in datalog-shaped flow; useful for guarded write ops) |
| `take_until(intent$)`             | op-call lifecycle bound                          |
| `combineLatest`                   | quiet$ derivation; multi-capture binding wait     |
| Observable subscribe              | op-call instantiation                            |
| Subject `next` / `complete`       | Subject `next` only; nothing completes            |

## Four structural divergences from rxjs

1. **Datalog rows (positional schema)** — ProbeOp/JoinOp compile to bag scans, not combineLatest choreography.
2. **Static BindingGraph rejection at lower-time** — kills subscribe-order race classes rxjs leaves to userland.
3. **Cursor lineage IS Subject identity** — forks share Subjects automatically; rxjs needs `share` discipline.
4. **Op-level Park is `Poll::Pending`** — pull-shape backpressure, not a custom primitive. (Initially flagged as a 4th divergence; collapsed into Stream's existing semantics.)

## What const-golf userland unlocks

- Pipeline values are durable artifacts. .sprf compiles to a serialized Pipeline; bd cards can carry Pipeline values; replay = re-deserialize + subscribe.
- Saga purity is structural, not opt-in. There's no place for hidden state to slip in at the userland boundary.
- Pipelines are sendable across threads / processes / machines. Distributed execution falls out for free if the wire format exists.
- Two op-calls from the same Pipeline value = monadic repeatable state. Same code, fresh state per invocation.

## Open design questions

- Pipeline serialization format (deferred until POC validates the value type).
- Quiet$ detector implementation site: RtCtx vs OpCall vs Batcher (probably Batcher exposes pending_count$ Subject, OpCall derives quiet$ via combineLatest).
- Operator overloading discipline: `>>` for seq, `|` for fork — does coherence hold across the Pipeline struct?
- Tag/relation/rule unification: ObservableTap publishes per-effect Subjects; tags use parallel SubjectRegistry. Converge after POC.

## Provenance

Session log: `chat_log/20260427.14` (this session, conversation derived).
Architecture sketch: this file.
POC card: `sprefa-4m7.7.12` (top-down rxRust validity proof, const-golf acceptance).
Related skills: `rx:rxrust-core`, `sagas:sprf-effect-runtime`, `sagas:redux-saga-essence`, `theory:push-pull-dam`, `orchestrator:sprf-rx-runtime`.
