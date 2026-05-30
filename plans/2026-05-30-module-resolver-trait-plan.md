# Module-resolver trait: cross-language file dependency graph

Goal: "the filesystem from language, as statically as possible." A diet/parse-only
SCIP at module/file granularity: file A depends on file B, across rs+ts, with
cycles/fan-in/fan-out/reaches via the existing `closure()`. No compiler, no
scip/tsc/cargo-check. ~90% correctness.

Builds on data-model Stage 1 (built-in `repo`/`rev`/`content`/`file` relations,
`6e2fb58`). Resolution math lives in Rust (a trait), so `.dl` never needs an
expression layer (`Term` has no `Call`; `dir()`/`join()` would be new grammar).

## Layer 1: type signatures (v5/src/modgraph.rs)

```rust
pub enum Resolution { File(String), External(String), Unresolved(String) }
pub struct ModuleRef { pub specifier: String, pub kind: &'static str, // "mod"|"use"|"import"
                       pub line: u32, pub target: Resolution }
pub struct ProjectCx<'a> { root: &'a Path, files: &'a HashSet<String>,
                           rust_index: OnceCell<HashMap<String,String>> } // mod_path -> file
pub trait ModuleResolver {
    fn exts(&self) -> &'static [&'static str];
    fn edges(&self, file: &str, content: &str, cx: &ProjectCx) -> Vec<ModuleRef>;
}
pub struct RustResolver;   // path arithmetic, no dep
pub struct TsResolver { resolver: oxc_resolver::Resolver }  // oxc, real-fs probe
pub fn resolvers(root: &Path) -> Vec<Box<dyn ModuleResolver>>;  // registry
```

## Layer 2: bodies (pseudo-code)

RustResolver::edges:
- regex mod decls `^\s*(pub..)?mod\s+(\w+)\s*;` -> kind "mod", target by FS
  convention from the importing file's own path (mod.rs/lib.rs/main.rs vs name.rs),
  NOT via mod-path round-trip. candidates dir/foo.rs, dir/foo/mod.rs.
- regex use decls `^\s*(pub..)?use\s+([^;]+);` -> for each braced/aliased segment,
  resolve_to_absolute(use_path, from_mod) [crate/self/super]; longest mod-path
  prefix present in cx.rust_index() -> File; std/serde/etc -> External; else Unresolved.
- from_mod = file_to_mod_path(file). rust_index built once over cx.files.

TsResolver::edges:
- regex specifiers after `from`/`import(`/`require(`/bare `import '...'`.
- oxc_resolver.resolve(importing_dir, specifier): Ok -> abs path; strip root ->
  rel in cx.files -> File; resolves outside root (node_modules) -> External; Err ->
  Unresolved. relative `.`/`..` only -> Unresolved means missing file.

## Layer 3: instance lifetimes

- RustResolver/TsResolver: built per `refresh_module_rels` call (root-scoped), dropped
  at end of tick. oxc Resolver holds its own fs cache; rebuilt each refresh = correct
  but no cross-tick reuse (cache-invalidation deferred; first cut favors soundness).
- ProjectCx: per (repo,rev) group within one refresh. rust_index lazy, one build/group.

## Layer 4: storage + read/write sequence + uniqueness

Three built-in relations (reserved names, like repo/rev/content/file), declared every
tick, populated by `refresh_module_rels` only when the program references one:
- module_import(file: path, rev: text, specifier: text, kind: text, line: int)  PK all
- module_edge(src: path, dst: path)  PK(src,dst)  -- the 2-col closure edge
- module_unresolved(file: path, specifier: text, reason: text)  PK all

Write sequence per tick (after refresh_builtin_rels):
1. DELETE all three. 2. group `_file` rows (path,rev,hash) by rev. 3. per rev build
ProjectCx + read content (read_content); pick resolver by ext; edges(); emit rows.
`reaches(a,b) <- closure(module_edge).` then works unchanged.

Uniqueness: module_edge is a set on (src,dst) so closure condensation is clean.
Cross-rev: edges resolved within one rev; module_edge merges path pairs across revs
(acceptable Stage 1, single-rev WORK is the common case). Documented limitation.

## Tasks

- [x] this plan
- [ ] modgraph.rs trait + RustResolver + unit tests (file_to_mod_path, resolve_to_absolute, edges)
- [ ] engine: MODULE_RELS reserved + declared; refresh_module_rels(rust); call in tick/tick_paths under files-used + files-changed gate; mark changed for incremental
- [ ] tests/module_graph.rs: rust mod/use edges + reaches closure
- [ ] TsResolver + oxc_resolver dep; cross-language reaches test
- [ ] session/doc update

## Lever coverage (2026-05-30, all ✓)

Rust: mod decl, inline mod (no edge), use crate/self/super (multi-level), external
crate → External, brace groups + alias + pub use, glob, **#[path] override**,
**multi-crate crate:: namespace** (Cargo.toml registry), **cross-crate use othercrate::**,
**nested braces**, **raw idents r#**.
TS: relative/ext-probe/index, import/export-from/export*/require/dynamic import(),
import type, bare → External, **per-package tsconfig (Auto + resolve_file)**,
**workspace package.json fallback**, **static template literal** (interpolated import()
→ Unresolved(dynamic), correctly unresolvable).
Cross-cutting: **comment/string stripping** (no phantom edges from use/import in
comments or string literals).

Validation: `rust-analyzer scip` differential oracle (tests/oracle_rust.rs) — on the
2-crate workspace fixture, module_edge == RA's symbol-resolved file graph, precision
1.00, recall 1.00. 8 unit + 8 integration + 1 oracle.

Remaining genuine gaps (not levers, harder): `#[path]` nested >1 level; macro-generated
modules; cfg-gated mods over-approximated (emitted regardless); cross-rev edge merge;
recall on large code (method/type-resolved refs are SCIP's hard 10%, by design out of scope).

## Known cut corners (diet, by design)

- regex extraction (not ast-grep) -> string-literal `mod`/`use` in comments could
  false-positive; acceptable at 90%. inline `mod foo {` correctly excluded (no `;`).
- use-edge resolves to longest module-path prefix that is a file -> points at the
  parent module file, not the exact item; symbol-level is SCIP's hard 10%.
- #[path] overrides: not honored in first cut (rare); note for follow-up.
- re-reads file content each refresh (reconcile already hashed it); cache later.
