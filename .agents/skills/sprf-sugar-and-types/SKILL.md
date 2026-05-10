---
name: sprf-sugar-and-types
description: [v4 planning] Sugar reform for sprefa — TERM as op call, removing ${...} from statement level, str/re/glob as value-emitting ops. Plus light lattice types (bytes ⊑ string ⊑ tokens ⊑ tree). Load when designing the parser, lower, or any new op kind.
---

# Sugar reform + light lattice types

## Sugar table

The only token-level holes left in raw sprf are `${...}`, `:atom`, and `:atom?`. Everything else is op calls.

| Surface form    | Lowers to              | Notes |
|-----------------|------------------------|-------|
| `TERM`          | `term(:TERM)`          | read capture, 1-row pipe over cursor.captures |
| `TERM?`         | `term?(:TERM)`         | bind capture (introducer) |
| `"string"`      | `str("string")`        | value-emitting op |
| `/regex/`       | `re("regex")`          | value-emitting op |
| `g'glob'`       | `glob("glob")`         | value-emitting op |
| `ast'pattern'`  | `ast("pattern")`       | value-emitting op |
| `sh'cmd'`       | `sh("cmd")`            | value-emitting op (shell DSL) |

## `${...}` carveouts

`${...}` only lives inside DSL string carveouts. The carveout is a syntactic mode change: inside the DSL string, the parser is in regex/sh/jsonpath mode, not sprf mode. `${X}` is the only escape back to sprf names.

```
re("user_${ID}_log")            → interpolation in the regex DSL
sh"git log ${REV}"              → interpolation in the shell DSL
```

At sprf-statement level, bare TERM is fine because:
- UPPER_SNAKE_CASE is the convention.
- Registry-first lookup at parse time disambiguates op-calls (`repo`, `fs`, `re`, `str`) from term references (`UPPER`, `MY_VAR`).
- `:atom` always identifies a registry tag, not a TERM.

The only ambiguity hazard is `re` (builtin op) vs `RE` (user TERM). Resolved by case convention + registry priority.

## str() — value, pipe, or DSL template

```
str("foo")                              just a Str value, 1-row pipe
str("user_${ID}_log")                   templated, ID resolved at run
str("query: ${PATTERN | re}")           pipe-stitched, embedded re()
```

Lower str:

- Parse the literal at parse time.
- Carveout segments: `Lit | Hole | Lit | Hole`.
- Holes are either:
  - `${IDENT}` → `term(:IDENT)` value
  - `${EXPR | OP}` → run EXPR's pipe, fold result through OP

Run-time closure:

```rust
move |row| {
    let s = String::new();
    for seg in segments {
        match seg {
            Lit(t) => s.push_str(t),
            Hole(IdentResolver) => s.push_str(row[ident]),
            Hole(PipeResolver { op }) => s.push_str(eval_op(op, row)),
        }
    }
    emit Value::Str(Arc::from(s))
}
```

## Light lattice types

```
bytes  ⊑  string  ⊑  tokens  ⊑  tree(grammar)  ⊑  graph
```

Each op declares its required input level:

| Op       | Requires           | Produces |
|----------|--------------------|----------|
| `re()`   | string             | string + captures |
| `glob()` | string (path)      | string |
| `ast()`  | tree(grammar)      | tree + captures |
| `line()` | string             | string |
| `fs()`   | —                  | string (paths) |
| `repo()` | —                  | tree(filesystem) |
| `rev()`  | —                  | tree(git) |
| `read()` | bytes              | string OR tree(grammar) via parser registry |

## Enforcement

At lower time, walk the pipe. Each op asserts its input level is ≥ its required. Fails with parse-time diag, no runtime cost.

```
fs() > ast(...)          → parse-time diag: ast needs tree(grammar),
                            fs produces string. need read() in between.
fs() > read('ts') > ast(...)  → ok. read produces tree(grammar).
```

This is types in the cheapest possible form. No inference, no unification. Just "this op needs at-least-X, upstream gives Y, here is the bridge op you forgot."

## re/glob/ast are NOT config values

They are capture-producing pipe ops. Always pipe-position. They consume `cursor.content` and emit 0..N cursors with new captures bound. Zero-emission is normal for negation use cases.

No position polymorphism. One shape per name. `tag` vs `tag?`. `rule` vs `rule?`. If a passive "no-capture, just match" form of `re` is ever needed, that's `re?`.

## Filter-to-zero diagnostic

Per-op runtime emission counter. If an op emits 0 rows for K consecutive gens AND its rule isn't tagged negation, surface a hint. This is a side-channel diag, not a parse-time analysis.

## One-page summary

- Bare TERM at sprf statement level. `${...}` only inside DSL string carveouts.
- `"..." /.../ g'...' ast'...'` desugar to value-emitting ops.
- str() supports template holes resolved at run time.
- Lattice types (bytes ⊑ string ⊑ tokens ⊑ tree(grammar) ⊑ graph) are at-least-X checks at lower time. Cheapest possible.
- re/glob/ast are pipe-position capture-producing ops, never config values.
- One name per shape. No position polymorphism.

## Sources

- chat_log/20260501.1.dd-effects-control-flow-types.md (sugar reform + lattice types)
- ref-v0-goals.md item 5 (Language liftoff: sugar theory, ALL_CAPS = term(:ALL_CAPS))
