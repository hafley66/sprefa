# Plan: LSP lint — prevent per-row work in sprf Component implementations

## Motivation

This session uncovered two real N+1s that should have been impossible to author:

1. `mounted_query::reconcile_owner_table` looped `SupportLedger::add` per row → per-row `insert_batch(SUPPORT_TABLE, vec![1])` + per-row full-table-scan `triple_mult`.
2. `runtime_graph::resolve_source_root` shelled out `git rev-parse --show-toplevel` and was called from `ast.dispatch`'s per-row pre-par_render loop. 63k fork+exec per bench = 10-min wall.

Both bugs lived inside files declared as effect/dispatch primitives. Both were syntactically obvious in isolation:

- `for ... { self.store.insert_batch(...) }` with single-row vecs.
- `for ... { Command::new("git")... }`.

A static lint over `impl Component for X { fn dispatch(...) | fn render_batch(...) }` would have flagged both at write time.

## Scope

Lint applies to:

- `impl Component<...> for X { fn dispatch(...) }`
- `impl Component<...> for X { fn render_batch(...) }`
- Any free function called from inside those impls (transitive, one level — see Scaling).

Inside those bodies, flag any of:

- `std::process::Command::new(...)` — fork+exec.
- `std::fs::read*`, `std::fs::metadata`, `std::fs::File::open` — sync IO.
- `.lock().unwrap()` on a `Mutex` whose declaration is `&self.X` and `X` is a shared store / cache (heuristic: type is `SqliteFactStore` / `dyn FactStore` / `rusqlite::Connection`).
- `.insert_batch(..., vec![...])` with a vec literal of ONE element.
- `.insert(..., ...)` (single-row insert on a FactStore — should be batched).
- `.read_where(...)` / `.rows_of(...)` (full-table or indexed read on a FactStore).
- `blake3::hash` / `Hasher::finalize` (per-row hashing — usually OK, but flag if the hash result is then used in a sqlite lookup).
- `.coord_of(...)` / `.intern_*(...)` on `SprfStore` (these CAN be amortized via LRU but were per-row IO until today).

That are called inside a `for`/`while`/`loop` block whose preceding line is NOT a justification comment.

## Justification comment schema

Reuse the schema from `lsp-loop-justification-lint.md`:

```rust
// loop-justify: invariant=<text>; side-effect=<text>; throughput=<text>
```

Same single-line, three-key, anchor-regex form. A primitive-flagged call site inside an unjustified loop is an `lsp_error`. Inside a justified loop is silent (caller asserts they thought about it).

## sprf author surface

```sprf
rule(:hot_primitive_callsites, FS?, LO?, HI?, KIND?);
rule(:dispatch_or_batch_impls, FS?, LO?, HI?, FN_KIND?);
rule(:hot_loops_in_impl,       FS?, LO?, HI?);          # for/while/loop inside an impl block
rule(:justified_hot_loops,     FS?, LO?, HI?);          # same set with neighbor comment
rule(:bad_callsite,            FS?, LO?, HI?, KIND?);

# 1. Find every impl Component fn dispatch / fn render_batch in v4/src
rule(:dispatch_or_batch_impls, FS?, LO?, HI?, FN_KIND?) {
    fs > glob`v4/src/**/*.rs` > ast_yaml(:rs)`
        rule:
          all:
            - { inside: { kind: impl_item, has: { kind: trait, regex: "^Component" } } }
            - { any: [ { kind: function_item, regex: "fn (dispatch|render_batch)\\b" } ] }
    `
};

# 2. Hot loops INSIDE those impls
rule(:hot_loops_in_impl, FS?, LO?, HI?) {
    dispatch_or_batch_impls?(FS?, LO?, HI?)
      > ast_yaml(:rs)`
        rule:
          any:
            - { kind: for_expression }
            - { kind: while_expression }
            - { kind: loop_expression }
            # iterator chains too: .map / .for_each / .filter on a slice
            - { pattern: "$$$.for_each($$$)" }
            - { pattern: "$$$.map($$$)" }
            - { pattern: "$$$.filter($$$)" }
    `
};

# 3. Justified hot loops (comment above)
rule(:justified_hot_loops, FS?, LO?, HI?) {
    hot_loops_in_impl?(FS?, LO?, HI?)
      > ast_yaml(:rs)`
        rule:
          follows:
            kind: line_comment
            regex: "^\\s*//\\s*loop-justify:\\s*invariant=[^;]+;\\s*side-effect=[^;]+;\\s*throughput=[^;]+\\s*$"
            stopBy: neighbor
    `
};

# 4. Forbidden call inside any hot loop
rule(:hot_primitive_callsites, FS?, LO?, HI?, KIND?) {
    hot_loops_in_impl?(FS?, LO?, HI?)
      > ast_yaml(:rs)`
        rule:
          any:
            - { pattern: "std::process::Command::new($$$)" }
            - { pattern: "Command::new($$$)" }
            - { pattern: "std::fs::read($$$)" }
            - { pattern: "std::fs::metadata($$$)" }
            - { pattern: "std::fs::File::open($$$)" }
            - { pattern: "$$$.insert_batch($$$, vec![$$$])" }
            - { pattern: "$$$.insert($$$)" }
            - { pattern: "$$$.read_where($$$)" }
            - { pattern: "$$$.rows_of($$$)" }
            - { pattern: "$$$.coord_of($$$)" }
            - { pattern: "$$$.intern_ref($$$)" }
            - { pattern: "$$$.intern_string($$$)" }
            - { pattern: "$$$.intern_file($$$)" }
    `
};

# 5. Bad call = primitive callsite inside an UNJUSTIFIED hot loop
rule(:bad_callsite, FS?, LO?, HI?, KIND?) {
    hot_primitive_callsites?(FS?, LO?, HI?, KIND?)
      > not(justified_hot_loops?(FS, LO, HI))
};

bad_callsite?(FS?, LO?, HI?, KIND?)
  > lsp_error(:sprf-component-n-plus-1)
        `per-row "${KIND}" inside Component::dispatch/render_batch without loop-justify comment at ${FS}:${LO}`;
```

## Scaling

- One ast_yaml pass per rule (4 passes total over `v4/src/**/*.rs`, not the whole repo).
- Antijoin = NOT EXISTS, single SQLite statement.
- No per-row work in the lint itself (rule above is itself a stress-test for N+1 prevention).

## Open questions

1. **Helper functions called by dispatch**: the lint as written only catches calls written LITERALLY inside the dispatch body. If `dispatch` calls `self.write_one(row)` which is a method elsewhere that does `insert(...)`, the lint misses. Two options:
   - (a) Accept the miss; rely on review for helper methods.
   - (b) Inline-trace one level: lift any `fn` called from a flagged dispatch into the scan set. Implementable as a separate `helper_methods_of_dispatch` rule, but doubles complexity.
   Default to (a) for v1.

2. **`.lock().unwrap()` heuristic**: ast-grep can match the AST shape, but distinguishing "lock on the heavy store" vs "lock on a thread-local buffer" needs a type-level check. Without type info, default to flagging ALL `.lock().unwrap()` inside hot loops; users justify when intended (e.g., per-row atomic counter is fine — emit the loop-justify comment).

3. **False positives in `into_pipe`-like setup code**: `Component::new(...)` builders call `intern_*` once per declared column. That's NOT a hot loop. Should be safe because the lint only fires inside `impl Component for X { fn dispatch | fn render_batch }`. Verify with the fixture below.

4. **`par_render` closures**: ast.render_batch already calls `par_render(batch, |c| { ... })`. The body of the closure runs per-row. Should the lint flag calls inside the closure? YES — but the closure is structurally a `move |c| { ... }`, not a `for`. The lint must also flag the bodies of `par_render(..., |c| { ... })`, `.par_iter().for_each(...)`, `.par_iter().map(...)`. Add those to the iterator-adapter list.

5. **Where the lint runs**: `cargo test --test sprf_component_n_plus_1` invoked by CI, OR live in `sprefa-lsp`. Default to the former; live-LSP is nice-to-have.

## Test plan

Fixture: `v4/tests/fixtures/n_plus_1_lint_target.rs`:

```rust
// (a) Justified — no diag
impl Component for Good {
    fn dispatch(&self, ctx: &RenderCtx, rows: &[QueueRow<Cursor>], queue: &dyn QueueBackend<Cursor>) {
        // loop-justify: invariant=rows len bounded by batch_cap; side-effect=batched_insert; throughput=O(n) amortized
        for row in rows {
            self.store.insert(table, row.value.clone());
        }
    }
}

// (b) Unjustified per-row insert — diag
impl Component for Bad1 {
    fn dispatch(&self, ctx: &RenderCtx, rows: &[QueueRow<Cursor>], queue: &dyn QueueBackend<Cursor>) {
        for row in rows {
            self.store.insert(table, row.value.clone()); // <-- flagged
        }
    }
}

// (c) Unjustified fork — diag (the resolve_source_root archetype)
impl Component for Bad2 {
    fn render_batch(&self, _ctx: &RenderCtx, batch: &[&Cursor]) -> Vec<Node<Cursor>> {
        batch.iter().map(|c| {                    // <-- iter adapter, hot
            std::process::Command::new("git")     // <-- flagged
                .arg("-C").arg(c.value.as_ref())
                .output();
            Node::Done
        }).collect()
    }
}

// (d) Unjustified per-row insert_batch with 1-element vec — diag (the SupportLedger archetype)
impl Component for Bad3 {
    fn dispatch(&self, ctx: &RenderCtx, rows: &[QueueRow<Cursor>], queue: &dyn QueueBackend<Cursor>) {
        for row in rows {
            self.store.insert_batch(table, vec![row.value.clone()]); // <-- flagged
        }
    }
}
```

Expected diagnostics: 3 (Bad1, Bad2, Bad3). Good produces zero.

## Critical files

- `v4/examples/dogfood-sprf-component-n-plus-1.sprf` (new — the lint program)
- `v4/tests/fixtures/n_plus_1_lint_target.rs` (new — fixture)
- `v4/tests/lsp_sprf_component_n_plus_1.rs` (new — integration test)
- `v4/src/v2_ops.rs` (reference — `AstYamlComponent`)
- `v4/src/lsp.rs` (reference — `lsp_error` surface)
- `v4/plans/lsp-loop-justification-lint.md` (companion plan; reuses comment schema + antijoin shape)

## TODO checklist

- [ ] Resolve open question 1 (helper-method inlining) and 2 (lock heuristic) with user.
- [ ] Write `dogfood-sprf-component-n-plus-1.sprf`.
- [ ] Write fixture + integration test.
- [ ] Run on `v4/src/` and assert zero diags (today's tree, post N+1 fixes).
- [ ] Add a `// loop-justify` to every legitimately hot loop in `v4/src/` (the rayon par_render closures, etc.) so the gate doesn't false-positive.
- [ ] Wire into CI as a fail-on-diag rule.
