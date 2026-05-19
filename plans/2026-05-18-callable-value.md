# Callable as a Value — application IS a Pipe

Status: PATCHED 2026-05-19 after a code-grounded hole-poke. Original
§2/§4 pseudo was wrong against real signatures; corrections below in
`## Corrected (2026-05-19)`. RED tests in
`v4/tests/callable_value_target.rs` are the executable spec. Build in
worktree `feat/callable-value` (base main 9bd1fad6).

## Premise

`Value` (`v4/src/compile/lower/value.rs:35`) is `ValueKind { Atom |
Pipe }`. It can name a callable's *output* (`Pipe`) but not the
callable itself. To pass rules/ops as values, and to let a rule-type
reference another rule-type, the enum must widen.

Key collapse (user, 2026-05-18): **application = lowering the
callable+args into one Pipe step.** Output side is already `Pipe`.
No new output concept. `Pipe` = an applied callable; the addition is
the *un-applied* handle. Three variants total; the third is inert
until applied. Callable = symbol, Pipe = eval result, apply = eval —
the runtime already evaluates (pipe engine); the missing piece is the
unevaluated symbol.

No curry: arity-exact, all args at once → zero partial-application
state. This directly removes the grammar-slotting pain (carrying
partial bindings) the user flagged.

## 1. Type signatures

```rust
enum ValueKind {
    Atom(Arc<str>),
    Pipe(Pipe<Cursor>),         // applied callable. unchanged.
    Callable(CallableRef),      // un-applied. only addition.
}
struct CallableRef { kind: CallableKind, name: Arc<str> }  // ref by name only
enum CallableKind { Op, Rule }
// NO Arc<dyn OperatorDef>, NO embedded rule body.
```

## 2. Body (pseudo)

```
fn apply(ctx, c: &CallableRef, args: &[Value]) -> Result<Value, LowerError> {
    let comp = match c.kind {
        Op   => ctx.registry.lower(&c.name, args)?,   // EXISTING op path
        Rule => ctx.rule_component(&c.name, args)?,    // EXISTING project/read path
    };
    Ok(Value::pipe(Pipe::new().step(comp)))            // output = existing Pipe
}
// rule->rule: Callable(Rule "Fn").body
//   → declared_cols("Fn") has "body", col_type("Fn","body")="Block"
//   → Callable(Rule "Block")  built LAZILY on the dot (cycles safe, lazy per hop)
```

## 3. Instance lifetimes

| Type | Lifetime |
|---|---|
| `CallableRef` | Arc<str>, outlives any ctx |
| Op resolution | only within a `LowerCtx` owning that `Registry` (resolve-on-use; error if applied outside) |
| Rule resolution | within ctx whose `FactStore` declared `name` |
| value identity | `(kind,name)` — name-based eq/hash, feeds existing blake3 `_id` |

## 4. Storage / reads / writes / uniqueness

- Storage: +1 enum variant. `DotTable.ty` stays `Arc<str>` — do NOT
  widen ty→Value here (separate one-model move, not required).
- Read: `resolve_dot` step 2 unchanged (name-keyed, already covers
  Callable(Rule) since it resolves by `.ty` name). New read = `apply`.
- Write (Callable as a column cell): intern `name` + a kind sigil into
  `sprf_strings`; never a closure. Read back = re-resolve by name.
- Uniqueness: `_id = blake3(table || (kind,name) || …)`, consistent
  with current scheme (`fact_store.rs:16`).

## Worms — defused vs still-open

| Worm | State |
|---|---|
| rule→rule cycles | defused: lazy per-hop resolve (existing resolve_dot trick) |
| separate apply return type | defused: output reuses `Pipe` |
| partial application / arity threading | defused: no curry, arity-exact |
| op needs Registry, value is 'static | defused: resolve-on-use; error if applied outside owning ctx |
| Callable in a FactStore cell | defused: store name+sigil, re-resolve by name |
| recursive rule + merged recursion engine (main 37bb93a5) | designed (H7): apply(Rule) = the EXISTING `RuleInvokeComponent`, no new owner-subscription ⇒ inherits the self/SCC wake-guard. RED-pinned. |

## Relation to other tracks

- Independent of the scrapped ts-import bulk grammar dump.
- Independent of cross-file entity graph (that is name resolution, not
  types/callables). See [[project_cross_file_entity_graph]].
- The one-model/comptime move (`DotTable.ty: Arc<str>` → `Value`) is a
  *later, separate* widening. This plan deliberately keeps `ty` a name.
- Additive: 3rd variant, reuses existing op-lowering + resolve_dot.
  No redesign, no undo.

## Corrected (2026-05-19) — holes found vs real code

Poked against `value.rs`, `ctx.rs:272`, `registry.rs`, `rule.rs`,
`op_def.rs`. Five corrections; the original §2/§4 do not compile.

### H1 — `ctx.registry` does not exist (was FATAL)

`LowerCtx` has no registry field/back-pointer (grep = 0). Direction is
`Registry::lower_call_at(ctx, …)`: registry takes ctx, never reverse.
Fix: `apply` takes `&Registry` as an explicit param. Caller is the
walker, which already holds both `&LowerCtx` and the `Registry`.

```rust
// free fn (avoids a self-borrow tangle); lives in ctx.rs or value.rs
pub fn apply(
    ctx: &LowerCtx,
    reg: &Registry,
    c: &CallableRef,
    args: Vec<Value>,
) -> Result<Value, LowerError> { … }
```

### H2 — real `Registry::lower` signature

```rust
Registry::lower(
  ctx: &LowerCtx, name: &str,
  flow: Option<(Value, ByteRange)>,
  args: Vec<(Value, ByteRange)>,
  block: Option<(Pipe<Cursor>, ByteRange)>,
  dsl:  Option<(DslBody, ByteRange)>,
  call_span: ByteRange,
) -> Result<Pipe<Cursor>, Vec<Diag>>
```

So the Op arm is:

```rust
CallableKind::Op => {
    let span = ctx.current_call_span.get().unwrap_or(ByteRange{lo:0,hi:0});
    let args = args.into_iter().map(|v| (v, span)).collect();
    reg.lower(ctx, &c.name, None, args, None, None, span)
       .map_err(LowerError::Validate)?      // Vec<Diag> -> LowerError
}
```

### H3 — `ctx.rule_component` is fictional; real Rule→Pipe path

No such fn. The invoke path is `RuleInvokeComponent::new(rule,
assignments, force)` (rule.rs:164) wrapped in a Pipe, or
`Rule::into_pipe()` (rule.rs:98). For apply = "run callable with args":

```rust
CallableKind::Rule => {
    let rule = ctx.get_rule(&c.name)
        .ok_or_else(|| LowerError::Unknown(format!("no rule `{}`", c.name)))?;
    // map positional args -> RuleInvokeAssign over rule.sink_cols
    let assigns = rule.sink_cols.iter().zip(args).map(|(col,v)| {
        RuleInvokeAssign { col: col.clone(), value: rule_invoke_value_of(&v) }
    }).collect();
    let comp = RuleInvokeComponent::new(rule, assigns, /*force*/ false);
    Pipe::new().step(Arc::new(comp))
}
```
(`rule_invoke_value_of`: Atom→`RuleInvokeValue::Literal`, Pipe→
`RuleInvokeValue::Value`, Callable→error-for-now or Literal of a sigil.)
Output is `Value::pipe(that)` — §"Premise" collapse holds.

### H4 — `resolve_dot` does NOT already cover `Callable(Rule)` (was FATAL)

`resolve_dot` (ctx.rs:272) only reads `v.dots.ty` / `v.dots.map`; it
never inspects `v.kind`. A `Callable(Rule "Fn")` has `dots.ty == None`,
so step-2 projection never fires. §4's "already covers … verify, do not
duplicate" is false. Fix = add an arm BEFORE step 3:

```rust
// after step 1 (instance dot), before/with step 2:
let ty_name: Option<Arc<str>> = v.dots.ty.clone().or_else(|| {
    match v.kind() {
        ValueKind::Callable(CallableRef{kind:CallableKind::Rule, name})
            => Some(name.clone()),
        _ => None,
    }
});
// then run the existing declared_cols(ty_name) projection on ty_name
```
Do NOT auto-`.typed()` at construction (Op callables must not get a ty).

### H5 — callable-as-arg is rejected by `validate_call` unless ArgKind grows

`ArgKind = {Atom, Pipe, Any, Variadic}`. `matches` for Atom/Pipe is
`matches!(v.kind(), ValueKind::X)`; a `Callable` matches neither. 29 op
slots use `Pipe`/`Atom`. Headline test "callable passed as arg" fails
`validate_call` (`lower/wrong-arg-kind`) unless the slot is `Any`. Fix:

```rust
enum ArgKind { Atom, Pipe, Callable, Any, Variadic(&'static ArgKind) }
// matches: ArgKind::Callable => matches!(v.kind(), ValueKind::Callable(_))
// label:   "callable"
```
`ArgKind::Any => true` already accepts a Callable — keep it.

### H6 — fanout (mechanical, compiler-enforced)

`+1 ValueKind` variant is not free: 78 `ValueKind::` sites, and every
exhaustive `match v.kind()/self.kind` arm. `kind_str` gets a
`"callable"` arm; eq/hash by `(kind,name)`. The compiler lists them all
— do them, don't hand-wave.

### H7 — recursion double-fire (the only real RISK, was undesigned)

Recursion + owner-subscribe is merged (main 37bb93a5; memory
`project_recursion_surface_gaps` notes a self/SCC wake-guard). A
self-referential `Callable(Rule)` that is applied must route through the
SAME `RuleInvokeComponent` path normal rule self-calls use, so it
inherits the existing wake-guard — it must NOT open a second
owner-subscription. Design decision: apply(Rule) = exactly the existing
invoke component, no new subscription. Pinned by a RED test asserting a
self-applying rule reaches a finite fixpoint and is run-2 idempotent
(no double rows, no hang).

### H8/H9 — scope truth (do not oversell)

This plan is INTERNAL PLUMBING. It does NOT, by itself:
- land `rule(:P, x?: t.i64)` — CORRECTION 2026-05-19 (cons-plan
  feedback): `x?: t.i64` does NOT lex as a kwarg today. It lexes as
  NEITHER kwarg nor typed col: `split_keyword_arg` takes the first
  top-level `:`, sets `key="x?"`, and `is_ident("x?")` is false (the
  `?` is not an ident char), so it returns `None` and the whole
  `x?: t.i64` falls through to `classify_slot` as ONE positional slot
  — the `?` decl-mark is silently swallowed (walk.rs:557). The fix is
  a surface/classifier change so the decl-mark survives lexing, not a
  pure grammar add; callable-Value still touches zero grammar.
- make `t.i64` a value — `DotTable.ty` stays `Arc<str>` here by
  deliberate choice; `ty→Value` is a later separate widening.
- add any sprf surface to *write* or *apply* a callable — tests are
  Rust-API level (`Value::callable`, `apply(...)`, `resolve_dot`).
It IS the necessary precondition for all three. Sequencing rationale
corrected: callable-Value first because the grammar + `ty→Value` work
both need a callable to exist as a value to point at; it is not the
thing the user sees, it is what makes the visible thing possible.

## Implementation sketch (when unparked)

1. Add `Callable(CallableRef)` + `CallableKind`; `Value::callable(kind,name)`.
2. `kind_str` arm; eq/hash by `(kind,name)`.
3. `LowerCtx::apply(c, args)` reusing `Registry::lower` (Op) and the
   rule projection path (Rule). Arity-exact, no curry.
4. `resolve_dot`: a `Callable(Rule n)` dot resolves against
   `declared_cols(n)` exactly as a `.ty=n` value does (likely already
   works via the name; verify, do not duplicate).
5. FactStore cell round-trip: name+sigil intern/resolve.
6. Tests: callable passed as arg; rule→rule dot returns un-applied
   handle; apply→Pipe runs; self-referential rule does not double-fire
   recursion engine (the one OPEN worm).
