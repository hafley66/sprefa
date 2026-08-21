# Rust CallF: inline-mod defs and method owners

Branch `fix/rust-callf-def-scope-owner`. Both defects fixed, whole battery green,
zero golden rows moved.

## Contents

1. [What changed](#what-changed)
2. [Defect 1: inline mod bodies](#defect-1-inline-mod-bodies)
3. [Defect 2: a Method def names its owner](#defect-2-a-method-def-names-its-owner)
4. [Before / after counts](#before--after-counts)
5. [Validation output](#validation-output)
6. [Left alone, with the reason](#left-alone-with-the-reason)

## What changed

| commit | subject |
|---|---|
| `804d8f108` | red tests + the `CallFAux.method_owners` row shape, wire arm, schema line |
| `54b40ef47` | every rust projector descends inline `mod` bodies |
| `5386f5b07` | a rust `Method` def names its owner |
| `41d286d04` | drop unrelated `cargo fmt` churn in `lang/data/_0_source.rs` |

Files touched, all under `v6/sprefa-extract/`:

```
src/lang/rust.rs                  the five projector descents + the owner emission
src/types.rs                      MethodOwner, CallFAux.method_owners, FlatFact::MethodOwnerOut
src/wire.rs                       flatten_call emits the method_owner rows
src/schema.rs                     record=method_owner + the self_type/trait field glossary
src/family.rs, src/lib.rs         re-export MethodOwner
tests/30_rust_mod_scope_owner.rs  four tests, fail-first receipts in the header
tests/fixtures/rust_scopes/nested_mods.rs
tests/fixtures/rust_scopes/impl_owners.rs
```

## Defect 1: inline mod bodies

Five call sites had the same `_ => {}` hole. All five now recurse.

| projector | function | shape |
|---|---|---|
| CallF defs | `call_defs_in_items` (new, `rust.rs:1038`) | recursive over `&[syn::Item]`, `Item::Mod(m) => m.content` |
| TypeF entities | `item_entity` | new `Item::Mod` arm re-entering itself per inner item |
| TypeF docs | `doc_facts_in_items` (new) | `doc_facts` is now a one-line driver |
| TypeF consts | `const_values_in_items` (new) | `const_values` is now a one-line driver |
| TypeF edge candidates | `item_edge_candidates` | new `Item::Mod` arm |
| DfF | `df_items` (new) | carries `mod_path`, the enclosing inline-`mod` chain |

`df_items` threads the module path rather than flattening it away: the df `fn_sym`
is text (`{file}::function::{mod_path}{name}`, `{file}::method::{mod_path}{Owner}.{name}`),
so two sibling `mod`s each declaring `fn setup` would otherwise mint one sym for
two callables. `mod_path` is `""` at the file root and `inner::deeper::` two mods
down.

This is a deliberate divergence from the captured v5 oracle. v5 has the identical
blind spot (`src/graph/typegraph/rust/mod.rs` walks `parsed.items` with no
`Item::Mod` arm), so the oracle can never assert these rows. Named in the commit
message.

## Defect 2: a Method def names its owner

`Node<F>` at `types.rs:1164` is span/kind/name and is generic over every family,
so the owner rides `CallFAux` as a span-keyed aux row, the `CallSite` precedent.

```rust
pub struct MethodOwner {
    pub span: Span,                    // joins to the def node by span
    pub self_type: Option<NameId>,     // None for a trait declaration's own items
    pub trait_name: Option<NameId>,    // None for an inherent impl
}
```

Two seats, never one column. `impl Draw for Alpha` and `impl Erase for Alpha`
agree on `self_type` and differ only in `trait_name`; collapsing them would lose
exactly the fact that separates them. `i.trait_` was read by neither v5 nor v6
before this.

Wire and schema:

```
record=method_owner  family=call         owner={start,end}  self_type=<string|null>  trait=<string|null>
```

`self_type` reads through `primary_type(&i.self_ty)`, the same helper `project_df`
calls at `rust.rs:1434`. `trait` reads `i.trait_` for an impl and `t.ident` for a
trait declaration.

No `Edge<CallF>` and no `Node` field: an owner is not a resolved relationship
between two nodes in one file (the trait or self type may not be declared here at
all), so the edge plane was the wrong seat.

## Before / after counts

Probe, `/tmp/probe.rs`, `extract --family call`:

| row | before | after |
|---|---|---|
| def nodes | 5 | 7 (`nested_fn`, `deep_fn` appear) |
| sites | 2 | 2 |
| `method_owner` rows | 0 | 3 |

After:

```json
{"record":"node","family":"call","span":{"start":3,"end":30},"kind":"function","name":"top_level"}
{"record":"node","family":"call","span":{"start":50,"end":85},"kind":"function","name":"nested_fn"}
{"record":"node","family":"call","span":{"start":110,"end":122},"kind":"function","name":"deep_fn"}
{"record":"node","family":"call","span":{"start":149,"end":167},"kind":"method","name":"method_a"}
{"record":"node","family":"call","span":{"start":183,"end":198},"kind":"method","name":"t_method"}
{"record":"node","family":"call","span":{"start":220,"end":238},"kind":"method","name":"t_method"}
{"record":"node","family":"call","span":{"start":244,"end":257},"kind":"function","name":"helper_a"}
{"record":"site","family":"call","span":{"start":17,"end":25},"callee":"helper_a","callee_path":null}
{"record":"site","family":"call","span":{"start":64,"end":80},"callee":"top_level","callee_path":"super::top_level"}
{"record":"method_owner","family":"call","owner":{"start":149,"end":167},"self_type":"S","trait":null}
{"record":"method_owner","family":"call","owner":{"start":183,"end":198},"self_type":null,"trait":"T"}
{"record":"method_owner","family":"call","owner":{"start":220,"end":238},"self_type":"S","trait":"T"}
```

Real corpus, `~/projects/hafley-rs/crates/boop-store/src/ident.rs`. Both binaries
built `--release --features cli`; the before column is the same binary rebuilt
from `a5929de0a`.

| record | before | after | delta |
|---|---|---|---|
| call def nodes | 161 | 253 | +92 |
| call sites | 1685 | 1685 | 0 |
| `method_owner` rows | 0 | 77 | +77 |
| type entity nodes | 103 | 161 | +58 |
| df nodes | 3183 | 5572 | +2389 |

The +58 type entities matches the brief's measured "58 of 137 `fn` in ident.rs sit
inside an inline mod". The 1685 sites are unchanged, which is the point: the site
half never had the hole.

## Validation output

`cargo build --release --features cli` (the `--features cli` is the crate's own
gate, `AGENTS.md:65`; a bare `cargo build --release` builds no `extract` binary
because the bin carries `required-features = ["cli"]`):

```
warning: `sprefa-extract` (bin "extract") generated 1 warning
    Finished `release` profile [optimized] target(s) in 6.27s
```

`cargo test --features cli`, aggregated over every test binary:

```
total passed: 172  failed: 0
```

The golden battery, run on its own:

```
     Running tests/golden_parity.rs
running 9 tests
test rust_doc_parity ... ok
test type_edge_resolve_parity_go ... ok
test type_edge_resolve_parity_rust ... ok
test type_edge_resolve_parity_ts ... ok
test ported_facets_match_v5 ... ok
test deferred_and_v6_only_ledger ... ok
test call_resolve_scip_ratchet_ts ... ok
test call_resolve_scip_ratchet_rust ... ok
test call_resolve_scip_ratchet_go ... ok
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.76s
```

ZERO golden rows moved. `tests/24_rust_specifiers.rs` is green, so `mod tau { .. }`
still emits no specifier row while the `use` inside it still does.

The new tests:

```
running 4 tests
test rust_method_defs_carry_their_owner ... ok
test rust_df_reaches_inline_mod_bodies ... ok
test rust_call_defs_reach_inline_mod_bodies ... ok
test rust_type_entities_reach_inline_mod_bodies ... ok
test result: ok. 4 passed; 0 failed
```

`extract --schema | grep -i -A5 call`, the new line in place:

```
  record=site   family=call                span={start,end}   callee=<name>  callee_path=<string|null>
  record=method_owner  family=call         owner={start,end}  self_type=<string|null>  trait=<string|null>
  record=reference  family=call            span={start,end}   functor=<name/arity>  position=<goal|head_arg|term_arg>
```

Plus the FIELDS glossary gained:

```
  self_type    the impl self type a method def belongs to (null for a trait's
               own items).
  trait        the trait a method def implements or is declared in (null for an
               inherent impl).
```

No test pins the schema text as a whole; `tests/20_unresolved.rs:128` greps it for
one specific line and `tests/4_capability_parity.rs:320` only runs `--schema`.
Both green.

## Left alone, with the reason

| thing | state | reason |
|---|---|---|
| `cargo clippy --all-targets -- -D warnings` | RED at base and at HEAD | `tree-sitter-dl6/build.rs:14` `.include(&src_dir)` trips `needless_borrows_for_generic_args`. Last touched `c24a1962e` (2026-08-13), untouched by this branch. Unrelated to either defect; not fixed to keep the diff on task. `cargo clippy --all-targets --features cli` without `-D warnings` reports only pre-existing `field_reassign_with_default` warnings in `tests/13_flow_join.rs`, also untouched. |
| `cargo fmt --check` | dirty at base and at HEAD, in `src/lang/data/_0_source.rs` only | `cargo fmt` reformatted that file as collateral; commit `41d286d04` reverts it to the base bytes rather than shipping unrelated churn. Every file this branch does touch is fmt-clean. |
| consts inside `impl` and `fn` bodies | still skipped | The const pass now descends inline `mod`, which is the defect named in the brief. Descending into `impl` and fn bodies is a separate scope change with its own oracle question and was not asked for. The doc comment on `const_values` states the remaining non-goals. |
| `Item::ForeignMod`, `Item::Macro` | still `_ => {}` | An `extern "C" { }` block declares no callable body and a macro invocation needs expansion, which no projector here does. Neither is an inline `mod`. |
| `sites` for nested mods | already correct pre-fix | `visit_file` walked them all along; that half was never broken. |
