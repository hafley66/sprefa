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

### 2. Rust glob imports and renamed symbols

`use crate::utils::*` rewrites the module path on move. But if a
symbol pulled in via `*` is renamed in utils, there is no explicit
ref in the consuming file to rewrite. The consumer just silently
breaks.

**Fix**: expand glob imports at index time. syn parses `use foo::*`
and we already store it as an RsUse ref. At query time, resolve what
`*` expands to by cross-referencing with RsDeclare refs in the target
module. Then treat each expanded symbol as an implicit ref for rename
tracking. Does not require type info -- module-level declarations are
statically enumerable.

### 3. Cross-crate workspace imports (Rust)

`use other_crate::foo::Bar` -- sprefa tracks within a single crate's
module tree (looks for `src/`). Workspace members referencing each
other via crate names aren't resolved because file_to_mod_path maps
paths to `crate::...` relative to the nearest `src/`.

**Fix**: during scan, detect Cargo.toml workspace members and build a
map of crate_name -> root module path. When resolving `use other_crate::`,
look up the crate name in the workspace map and resolve against that
crate's file tree. All info is in Cargo.toml + the filesystem. syn
already extracts `extern crate` refs as DepName.

### 4. Consume #[path] attribute in module resolution (Rust)

`#[path = "weird_name.rs"] mod foo;` is now extracted (node_path field
on RsMod refs) but file_to_mod_path ignores it. The resolver assumes
`mod foo` lives in `foo.rs` or `foo/mod.rs`.

**Fix**: when building the module tree, check node_path on RsMod refs.
If present, use it instead of the default naming convention. All the
data is already in the DB. Pure wiring.

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
