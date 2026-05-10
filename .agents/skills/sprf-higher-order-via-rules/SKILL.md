---
name: sprf-higher-order-via-rules
description: Higher-order in sprefa is rules with value-typed params, no OpRef gymnastics. Two regimes (literal vs row-bound) detected by syntactic check at lower time. Load when designing rule call lowering, parametric rules, or branching/if op.
---

# Higher-order via rules with value params

## The thesis

The only higher-order construct sprefa needs is a rule that takes Value-typed params. No separate OpRef mechanism. Lowering inlines each call site as its own DD subgraph.

## The two regimes

```
rule scan(${PAT?}) > re(${PAT}) > publish_diag(${LINE})
;
scan("TODO")             ←── literal call (regime A, common)
scan("FIXME")            ←── literal call
scan(some_other_term)    ←── row-bound call (regime B, rare)
```

**Regime A — literal call args.** Common case. Lower instantiates the rule body once per call site, substituting the literal into `re()`'s arg. Each call site = own subgraph in DD. Cheap, fast, fully lowered.

**Regime B — row-bound capture arg.** Rare. Body must accept the pattern as a row column. Specialized variant ops needed:

```rust
re_dyn(captures, pat_col)
// compiles the pattern per row, LRU-cached by source string
```

Slower per-row but composable with arbitrary upstreams.

## Static analysis you DO need

- **Detect (A) vs (B) per call site.** Syntactic check, no inference. If the call arg is a literal at parse time, regime A. Otherwise regime B.
- **Reject body ops that can't accept (B).** E.g. `ast()` needs grammar at parse time; refuse with parse-time diag.

## Static analysis you DON'T need

- **Call graph.** Rules can recurse via the event/yield bag, but termination is bounded by the gen budget (one gen per hop in impl A). No recursion analysis required.
- **Type inference.** Lattice types (bytes ⊑ string ⊑ tokens ⊑ tree) are walk-and-check at lower time, not infer.

## DD's contribution

The operator graph **is** the higher-order resolution. Once each call site is lowered, DD just sees a fixed graph. The "graph power" you get is incremental retraction across all of them simultaneously. No separate macro/eval system required.

## Branching falls out

`if(${COND}, ${THEN_PIPE}, ${ELSE_PIPE})` is an op that takes Pipe-valued args. Each block lowers to a synthetic Pipe value. `if` op lowers to filter-then-pipe-then-concat. See `sprf-control-flow-as-ops` for full wiring.

## Pipe values, op refs (optional shape)

If you want first-class Pipe values stored in captures (advanced):

```
Value::Str(Arc<str>)             "foo"
Value::Pat(Arc<CompiledPattern>) re(...) / glob(...) / ast(...)
Value::Pipe(Arc<PipeFn>)         > a > b > c        first-class
Value::OpRef(Arc<OpFn>)          one op uninvoked

PipeFn = dyn Fn(Collection<G,Row>) -> Collection<G,Row> + Send + Sync
```

`apply(${X})` op takes an OpRef-valued capture and runs it on upstream. Lowers to:

```rust
c.flat_map(|row| {
    let op_ref = row[X].as_op_ref();
    op_ref.run_on(single_row_collection(row))
})
```

This is regime B with extra ceremony. Defer until the simpler rules-with-value-params shape proves insufficient.

## Filter-to-zero diagnostic

If a higher-order rule call always emits 0 rows for K gens, surface a hint. Per-op runtime emission counter keyed by call site. Cheap.

## Key sequence at lower time

```
1. parse rule decl → RuleIR { name, params, body: Vec<OpIR> }
2. for each call site:
     a. classify each arg as literal or row-bound
     b. if all literal → instantiate body with substituted literals
        emit a fresh subgraph per call site
     c. if any row-bound → require body ops support runtime variant
        wrap pattern compilation in LRU-cached closure
3. register into collections map under rule name
4. downstream consumers see a Collection just like any other
```

## Sources

- chat_log/20260501.1.dd-effects-control-flow-types.md (higher-order section)
- ref-v0-goals.md (parametric rules + "1 type of value, 1 type of non-value")
