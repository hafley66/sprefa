# Rust module rename: rewrite matrix

What happens when a `.rs` file moves (e.g. `types.rs` -> `_0_types.rs`).

## Reference forms

| Syntax form | Example | Extracted? | Rewritten? | Notes |
|---|---|---|---|---|
| `use crate::mod::Item;` | `use crate::types::Foo;` | RS_USE | syn | simple use |
| `use crate::{mod::Item, ..};` | `use crate::{types::Foo, ast};` | RS_USE | syn | grouped import, the hard case |
| `use crate::mod::*;` | `use crate::types::*;` | RS_USE | syn | glob re-export |
| `pub use mod::*;` (relative) | `pub use types::*;` | RS_USE | syn | parent file only, bare prefix |
| `mod name;` | `mod types;` | RS_MOD | syn | declaration in parent |
| `use ext_crate::mod::Item;` | `use sprefa_rules::types::Foo;` | RS_USE | syn | cross-crate |
| `use super::mod::Item;` | `use super::types::Foo;` | RS_USE (stripped) | syn | super prefix stripped in DB value |
| `use self::mod::Item;` | `use self::types::Foo;` | RS_USE (stripped) | syn | self prefix stripped in DB value |
| `use mod as alias;` | `use crate::types as t;` | RS_USE | syn | rename |
| `#[path = ".."] mod x;` | `#[path = "0_types.rs"] mod x;` | RS_MOD (node_path) | no | by design, user preference |
| inline qualified path | `fn f(r: &crate::types::Rule)` | no (DB) | syn visitor | fn sigs, struct fields, impl, where, generics |

## Architecture

```
DB (scan/extract)          Planner                    Rewriter
     |                        |                          |
  RS_USE refs  ------>  which files?  ------>  sprefa_rs::rewrite_module_refs()
  RS_MOD refs  ------>  which parent? ------>    parses with syn
                                                 walks UseTree + ItemMod
                                                 finds ident by name + prefix
                                                 replaces exact byte span
                                                 writes file back
```

DB narrows the search space (which files to touch). syn does the actual surgery.

Span-based `Edit`s are still used for JS/TS `ImportPath` rewrites.
Rust uses `RustRewrite` structs that carry `(old_stem, new_stem, use_prefixes, rewrite_mod_decl)` per file.

## The grouped import problem (solved)

`use crate::{types::Foo, ast};` -- the old span-based approach stored overlapping spans for all items in the group (all shared `span_start` at `crate`). Replacing one span destroyed the others.

Fix: syn-based rewriter walks the `UseTree::Group`, finds the specific ident node for `types`, gets its tight byte span, replaces only that ident. Each item in the group has its own non-overlapping ident span.

## Inline qualified paths (closed)

```rust
fn compile_rule(r: &crate::types::Rule) -> Result<CompiledRule> {
```

Type paths in fn sigs, struct fields, impl blocks, where clauses, generics, etc. are not extracted as RS_USE refs in the DB. The DB does not see them.

Handled by `syn::visit::Visit` in the rewriter. A `PathVisitor` walks the entire parsed AST, visits every `syn::Path` node, checks if a segment matches `old_stem` after the expected prefix, and records the ident's byte span. Skips `ItemUse` (already handled by the use-tree walker). Covers everything syn can parse -- no string guessing.

## mod_parent_candidates fix

`mod_parent_candidates(old_path, repo_root)` now takes the repo root and produces repo-relative candidate paths (e.g. `crates/rules/src/lib.rs`) instead of `src/`-relative paths (`src/lib.rs`). Required for workspace crates where the DB stores paths like `crates/rules/src/lib.rs`.

## Rust identifier constraint

File names starting with digits produce invalid Rust identifiers: `0_types.rs` -> `mod 0_types;` (parse error). Use underscore prefix: `_0_types.rs` -> `mod _0_types;`.
