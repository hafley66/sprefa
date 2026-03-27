# Remaining gaps in the live rewrite circuit

Tracking what breaks the "move anything without thinking" promise.
Each gap is tagged with whether static analysis (oxc, syn) can fix it
or whether it requires runtime/type information.

## Fixable from static analysis

### ~~1. Re-export chains (JS/TS)~~ FIXED

Fixed by emitting ImportName refs for the source-side name in indirect
re-exports (`export { Foo } from './utils'` now produces both ExportName
and ImportName "Foo"). plan_decl_rename follows re-export chains
transitively via rename_through_reexports, with cycle detection. Aliased
re-exports (`export { Foo as Bar }`) correctly stop propagation at the
alias boundary.

### ~~2. Rust glob imports and renamed symbols~~ FIXED

Fixed by expanding glob imports on rename. When `Foo` is renamed in
`utils.rs` and `consumer.rs` has `use crate::utils::*`, the planner
queries all RsDeclare names in the target file, applies the rename,
and rewrites the glob to explicit imports:
`use crate::utils::{Bar, Other}`. filter_rs_glob_uses resolves
super::/self:: glob paths to absolute form before matching.

### ~~3. Cross-crate workspace imports (Rust)~~ FIXED

Fixed by building a WorkspaceMap from Cargo.toml workspace members.
Maps crate names (with hyphen-to-underscore normalization) to their
src/ directories. plan_file_move and plan_decl_rename use the map to
find cross-crate `use other_crate::module::Item` refs and rewrite
them alongside same-crate refs.

### ~~4. Consume #[path] attribute in module resolution (Rust)~~ FIXED

Fixed by building a ModOverrides map from RsMod refs with non-null
node_path. file_to_mod_path_checked checks the override map before
falling back to filesystem convention. Threaded through plan_file_move
and plan_decl_rename so that `#[path = "weird.rs"] mod foo;` correctly
resolves src/weird.rs to crate::foo instead of crate::weird.

### 5. TypeScript path aliases without tsconfig resolution

`import { x } from '@lib/utils'` -- oxc_resolver handles this when
tsconfig.json is present. But if the alias is defined in a bundler
config (webpack resolve.alias, vite resolve.alias) instead of
tsconfig, oxc_resolver can't resolve it.

**Fix**: parse common bundler configs (vite.config.ts, webpack.config.js)
for resolve.alias entries. Feed them as additional alias mappings into
the resolver. These configs are statically parseable in most cases
(literal objects). oxc can parse the JS/TS config files.

### 6. CommonJS re-exports

`module.exports = require('./utils')` -- the require path rewrites
on move, but `const { Foo } = require('./barrel')` where barrel
re-exports from utils has the same transitive gap as ESM re-exports.

**Fix**: same transitive resolution as gap #1. The require() specifier
is already extracted as ImportPath. Need the same chain-following logic.

## Requires runtime or type information (not fixable from static analysis alone)

### 7. Dynamic imports with variable specifiers

`require(variable)` or `import(computedString)` -- no string literal
to rewrite. Cannot determine the target file statically.

**Status**: unfixable without runtime tracing or type narrowing. If
the variable is a const assigned from a string literal in the same
file, could theoretically follow it, but this is fragile.

### 8. Rust trait method dispatch

Renaming a trait method in a trait definition should rename it in all
impl blocks. Currently only the declaration site is tracked. Finding
all impl blocks that implement a given trait requires type resolution
(which types implement which traits).

**Status**: requires type info. rust-analyzer does this via
ra_ap_hir. Possible to approximate with heuristics (same method name
in impl blocks for types that appear in the same module tree), but
high false positive risk.

### 9. String-based references across languages

A TS frontend calls `fetch('/api/users')` and a Rust backend has
`#[get("/api/users")]`. Renaming the Rust route should update the TS
string. No static analysis can link these without schema/contract
knowledge.

**Status**: requires cross-language contract definitions (OpenAPI,
GraphQL schema, etc.) as the linking layer. Out of scope for
file/import-level refactoring.

## Not a gap but worth noting

### Git operation bursts

A `git checkout` or `git rebase` floods hundreds of FS events.
Currently processed individually, which could trigger spurious
rewrites against intermediate filesystem states.

**Planned fix**: threshold detection (>30 events in a debounce window)
-> skip classify/plan/rewrite -> cooldown -> full re-scan. Design
discussed, not yet implemented.
