# sprefa v3 Semantic Model

*Avant-garde language theory — the λ × bash × Prolog fusion*

---

## 1. Semantic Model Essay: Cursors as Closures

A **cursor** is a closure over file content. Like a bash process that inherits `PWD`, `PATH`, and file descriptors from its parent, a cursor inherits `content`, `byte_range`, `captures`, and `path` from the pipeline stage that spawned it. The cursor is the *only* first-class value that flows through the runtime. Scalars, rules, ops — everything else is either embedded in the cursor (as captures or slots) or lives in the static environment (as named entities). This is the invariant: **every expression returns cursors**.

**Binding** is attaching a name to a value in a cursor's capture map. When an `ast[rust] { class $NAME }` walker matches, it emits a cursor whose `captures` vector now contains `NAME → byte_range`. That binding is local to the cursor. Two cursors flowing through the same fork arm may have different bindings; the stream is a bag of closures, each carrying its own lexical payload. Binding is not assignment — it is *narrowing*. You don't mutate a cursor; you emit a new cursor with an extended capture map, just as bash `local VAR=val` creates a new shell variable without touching the parent's environment.

**Unbound** is a capture name that exists in the cursor's type but has no value yet. In lambda calculus terms, an unbound capture is a free variable *within the cursor's local scope* — it is waiting for a λ-abstraction (an upstream op) to supply it. The `TermMode` enum makes this explicit: `Bound(Value)` or `Unbound`. At runtime, an op encountering `Unbound` where it needs `Bound` does not throw; it **parks** the cursor (backpressure) until upstream closes the stream, at which point the cursor drops silently. The static checker guarantees that no cursor parks forever.

**Free variable** — in sprefa, this is a name that is not in scope at lower time. But here is the twist: **a free variable is not an error; it is an implicit unbound capture**. If you write `$FOO` and `FOO` has no binding source in the `BindingGraph`, the resolver does not emit a hard error (unless the op's `ArgSpec` demands `BoundOnly`). Instead, it treats `$FOO` as `TermMode::Unbound` with a fresh `SlotKey`. This makes every term reference *lazy by default*. The runtime decides whether the laziness resolves. This is the bash heritage: referencing an unset variable is not a fatal error unless the command you pass it to requires it.

**Cross-references (xref)** — `rule.$V` — are **static dependencies with dynamic materialization**. At lower time, `rule.$V` is a `TermRef` whose `term_path` points into `rule`'s schema. The resolver checks that `V` is a declared capture (or param) of `rule`. This is static. But at runtime, `rule.$V` does not read from `rule`'s SQLite table directly; it subscribes to `rule`'s output stream and performs a **semijoin**. The cursor carrying `rule.$V` is parked until the `rule` stream emits a row with `V` bound, then the value is projected into the waiting cursor. The xref is static in type, dynamic in delivery. Think of it as Prolog's `:-` with one crucial difference: there is no SLD resolution, only **stream subscription**.

**Bidirectionality without SLD** is the radical simplification. In Prolog, `member(X, [a,b])` can produce `X=a`, `X=b`, or verify `X=a`. In sprefa, this is not a unification algorithm; it is **op-local dispatch**. Every op declares an `ArgSpec`: `BoundOnly`, `UnboundOnly`, or `Either`. The runtime computes `TermMode` per arg at the call boundary. The op's `pipe` implementation receives a `Vec<TermMode>` and decides what to do. `tag($X, :kind)` — if `$X` is `Bound`, write a relation row; if `Unbound`, read relation rows and emit cursors with `$X` bound. There is no backtracking because there is no global unification store. Each op is a self-contained black box that knows how to behave in "producer mode" vs "consumer mode." A rule body (a Pipeline) is **not** inherently bidirectional; bidirectionality is a property of the **ops** inside it. However, a parametric rule like `rule(used_by, $CLASS)` can be called with `$CLASS` bound or unbound, and the body ops dispatch accordingly. The rule is a *template* for mode combinations, not a logical relation.

**Carveouts** `${...}` are anonymous pipelines evaluated in a **micro-scope**. When `${TERM > lowercase}` appears inside a sub-grammar body, the framework narrows the cursor to the carveout's byte range and evaluates the pipeline `TERM > lowercase` against that narrowed cursor. `TERM` resolves from the incoming cursor's captures; `lowercase` is an op receiving the stream. The result cursors from the carveout are **re-based** back onto the parent cursor's content (if they changed content) or simply returned as narrowed cursors. The `>` inside a carveout is the ordinary chain operator. `${...}` is not syntactic sugar; it is a **scope boundary** with automatic cursor narrowing. It is the bash `$(...)` of sprefa: a subprocess that inherits the environment, runs its own pipeline, and whose output is captured into the parent context.

**Assertions** (`assert`, `witness`, `check`) are ops that consume cursor streams and emit **violation cursors**. A violation cursor is a cursor like any other — it carries `path`, `content`, `byte_range`, and `captures` — but its `SprfPath` includes a `PathSeg::Op { name: "assert", ... }` and its captures contain the failing values. Assertions write to the `violations` persistence tier. They are first-class ops, not a separate top-level construct. The user wants `assert(eq(dep("service-a", $VERSION), dep("service-b", $VERSION)))` — this is an op call `assert` whose arg is a pipeline whose inner `eq` relation emits one cursor on mismatch and zero on match. The assertion op culls: if input cursors arrive, it emits them as violations. This is RxJS semijoin meets bash `test`.

---

## 2. Binding Calculus

We adapt the typing judgment `Γ ⊢ e : τ` to cursor streams. The judgment reads: *under capture environment Γ, pipeline P evaluates to a stream of cursors whose capture superset is at least Γ'.*

### Environments and Modes

```
Γ  ::= ∅ | Γ, $X:mode
mode ::= Bound | Unbound | Either

resolve(Γ, $X) = mode     (look up term in environment)
Γ ⊎ Γ'  =  merge of capture maps (right-biased on overlap)
```

### Judgments

```
Γ ⊢ scalar     →  Stream<Cursor{·}>          (cursor with no new captures)
Γ ⊢ $X         →  Stream<Cursor{captures[$X]=resolve(Γ,$X)}>
Γ ⊢ op(args)   →  Stream<Cursor{Γ' ⊇ Γ}>    (where Γ' is op's emit schema)
```

### Chain Rule (function composition with capture threading)

```
Γ ⊢ A  →  S₁   with captures S₁ = Γ₁
Γ₁ ⊢ B  →  S₂   with captures S₂ = Γ₂
─────────────────────────────────────
Γ ⊢ A > B  →  S₂   with captures Γ₂
```

Chain is sequential bind: the output stream of `A` becomes the input stream of `B`. `B` sees every capture bound by `A`. This is the Kleisli composition of the cursor monad.

### Fork Rule (merge)

```
Γ ⊢ A  →  S_A   with captures Γ_A
Γ ⊢ B  →  S_B   with captures Γ_B
─────────────────────────────────────────────────
Γ ⊢ { A ; B }  →  merge(S_A, S_B)   with captures Γ_A ∩ Γ_B
```

Fork duplicates the input cursor to both arms (like bash `{ cmd1 & cmd2 & wait }`). The output stream is the disjoint union of both arm outputs, tagged with `PathSeg::ForkArm`. The resulting capture environment is the **intersection** of arm captures: a downstream op can only rely on captures present in *both* arms, because a cursor from arm 0 may lack arm 1's bindings and vice versa. *(Proposed Lock Amendment: current lock says captures survive fork; this rule makes the intersection explicit for static checking.)*

### Op Call with Mode Dispatch

```
signature(op) = [arg₀: AcceptsMode₀, arg₁: AcceptsMode₁, ...]
Γ ⊢ argᵢ  →  modeᵢ   (mode computed from Γ)
modeᵢ ⊑ AcceptsModeᵢ   for all i   (static check)
─────────────────────────────────────────────────
Γ ⊢ op(arg₀, arg₁, ...)  →  op.dispatch(mode₀, mode₁, ...)
```

`⊑` is the mode subtyping relation: `Bound ⊑ Either`, `Unbound ⊑ Either`, `Bound ⊑ Bound`, `Unbound ⊑ Unbound`. No `Bound ⊑ Unbound` or vice versa.

The runtime dispatch rule:

```
op.dispatch(modes, cursor) =
  match (modes, cursor) with
  | all required args Bound(v)      →  EmitBound(cursor')
  | argᵢ Unbound, upstream open     →  Park(cursor)   (backpressure)
  | argᵢ Unbound, upstream closed   →  Drop           (silent)
  | argᵢ Unbound, op is producer    →  IterateRegistry(cursor_stream)
```

### Capture Write Rule

```
Γ ⊢ expr  →  S   with captures Γ'
Γ' ⊢ > $X   →  Stream<Cursor{Γ', $X=val}>
──────────────────────────────────────────
Γ ⊢ expr > $X  →  Stream<Cursor{Γ', $X=val}>
```

`> $X` is a pseudo-op (lowers to `CaptureWriteOp`) that binds the current cursor's active bytes (or value) into `$X`. It is the bash `read VAR` of sprf.

### Carveout Rule (micro-scope)

```
Γ ⊢ outer_expr  →  S_outer
c ∈ S_outer has byte_range R
c.narrow(R) yields c'
Γ ⊢ inner_expr  →  S_inner   evaluated against c'
────────────────────────────────────────────────────
Γ ⊢ outer_expr{ inner_expr }  →  rebase(S_inner, c)
```

The carveout evaluates `inner_expr` on a cursor narrowed to the carveout's source range. Results are re-based onto the parent cursor's content if content changed, or returned as-is.

---

## 3. Concrete Syntax Proposals

### 3.1 `${TERM > lowercase}` — chain inside carveouts

**Resolution:** `>` inside `${...}` is the ordinary chain operator. The carveout body parses as a full `host_expr`, which includes chain expressions. The lowered form is:

```
${TERM > lowercase}  →  CarveoutOp(Chain([TermOp("TERM"), lowercase_op]))
```

The cursor entering the carveout is narrowed to the carveout's byte range. `TERM` resolves against the narrowed cursor's captures. `lowercase` receives the stream and emits transformed cursors. The carveout op re-bases results.

**Nested example:**

```sprf
${TERM > re(r"[a-z]+") > lowercase}
```

This is a three-stage chain inside the carveout. `re(...)` emits cursors for each match; `lowercase` transforms each. The final cursors are the carveout's output. This is bash `${var##pattern}` generalized to a pipeline.

### 3.2 Cross-rule refs: `rule.$V` vs `&.rule.$V` vs `$rule.$V`

**Resolution:** Use `rule.$V` for xref projection. The `&.` prefix is reserved for **cursor rebase** (field access on the *current* cursor), not rule access. The `$` prefix is reserved for **term reference** (capture names), not rule names.

```sprf
# Static dependency + dynamic semijoin
rule(calls) > ast[rust] { new ${classes.$NAME > TARGET}() }
#                              ^^^^^^^  ^^^^^
#                              rule     capture projection
```

Lowered form:

```rust
TermRef {
    term_path: ("classes", "NAME"),   // static: rule + capture
    slot_key:  runtime_key,
    source:    Xref { rule: "classes", capture: "NAME" },
}
```

At runtime, the op containing the carveout subscribes to the `classes` stream and performs a semijoin on `NAME`. The matched value is bound locally as `TARGET`.

**Proposed Lock Amendment:** The teaching doc `v2/docs/_b_v3-unified-language.md` Chapter 17 spells this `${ rule_name.$CAPTURE > $LOCAL_NAME }`. The outer `$` and `{}` are the carveout; inside, `rule_name.$CAPTURE` is the xref expression. This is consistent. Keep it.

### 3.3 Assertions / Witnesses

Three op forms, no new top-level keyword:

```sprf
# check: SQL returns rows → those rows are violations
check(name) { SQL_BODY }

# assert: SQL returns rows → violation (any row = failure)
assert(name) { SQL_BODY }

# witness: SQL returns zero rows → violation (empty = failure)
witness(name) { SQL_BODY }
```

**Inline assertion (the microservice invariant):**

```sprf
rule(invariant_deps)
  > fs(r"**/package.json")
  > json { dependencies { $PKG: $VER } }
  > tag($PKG, :package)

assert(services_synced) {
    SELECT a.PKG, a.VER as ver_a, b.VER as ver_b
    FROM   invariant_deps a
    JOIN   invariant_deps b ON a.PKG = b.PKG
    WHERE  a.VER <> b.VER
}
```

**Pipeline assertion (no SQL, pure cursor logic):**

```sprf
# Assert as a higher-order op taking a pipeline of relation ops
assert_inline(eq(dep("service-a", $VERSION), dep("service-b", $VERSION)))
```

Lowered form: the `eq` relation op emits cursors only when its args disagree (mismatch = violation row) and zero cursors on match. The `assert` op culls its input: if any cursor arrives, it is a violation. This makes assertions **just another stream semijoin**.

### 3.4 Bidirectional rule calls

Parametric rules declare params as unbound terms. Call sites supply bound or unbound args.

```sprf
# Declaration: one param, lazy until called
rule(used_by, $CLASS) > ast[rust] { new $CLASS() }

# Call with bound arg: filter semijoin
rule(calls) > classes > used_by($NAME)
#                               ^^^^^ bound by upstream `classes` capture

# Call with unbound arg: producer mode
rule(all_classes) > used_by($UNBOUND)
#                           ^^^^^^^^ unbound → op iterates all `new X()` sites
```

The `used_by` rule's body contains `ast[rust] { new $CLASS() }`. The `ast` walker requires `$CLASS` to be either `Bound` (filter to that class) or `Unbound` (emit all matches, binding `$CLASS` per cursor). The walker's `ArgSpec` declares `Either` for pattern holes.

**Multiple unbound args:**

```sprf
rule(link, $SRC, $DST) > ast[rust] { $SRC -> $DST }

# Producer: emit all edges
rule(all_edges) > link($UNBOUND_A, $UNBOUND_B)

# Filter one side: emit all destinations from a given source
rule(from_x) > link(:module_x, $UNBOUND_DST)
```

This is **shallow Prolog**: mode dispatch is per-op, not global. The `ast` walker knows how to handle `Bound → Unbound` and `Unbound → Unbound` combinations. No SLD, no backtracking. If you need "find all pairs satisfying both patterns," you use a Fork and a join op, not logical conjunction.

---

## 4. Extension Roadmap

### 4.1 Higher-order rules (rules as arguments)

**Status:** Grammar already supports it. `retry(body, 3)` takes a Pipeline value. Higher-order *rules* (parametric rule values) need one addition: `Rule` as a first-class `EntityRef` that can be passed to ops.

**Semantic machinery:**
- `EntityRef::Rule` already exists. Higher-order ops receive it and call `rule.body.run(ctx, cursors)`.
- The missing piece is **capture mapping at application time**: when passing `used_by` to `retry`, the param `$CLASS` must be bound or left unbound at the call site. This is just `ArgValue::TermRef` propagation.

### 4.2 Recursion (`@recursive`)

**Status:** `@recursive(max_depth=N)` is locked as the opt-in syntax. Cycle detection runs at lower time via Tarjan on the rule-call graph.

**Semantic machinery needed:**
- **Fixed-point iteration.** A recursive rule's output stream feeds back into its input. The runner must maintain a `RecursiveFrame { depth, seen_keys }` to detect divergence.
- **Termination guarantee.** Without SLD, termination is the user's responsibility (via `max_depth`). The runner enforces the depth cap; exceeding it drops cursors with a runtime diagnostic.
- **Memoization.** Recursive rules want `Memo` subscribe policy to avoid recomputing at each depth. The frame cache is keyed by `(rule_path, arg_tuple, depth)`.

### 4.3 Mutation effects

**Status:** `MutationEffect` trait is locked. Four optional slots: `preview`, `reversible`, `reverse`, `source_range`.

**Semantic machinery needed:**
- **Cursor invalidation.** A mutation op (e.g., `sed`) changes file content. The emitted cursors must carry a `ReparseDomain` annotation that triggers invalidation of all downstream cursors sharing that `fs/repo/rev` key.
- **Transaction boundary.** Mutations queue to an mpsc. The runner drains and applies them in batch. If a mutation fails, already-emitted cursors from the same pipeline are stale — this is the **eventual consistency** model. For strong consistency, mutations must run in a separate phase after all reads complete.

### 4.4 Full unification (if ever)

**Proposed Lock Amendment: Reject.** The current design's elegance comes from *not* having a global unification algorithm. Adding full unification (even Paterson-Wegman) would:
1. Introduce a global constraint store, violating "ops own everything."
2. Require backtracking, complicating the streaming runtime.
3. Make mode derivation undecidable in general.

**Alternative:** If the user ever needs "real Prolog," embed a Prolog engine as *one op*. `prolog(datalog_body)` takes bound/unbound args, runs SLD inside the op's black box, and emits cursors. The rest of the language stays shallow. This preserves the invariant: deep logic lives inside ops, not in the framework.

---

## 5. Summary Table: The Six Questions Answered

| Question | Answer |
|---|---|
| `$TERM` vs `${TERM > lowercase}` | `$TERM` is capture ref (resolve from cursor). `${...}` is a carveout: micro-scope pipeline evaluation with automatic cursor narrowing. `>` inside is the normal chain op. |
| `rule.$V` xref | Static dependency at lower time (resolver checks schema). Dynamic semijoin at runtime (subscribe to rule stream, project capture). Not a direct SQLite read. |
| Bidirectionality | **Op-local only.** `ArgSpec + TermMode + OpAction` dispatch. Rule bodies inherit bidirectionality from their constituent ops. No SLD, no global unification. |
| Nested cursor enumeration | Every stage emits cursors; next stage iterates. Fork `{A; B}` merges streams. Scalars promote to cursors via implicit `literal_op` (deferred/retrofit). |
| Assertions | **Ops, not keywords.** `assert`, `witness`, `check` are ops that emit violation cursors. Violations are cursors with special path tagging, persisted to `violations` tier. |
| Regex/glob/ast-grep/json | **Ops with string args or sub-grammar slots.** `re("pattern")`, `fs(glob)`, `ast[lang]{...}`, `json{...}`. Sub-grammar injection is host-parser managed. No native syntax beyond op calls. |

---

*End of semantic model. Lockfile amendments flagged inline.*
