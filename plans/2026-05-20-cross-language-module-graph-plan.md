# Cross-language module-graph extraction plan — 2026-05-20

Turbo-scrappy mechanism for pulling imports / exports / module paths / file ↔
folder edges out of any language without invoking that language's build
toolchain. Tree-sitter is the floor, specialized libs (oxc_resolver for JS/TS,
cargo-metadata for Rust, etc.) bolt on where TS queries cannot disambiguate.

Audience: anyone touching `v4/src/cst/dsls/ast/`, `feat/reify` extract crate,
or wiring a new language into the module-edge graph.

Pre-reads:
- Memory `project_cross_file_entity_graph.md`. `feat/reify` is the unmerged
  branch where the TS-only extract crate landed (commit `952d7832`, gate
  503/0/1). REIFY MACRO 3 stones GREEN there. The crate name is
  `sprefa-extract` per that memory.
- `v4/src/cst/dsls/ast/mod.rs` and `v4/src/cst/build/grammar_walk.rs` for the
  current tree-sitter wrapper surface.
- `v4/examples/npm-sim/` for the JS/TS fixture this plan needs to handle on
  day one (`acme-billing`, `acme-checkout`, `acme-logger`, `acme-store-web`,
  `acme-ui-kit`).
- `v4/examples/crawl-sim/` for the second fixture surface.

---

## 1. Problem statement

Static analysis of code in any language needs three primitives:

1. **What modules a file imports** (specifier strings, kind = type/runtime,
   default vs named, side-effect).
2. **What a file exports** (symbol, kind, re-export source).
3. **How a specifier resolves to a target file** (which is the hard part and
   varies per ecosystem).

Today the project has a TS-only extract crate on `feat/reify`. The goal is
generalization to any language the tree-sitter ecosystem covers, plus a way
to write the resolution algorithm declaratively for each ecosystem without
linking the target language's compiler.

Three forces in tension:

| force | implication |
|---|---|
| no language-specific build | cannot run `tsc`, `cargo check`, `mypy`. Must reconstruct enough of the resolver to be useful, not enough to be correct in every edge case |
| any language | parser layer must be uniform; tree-sitter is the only thing that gives that uniformly |
| turbo scrappy | resolver lives in sprf rules where possible, not in Rust. Iterate without recompiling |

---

## 2. Scope

### In

- Imports (positional, named, namespace, dynamic where statically visible).
- Exports (declarations, re-exports, default).
- Module specifier → file-path resolution per ecosystem.
- File/folder structure as facts (already covered by existing `fs` op +
  `repo` op).
- Manifest reading (`package.json`, `Cargo.toml`, `pyproject.toml`,
  `go.mod`, etc.) as facts.
- The first target ecosystems on day one are JS/TS (npm) and Rust (cargo).

### Out (this plan)

- Type-level resolution (parametric polymorphism, generic instantiation).
  See the type-IR plan.
- Symbol-level resolution past the module boundary (which function in target
  file does this specific named import bind to). Phase 2 of this plan touches
  it; full resolution is its own initiative.
- Build-system DAG (Bazel, Buck, Pants). Out of scope.
- Dynamic imports whose argument is a computed string. Capture the call site,
  flag as unresolved, move on.

---

## 3. Design overview

Three stages, one direction:

```
  extract                resolve                 emit
  -------                -------                 ----
  per-file               per-project             per-edge

  tree-sitter        +   manifest facts      +   sprf rule rows
  + opt. specialized     declarative algo        module_edge(...),
  raw facts              (sprf rules where           module_export(...),
                          possible)                  module_unresolved(...)
```

Each stage writes facts. Each downstream stage reads upstream facts via the
sprf fact-store. There is no Rust API between stages; the boundary is the
table schema.

```text
                +---------------------+
   file(path)   |   extract           |  module_import_raw(file, specifier, kind, named?, alias?)
   --------->  |   per-language       |  module_export_raw(file, symbol, kind, source?)
                +---------+-----------+
                          |
                          v   (facts in fact_store)
                +---------------------+
   manifests   |   resolve            |  module_edge(src_file, dst_file, kind, named?, target_symbol?)
   --------->  |   per-ecosystem sprf |  module_unresolved(src_file, specifier, reason)
                +---------+-----------+
                          |
                          v
                +---------------------+
                |   emit               |  graph queries: fanin/fanout, dependency cycles,
                |   downstream sprf    |  unused exports, dead imports, package boundaries
                +---------------------+
```

**Why three stages instead of one:** the extractor only needs the local
file. The resolver needs the project. Separating them lets the extractor
parallelize per-file and the resolver recompute when manifests change
without re-parsing source. The fact-store cache already gives this for free.

**Why resolve in sprf rules instead of Rust:** the user picks the ecosystem.
JS/TS resolver semantics differ between Node, Bun, Deno, Vite, Webpack,
tsconfig `paths`. Encoding each in a sprf rule lets you fork the algo by
copying a 30-line `.sprf` file. Fork. Tweak. Re-run.

---

## 4. Per-language stack picks

Triage: tree-sitter is enough when imports are syntactically explicit and
resolution is filesystem-only. Specialized libs come in when there is a
config-driven path-rewrite layer (tsconfig `paths`, package.json `exports`,
Rust `[workspace]` members + `[patch]`, Python namespace packages).

| ecosystem | extractor | resolver inputs | specialized lib needed? |
|---|---|---|---|
| JavaScript / TypeScript / TSX | tree-sitter-typescript + tree-sitter-tsx | `package.json` (exports, main, types, types-versions), `tsconfig.json` (paths, baseUrl, references), file extensions try-list, index-file try-list | **yes** — wrap `oxc_resolver` for the path/exports/types-versions logic; tree-sitter does the import-statement extraction |
| Rust | tree-sitter-rust | `Cargo.toml` workspace members, `[lib]` + `[[bin]]` paths, `Cargo.toml` `[dependencies]` keys, `src/lib.rs` / `src/main.rs` / `src/bin/*.rs` conventions, `mod` declarations | tree-sitter only for v1; cargo-metadata if needed for workspace resolution. `mod.rs` / file-per-module is filesystem walk |
| Python | tree-sitter-python | `pyproject.toml` / `setup.cfg`, `sys.path` emulation, `__init__.py` markers, relative vs absolute import distinction, namespace packages | tree-sitter only. Sys.path emulation is a list of folders, expressible as sprf facts |
| Go | tree-sitter-go | `go.mod`, `GOPATH`-style folder mapping, internal package rules | tree-sitter only. go.mod parsing is trivial |
| Java / Kotlin | tree-sitter-java / tree-sitter-kotlin | classpath, source roots from `build.gradle` / `pom.xml`, package → folder mapping | tree-sitter only for the import-line parse. Source-root determination is configurable |
| C / C++ | tree-sitter-c / tree-sitter-cpp | `compile_commands.json` for include paths, `#include` style, system vs project includes | tree-sitter for `#include` extraction; `compile_commands.json` is JSON, already loadable via existing `json` op |

Day-one picks: TypeScript + Rust. Python and Go follow once the schema and
sprf resolver pattern are validated.

---

## 5. Extractor IR

Tree-sitter queries produce raw facts. One query file per language. Output
is uniform across languages.

### 5.1 Raw fact schema

```text
module_import_raw(
    src_file:      Path,
    specifier:     Str,            // exact bytes between quotes / after `use`
    kind:          Str,            // "es-import", "es-dynamic", "cjs-require",
                                   // "rust-use", "py-import", "py-from", ...
    named:         Optional<Str>,  // imported name, None if namespace or default
    alias:         Optional<Str>,  // as-alias inside this file
    line:          U32,
    col:           U32,
    raw_span:      ByteRange,
)

module_export_raw(
    src_file:      Path,
    symbol:        Str,
    kind:          Str,            // "value", "type", "default", "re-export"
    re_source:     Optional<Str>,  // for re-exports, the specifier
    line:          U32,
    col:           U32,
)
```

Two tables. No resolution attempted at this stage. The extractor's job is
to be a faithful syntactic scribe.

### 5.2 Extractor registration

```rust
// new in v4/src/cst/dsls/ast/extractors/
pub trait ImportExportExtractor {
    fn language(&self) -> &'static str;            // "typescript", "rust", ...
    fn file_predicate(&self, path: &Path) -> bool; // .ts/.tsx for typescript, ...
    fn query_source(&self) -> &'static str;        // tree-sitter S-expr query
    fn project_capture(
        &self,
        captures: &CaptureMap,
        out: &mut Vec<RawFact>,
    );
}
//   pseudo:
//     each extractor owns one .scm query file (or in-Rust S-expr string)
//     project_capture maps capture names (@import, @specifier, @named, ...)
//     into RawFact rows
//     registry is a vec, picked by file_predicate; first match wins
//     no fallthrough; unknown languages produce zero facts
```

Lifetimes:

| type | lifetime | notes |
|---|---|---|
| `ImportExportExtractor` (impl) | process | one static instance per language |
| `CaptureMap<'tree>` | per file parse | borrows the parsed tree |
| `RawFact` | per file parse | owned, fed to FactWrite |
| query compiled `Query` | process | tree-sitter `Query` is reusable; lazy-init once |

Storage layout: extractors hold no state past the query handle. The fact
store gets the rows. Per-file extract is a pure function from (path, bytes)
to `Vec<RawFact>`.

Reads / writes:
1. fs op emits `(path, bytes)`.
2. ast op picks the language from `path`.
3. matching extractor runs its query, emits `module_import_raw` + `module_export_raw` rows.
4. one row per import statement (and one per imported name within a list import).

Uniqueness: `(src_file, line, col, named?)` — same file can re-export the
same symbol with different aliases on the same line in theory; line+col
suffices unless tree-sitter spans collide.

### 5.3 Worked TS query sketch

```scheme
; v4/src/cst/dsls/ast/extractors/typescript/imports.scm
(import_statement
  source: (string (string_fragment) @specifier)
  (import_clause
    (identifier)? @default-name
    (namespace_import (identifier) @namespace-name)?
    (named_imports
      (import_specifier
        name: (identifier) @named-name
        alias: (identifier)? @named-alias)*)?)?) @import

(export_statement
  source: (string (string_fragment) @re-source)?
  declaration: (_ (identifier) @exported-symbol)?
  (export_clause
    (export_specifier
      name: (identifier) @export-name
      alias: (identifier)? @export-alias)*)?) @export

(call_expression
  function: (identifier) @_fn (#eq? @_fn "require")
  arguments: (arguments (string (string_fragment) @specifier))) @cjs-require

(call_expression
  function: (import) ; dynamic import
  arguments: (arguments (string (string_fragment) @specifier))) @dynamic-import
```

`project_capture` reads each capture, decides the `kind`, and emits one
`module_import_raw` per imported name (or one for the whole statement if
namespace-only).

### 5.4 Worked Rust query sketch

```scheme
; v4/src/cst/dsls/ast/extractors/rust/imports.scm
(use_declaration
  argument: [(scoped_identifier) (scoped_use_list) (use_list) (identifier)]
    @use-tree) @import

(mod_item
  name: (identifier) @mod-name
  body: (declaration_list)? @body) @mod-decl

(extern_crate_declaration
  name: (identifier) @crate-name) @extern-crate
```

`project_capture` walks `@use-tree` recursively (because `use foo::{a, b::c}`
is one statement with N effective imports), emitting one row per leaf.
`mod-decl` without a body becomes a `module_import_raw` with
`kind = "rust-mod-decl"` and the resolver finds the matching `<name>.rs` or
`<name>/mod.rs`.

---

## 6. Resolver IR (the "programmable inputs" part)

The resolver is a set of sprf rules. The rules take raw facts and project
context (manifests parsed via existing `json` / `toml` ops) and emit
`module_edge` and `module_unresolved`.

### 6.1 Resolver shape per ecosystem

```sprf
; v4/examples/resolvers/resolve-ts-node.sprf
rule(:resolve_relative, ts.cols(src_file, dst_file)) {
    module_import_raw?(src_file, specifier, ...)
    where ${specifier} like './%' or ${specifier} like '../%'
    ; specifier is path-relative; join + try-extensions
    let candidate = join_relative(dirname(src_file), specifier)
    fs.try_extensions(${candidate}, [".ts", ".tsx", ".js", ".jsx",
                                     "/index.ts", "/index.tsx",
                                     "/index.js"]) -> dst_file?
}

rule(:resolve_tsconfig_paths, ts.cols(src_file, dst_file, target_symbol)) {
    ; tsconfig.json `paths` map
    tsconfig?(project_root, alias_pattern, target_pattern)
    module_import_raw?(src_file, specifier, ...)
    where ${specifier} matches ${alias_pattern}
    let candidate = substitute(${specifier}, ${alias_pattern}, ${target_pattern})
    fs.try_extensions(${candidate}, ...) -> dst_file?
}

rule(:resolve_package_exports, ts.cols(src_file, dst_file, target_symbol)) {
    ; package.json `exports` map
    package_json?(pkg_dir, pkg_name, exports_map)
    module_import_raw?(src_file, specifier, ...)
    where ${specifier} = ${pkg_name} or ${specifier} like '${pkg_name}/%'
    let subpath = strip_prefix(${specifier}, ${pkg_name})
    exports_map_lookup(${exports_map}, ${subpath}) -> ${relative}
    let candidate = join_relative(${pkg_dir}, ${relative})
    fs.exists?(${candidate}) -> dst_file?
}

; aggregator
rule(:module_edge, ts.cols(src_file, dst_file, kind, named, target_symbol)) {
    resolve_relative?(src_file, dst_file, ...)        -> module_edge?
    resolve_tsconfig_paths?(src_file, dst_file, ...)  -> module_edge?
    resolve_package_exports?(src_file, dst_file, ...) -> module_edge?
}

rule(:module_unresolved, ts.cols(src_file, specifier, reason)) {
    module_import_raw?(src_file, specifier, ...)
    not(module_edge?(src_file, _, _, _, _))
    -> module_unresolved?(src_file, specifier, "no rule matched")
}
```

The above is illustrative; the grammar is settled around `feat/callable-value`
+ ROUTE A. Adjust syntax to whatever is current when this lands.

### 6.2 Resolver helper ops

A small set of pure ops makes the resolver rules expressible without
shelling out to Rust:

| op | purpose | already exists? |
|---|---|---|
| `fs.exists(path)` | predicate | yes (fs op family) |
| `fs.try_extensions(base, exts) -> path` | try base+ext until one exists | new helper; thin wrapper over fs.exists |
| `join_relative(dir, rel)` | path join + normalize `..` | new helper |
| `dirname(path)` | parent dir | already in path ops |
| `basename(path)` | leaf | already in path ops |
| `strip_prefix(s, prefix)` | string prefix strip | new helper |
| `substitute(s, pat, target)` | TS `paths` pattern substitution | new helper, simple `*` glob |
| `exports_map_lookup(map_json, subpath)` | walk package.json `exports` tree per Node spec | needs careful encoding — see §6.3 |
| `glob_match(s, pat)` | shell-glob match | already in glob op family |
| `where ... like ...` | SQL LIKE | already (sql_where DSL) |

Each helper that is "new" is small and pure. Most are 10-line ops.

### 6.3 The hard one: `exports_map_lookup`

Node's `package.json` `exports` field has nested conditions
(`{ "import": ..., "require": ..., "types": ..., "default": ... }`),
sub-paths (`{ "./submodule": ... }`), and wildcards (`./feature/*`).

```rust
fn exports_map_lookup(
    exports: &Json,
    subpath: &str,       // "" or "./foo" or "./foo/bar"
    conditions: &[&str], // ["types", "import", "default"] for TS, etc.
) -> Option<String>;
//   pseudo:
//     normalize subpath to "." or "./<rest>"
//     if exports is a string and subpath == "."  -> return string
//     if exports is an object:
//       1. exact subpath match
//       2. wildcard subpath match (longest pattern wins)
//       3. for the chosen value, walk conditions in priority order;
//          first matching condition wins
//     return None on miss
```

This is the one place we cannot dodge writing real Rust. Either implement
the Node spec subset in-process (~200 lines) or wrap `oxc_resolver` and call
it from a thin op (`oxc_resolver::Resolver::resolve(directory, specifier)`).
**Recommendation:** wrap `oxc_resolver`. It is the spec, maintained by a
team that cares about it, and it already handles `paths`, `exports`,
`imports`, `types-versions`, package conditions, and the conditional fall-
back tree. Map its output back into a single `dst_file` + reason.

```rust
// v4/src/cst/dsls/ast/extractors/typescript/oxc_resolve.rs
pub fn resolve_ts(
    base_dir: &Path,
    specifier: &str,
    project_options: &ResolveOptions,  // tsconfig paths, baseUrl, conditions, ...
) -> Result<PathBuf, ResolveError>;
//   pseudo:
//     Resolver::new(project_options).resolve(base_dir, specifier)
//     translate oxc errors into module_unresolved.reason strings
```

The Rust op surface (`fn op_resolve_ts(...) -> ...`) is the only language-
specific Rust the resolver needs. Other ecosystems (Rust, Python, Go) are
filesystem-only and stay in sprf.

### 6.4 Programmable inputs to the algo

The user said: program the algo for whatever inputs without building the
program. Concretely, the inputs are:

| input | source | how programmable |
|---|---|---|
| project root | `repo()` op already binds `ROOT` and `SLUG` | sprf rule chooses which root to scope to |
| manifests | parsed via existing `json` / `toml` ops | rules pick which fields to read |
| extension try-list | hard-coded today; lift into a sprf fact `extensions(lang, ext, priority)` | edit the .sprf file |
| index-file convention | hard-coded today; lift into `index_file(lang, name)` | edit the .sprf file |
| conditions priority | per ecosystem; lift into `resolver_conditions(lang, condition, priority)` | edit the .sprf file |
| custom path alias | per project; read from tsconfig / Cargo.toml / pyproject | resolver rule reads the manifest fact |
| custom resolver fork | the user can copy a resolver .sprf and tweak one rule | filesystem |

All inputs are facts. The resolver consumes facts. Fork = copy a 30-line
.sprf and edit a rule body. No recompile.

---

## 7. Output schema

Two tables sit at the "stable" boundary downstream graph queries should
read from:

```text
module_edge(
    src_file:        Path,
    dst_file:        Path,
    kind:            Str,            // "es-import", "rust-use", ...
    named:           Optional<Str>,  // imported name (when known)
    alias:           Optional<Str>,
    target_symbol:   Optional<Str>,  // where the named import resolves in dst (often None at this stage)
    via:             Str,            // which resolver rule emitted this edge (for debugging)
    line:            U32,
    col:             U32,
)

module_unresolved(
    src_file:        Path,
    specifier:       Str,
    kind:            Str,
    reason:          Str,
    line:            U32,
    col:             U32,
)
```

Uniqueness:

| key | constraint |
|---|---|
| `(src_file, line, col, named)` | one edge per imported name per import site |
| `(src_file, line, col)` for unresolved | one unresolved entry per import site |
| `kind` | enum-shaped string; the set is closed per ecosystem but open across the system |

Downstream queries (illustrative, week-1):

```sprf
; fan-in: who imports me?
rule(:fanin, fanin.cols(dst_file, importer)) {
    module_edge?(importer, dst_file, ...)
}

; cross-package edges (where src_pkg != dst_pkg)
rule(:cross_pkg_edge, cp.cols(src_pkg, dst_pkg)) {
    module_edge?(src_file, dst_file, ...)
    repo?(src_file, src_pkg, ...)
    repo?(dst_file, dst_pkg, ...)
    where ${src_pkg} != ${dst_pkg}
}

; dependency cycles among files
rule(:file_cycle, fc.cols(a, b)) {
    module_edge?(a, b, ...)
    module_edge?(b, a, ...)
    where ${a} < ${b}
}
```

These are downstream consumers and not part of this plan's deliverables;
listed to ground the schema.

---

## 8. Worked example: npm-sim acme-billing → acme-logger

Fixture: `v4/examples/npm-sim/acme-billing` imports from `acme-logger`.

```text
acme-billing/src/index.ts:
  import { log } from 'acme-logger';

acme-billing/package.json:
  { "dependencies": { "acme-logger": "1.0.0" } }

acme-logger/package.json:
  { "name": "acme-logger",
    "exports": { ".": "./src/index.ts" } }
```

Flow:

| stage | row produced |
|---|---|
| extract | `module_import_raw("acme-billing/src/index.ts", "acme-logger", "es-import", "log", None, ...)` |
| extract | `module_export_raw("acme-logger/src/index.ts", "log", "value", None, ...)` |
| resolve (via oxc_resolver wrapper) | `module_edge("acme-billing/src/index.ts", "acme-logger/src/index.ts", "es-import", "log", None, None, "resolve_package_exports", ...)` |

If `acme-logger/package.json` had no `exports` field and a `main: ./src/index.js`, the resolver would walk the `main` fallback (oxc handles that
internally). If neither, `module_unresolved(..., reason: "no main, no exports")`.

---

## 9. Worked example: Rust cargo workspace

Fixture: `v4/` is a Cargo workspace with members.

```text
v4/src/app.rs:
  use crate::store::SprfStore;
  use effect_runtime::v2::fact_store::SqliteFactStore;
```

| stage | row produced |
|---|---|
| extract | `module_import_raw("v4/src/app.rs", "crate::store::SprfStore", "rust-use", "SprfStore", None, ...)` |
| extract | `module_import_raw("v4/src/app.rs", "effect_runtime::v2::fact_store::SqliteFactStore", "rust-use", "SqliteFactStore", None, ...)` |
| manifest | `cargo_workspace_member("v4/", "effect_runtime", "v3/crates/effect_runtime/Cargo.toml")` |
| manifest | `cargo_lib_root("effect_runtime", "v3/crates/effect_runtime/src/lib.rs")` |
| resolve (`crate::` → same crate root) | `module_edge("v4/src/app.rs", "v4/src/store.rs", "rust-use", "SprfStore", None, None, "resolve_rust_crate_self")` |
| resolve (external crate → workspace member) | `module_edge("v4/src/app.rs", "v3/crates/effect_runtime/src/lib.rs", "rust-use", "SqliteFactStore", None, None, "resolve_rust_workspace_member")` |

The target-symbol resolution (which exported item in `lib.rs` is
`SqliteFactStore`) is a follow-on. The edge in the graph terminates at the
module root; per-symbol resolution is its own pass.

---

## 10. Phases

```
Phase A  schema + tree-sitter extractor scaffold + day-1 TS extractor
Phase B  oxc_resolver wrapper + TS resolver sprf rules + npm-sim green
Phase C  Rust extractor + Rust resolver sprf rules + v4-self green
Phase D  python or go (pick one based on user need)
Phase E  per-symbol resolution for TS named imports (uses existing entity-graph work on feat/reify)
```

Phases A → B → C are sequential. D is independent of E. E depends on C
because Rust's `pub use` re-export shape stresses the symbol resolver.

### Phase A — schema + scaffold

Targets:

| file | change |
|---|---|
| `v4/src/cst/dsls/ast/mod.rs` | add `extractors` submodule + `ImportExportExtractor` trait per §5.2 |
| `v4/src/cst/dsls/ast/extractors/typescript/` | new dir; one query file, one Rust file mapping captures to rows |
| `v4/src/cst/dsls/ast/extractors/registry.rs` | static `&[&dyn ImportExportExtractor]` indexed by language string |
| schema | document `module_import_raw` and `module_export_raw` in `v4/docs/v4-glossary.md` |
| test | `v4/tests/module_extract_ts_target.rs` against an inline fixture |

Acceptance: running ast op over `npm-sim/acme-billing/src/index.ts` produces
the expected `module_import_raw` row.

### Phase B — TS resolver

Targets:

| file | change |
|---|---|
| `v4/Cargo.toml` | add `oxc_resolver = "x.y"` |
| `v4/src/cst/dsls/ast/extractors/typescript/oxc_resolve.rs` | thin wrapper per §6.3 |
| new sprf op `ts_resolve(specifier, base_dir)` | calls the wrapper, emits resolved path or None |
| `v4/examples/resolvers/resolve-ts-node.sprf` | the day-1 resolver rules per §6.1 |
| test | `v4/tests/module_resolve_ts_npmsim_target.rs` — npm-sim cross-package edges |

Acceptance: `resolve-ts-node.sprf` over `npm-sim/` yields the expected
`module_edge` set across all 5 acme-* packages with zero `module_unresolved`
for the curated fixture.

### Phase C — Rust resolver

Targets:

| file | change |
|---|---|
| `v4/src/cst/dsls/ast/extractors/rust/` | new dir; queries + capture mapping |
| `v4/examples/resolvers/resolve-rust-cargo.sprf` | crate-self + workspace-member rules |
| new sprf helper ops (if needed for path normalize / strip_prefix) | small |
| test | `v4/tests/module_resolve_rust_v4_self_target.rs` against the v4 crate itself |

Acceptance: running the resolver over `v4/src/` produces edges for `crate::`
and `effect_runtime::` paths and zero unresolved for paths that match a
workspace member.

### Phase D — Python or Go

Pick one based on whichever ecosystem the user is targeting next. The
shape mirrors Phase C: extractor + sprf resolver. No new Rust ops needed
beyond what Phase A and C already add (filesystem helpers).

### Phase E — Per-symbol resolution

Depends on entity-graph work on `feat/reify` being merged. The bridge from
`module_edge(src, dst, ..., named, target_symbol = None)` to
`target_symbol = Some(...)` is:

```sprf
rule(:resolve_named, ns.cols(src, dst, named, target_symbol)) {
    module_edge?(src, dst, ..., named, None)
    module_export_raw?(dst, target_symbol, ...)
    where ${named} = ${target_symbol}
       or (exists re-export: module_export_raw?(dst, target_symbol, "re-export", ...)
           and re_source resolves to a file that exports named)
    -> module_edge?(src, dst, ..., named, ${target_symbol})  ; replace
}
```

The "replace" semantics here ride on the existing retraction/recursion
work that merged on `main 37bb93a5`.

---

## 11. Lifetimes table

| type | location | lifetime | wrapper |
|---|---|---|---|
| `ImportExportExtractor` (impl) | extractors/<lang>/mod.rs | process | static registry |
| `tree_sitter::Query` | per extractor | process | lazy `OnceLock` |
| `RawFact` | per file parse | function-local | passed to FactWrite |
| `oxc_resolver::Resolver` | wrapper module | per ResolveOptions instance; cache per project root | `Lazy<Mutex<HashMap<ProjectRoot, Resolver>>>` |
| `ResolveOptions` | sprf op input | per resolve call | derived from manifest facts |
| resolver `.sprf` rules | filesystem | per program | rebuilt per ingest like any sprf rule |
| `module_import_raw` / `module_export_raw` rows | fact_store | per `(src_file, src_gen)` | retract + replace on file edit (existing retraction support) |
| `module_edge` / `module_unresolved` rows | fact_store | derived; rebuilt when raw + manifests change | recursion + retraction handles this |

---

## 12. Storage layout

| table | columns | indexes |
|---|---|---|
| `module_import_raw` | `(src_file, specifier, kind, named?, alias?, line, col, raw_span)` | PK `(src_file, line, col, named)`; index on `(specifier)` for resolver lookups |
| `module_export_raw` | `(src_file, symbol, kind, re_source?, line, col)` | PK `(src_file, symbol, line, col)`; index on `(symbol)` for Phase E |
| `module_edge` | `(src_file, dst_file, kind, named?, alias?, target_symbol?, via, line, col)` | PK `(src_file, line, col, named)`; index on `(dst_file)` for fan-in queries |
| `module_unresolved` | `(src_file, specifier, kind, reason, line, col)` | PK `(src_file, line, col)` |

These ride on the existing fact_store schema mechanics; no new infra. The
PKs above are advisory; current `_id` derivation in `SqliteFactStore`
already gives row uniqueness.

---

## 13. Sequence of reads / writes per operation

```text
on file edit "X.ts":
  1. fs op produces (path = X.ts, bytes)
  2. ast op picks TS extractor
  3. TS extractor runs query, writes module_import_raw + module_export_raw rows
     keyed on (X.ts, line, col, named); old rows for X.ts retract via the
     existing retraction surface
  4. resolver rules wake on the module_import_raw delta
  5. for each new specifier, the resolver picks a branch:
       relative   -> filesystem try-extensions
       package    -> ts_resolve op -> oxc_resolver
       tsconfig path -> substitute + try-extensions
  6. module_edge rows are written; module_unresolved for remaining
  7. downstream queries (fanin, cycles, etc.) wake on module_edge delta

on package.json edit:
  1. fs + json ops produce the new manifest fact
  2. resolver rules that read package.json wake; rules that read other
     manifests do not
  3. only module_import_raw rows whose specifier matches the changed
     package recompute their edge
```

Reactivity arc: the same recursion + OWNER-SUBSCRIBE work that landed on
`main 37bb93a5` handles all of this. No new runtime work required.

---

## 14. Uniqueness conditions

| invariant | enforced where |
|---|---|
| one `module_import_raw` row per (src_file, import-site, named) | extractor's emit loop; tree-sitter span uniqueness |
| one `module_edge` row per (src_file, import-site, named) | resolver rules union into a single edge per site; the "first resolver wins" or "explicit priority" rule decides ties |
| `kind` strings are per-ecosystem but open across the system | enforced by convention in the extractor's `kind:` mapping |
| `via` records which resolver rule produced the edge | debug-only; not load-bearing for correctness |
| an import that hits both `module_edge` AND `module_unresolved` is a bug | downstream sanity check; assert in tests |

---

## 15. Tests to add

| phase | test file | content |
|---|---|---|
| A | `v4/tests/module_extract_ts_target.rs` | inline TS fixture; assert 4 import rows for a 4-import file |
| A | `v4/tests/module_extract_rust_target.rs` | inline Rust fixture; `use foo::{a, b::c}` expands to 2 leaves |
| B | `v4/tests/module_resolve_ts_npmsim_target.rs` | full npm-sim resolution; expected `module_edge` count + 0 unresolved |
| B | `v4/tests/module_resolve_ts_tsconfig_paths_target.rs` | tsconfig `paths` alias resolution |
| C | `v4/tests/module_resolve_rust_v4_self_target.rs` | run resolver against v4's own source tree; spot-check edges |
| C | `v4/tests/module_resolve_rust_workspace_target.rs` | crate-self + workspace-member edges |
| D | `v4/tests/module_resolve_python_or_go_target.rs` | minimal fixture per chosen language |

The `_target.rs` naming convention matches the existing test layout.

---

## 16. Open questions

1. **Lib for TS resolution:** `oxc_resolver` vs hand-rolling a Node-spec
   subset. Recommendation: oxc. Hard-cost: one Cargo dep + the wrapper
   module. Soft-cost: oxc compiles fast and is maintained.
2. **Where does the resolver `.sprf` live:** alongside the target project
   (`./resolve-ts-node.sprf`) so users can fork per-project, or shipped as a
   built-in alongside `v4/examples/`? Recommendation: ship default in
   `v4/examples/resolvers/`, document that users can copy and edit.
3. **Manifest parsing:** do `package.json` + `tsconfig.json` get a
   specialized op (`ts_manifest`) or go through the existing `json` op + sprf
   rules that pick fields? Recommendation: existing `json` op. The fields the
   resolver needs are small; sprf-rule extraction is in the user's hands.
4. **Re-export chains:** how deep does the resolver follow `export * from
   './foo';` re-exports? Recommendation: lazy via Phase E; don't expand chains
   at resolve time, only when a downstream query asks for `target_symbol`.
5. **Multi-root projects:** `repo()` already binds one ROOT term. A workspace
   with sub-projects (lerna, pnpm workspaces, Cargo workspace) needs N roots.
   Are workspace members all under one `repo()` binding, or is each a
   separate one? Plan assumes one `repo()` per outer workspace with sub-
   manifests as facts. Confirm.
6. **Per-symbol resolution timing:** Phase E depends on `feat/reify`
   merging. Acceptable to gate the plan on that, or push toward an
   alternative path?
7. **Dynamic imports with computed strings:** capture and flag (current
   plan), or attempt constant-folding when the argument is a string literal
   built from concatenation? Recommendation: flag for v1; constant-fold is
   easy to add later if needed.
8. **Performance ceiling:** day-one fixtures are tens of files. Real-world
   monorepos are tens of thousands. The resolver-as-sprf-rules path is
   pleasant but slower than a Rust loop. Recommendation: ship the sprf
   resolver first; profile at 10k files; only then consider lifting the
   hottest resolver branch into a Rust op.

---

## 17. Out of scope (deliberate)

- Type-checking imports (does `import { foo }` correspond to an actual
  exported `foo`?). Phase E hint above is the closest this plan goes.
- Bundler-specific resolution (Webpack `resolve.alias`, Vite plugins). Add
  them as new `.sprf` resolvers later.
- Editor-time incremental: tree-sitter's `Tree::edit` is available; using it
  for sub-file delta extraction is a perf chore for later.
- Cross-repo edges (one repo imports a package published from another).
  Treat the published artifact as the resolution target; provenance back to
  source repo is a higher-layer query.

---

## 18. Effort estimate

| Phase | size | gating |
|---|---|---|
| A | ~1.5 days | none |
| B | ~2 days | Phase A landed; oxc_resolver picked |
| C | ~1.5 days | Phase A landed |
| D | ~1 day | language chosen |
| E | ~3 days | `feat/reify` merged |

Total to Phase C green on day-1 fixtures: ~5 working days.
