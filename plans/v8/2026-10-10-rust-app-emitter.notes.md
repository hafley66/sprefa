# Rust app emitter: notes

Status: notes from chat, 2026-10-10. Nothing decided, nothing built.

User, 2026-10-10: "i want to make this typespec/programmable datalog lisp in rust with effects
and clock chceker, as the intended plan for making programs with it with the rust app emitter.
basically, take most of the compiler, get rid of the compiler bits, and inline the ivm logic or
the iterator logic + effects are just integrations that can be automated by saying here is
dylib/boundary protocol that can load in comptime and 3rd party 1st run app emitter that is rust
etc. ... i want to make the programs monomorphically to rust but from rust is silly. but the
compiler has a bunch of things i dont care for. i want to make a lot of things with this tool bcx
i like typespec but i want more"

## Decision (user, 2026-10-10)

"we are more postgers effectful server slash one day will be app emitter but tsp type and
jsonchema and opsenapi emitter yes to replace monotone fixpoint of features in tsp"

| now | later |
|---|---|
| effectful server: long-lived dl8, DD comptime as the runtime, one store | Rust app emitter |
| emitters: TypeSpec types, JSON Schema, OpenAPI, covering what TypeSpec's emitters do | |

Consequence for storage: the server owns the store, so `vec` keeps its `vec_item` table shape.

## Shape

```
dl8 lisp source
  -> read + macrotime          (programmable: macros, TypeSpec-like decorators)
  -> check: types + clocks     (labels each relation once / stratum / effect / live)
  -> Rust app emitter          (monomorphic crate, no interpreter inside)
       once, stratum -> plain Iterator code
       live          -> Stream / inlined IVM
       effects       -> calls into plugins loaded at comptime through a boundary protocol
```

## Today's phases (lines from `cat src/_N_*/**.rs | wc -l`)

| phase | lines | does | keep? |
|---|---|---|---|
| `_0_read` | 1193 | parse the lisp | |
| `_1_macrotime` | 3386 | macros as datalog over syntax rows | |
| `_2_lower` | 7007 | forms to rules, declarations, units | |
| `_3_check` | 3161 | names, binding, modes, strata | |
| `_4_comptime` | 6579 | compile-time evaluation, imports, TSI load | |
| `_5_reify` | 4243 | rows, JSON, SQLite, IR emit | |
| `_6_eval` | 12420 | interpreter, DD and IR engines | |
| `_8_driver` | 1202 | project and unit plumbing | |
| `_9_runtime` | 4569 | store, SQLite, executors | |

## Plugin boundary candidates (to research before code)

| option | crates |
|---|---|
| dylib, stable ABI | `abi_stable`, `stabby` |
| WASM component | `wasmtime` + WIT |
| child process protocol | JSON-RPC over stdio (ryi pattern) |

## Open questions

1. Which phases are the "compiler bits" to drop.
2. Emitted app storage: SQLite, or storage as an effect.
3. Same plugin boundary at comptime and in the emitted app, or two.

## Related

- Clock labels: `plans/v8/2026-10-09-clock-checker-strategy.design.md`
- Clock types probe: `plans/v8/probes/clock_types/`
- Atlas: `plans/v8/2026-10-09-clock-types-atlas.html`
