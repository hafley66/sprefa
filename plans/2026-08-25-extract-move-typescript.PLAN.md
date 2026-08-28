# extract move for TypeScript: corpus walk, path resolution, batch list

Auditor copy. Every claim carries `path:line`. Design decided 2026-08-25, not
re-litigated. No Rust written in this lane; signatures live here only.

<!-- todo(triage): resolve_file on a bare specifier that aliases into the source tree is a move target; a bare specifier resolving to node_modules is not. The within_root gate decides. -->

## Context

`extract move` is prolog-only by construction. `0_move.rs` walks `.pl/.plt`
with `prolog_files` (`0_move.rs:461-487`), resolves prolog specifiers against
the loading file's directory taking `.pl` when bare (`0_move.rs:490-501`), and
stages one soopy `Replace` per importer plus one `Move` per moved file
(`0_move.rs:71-86`, `250`). The rule is data, `rules/move_specifier.yml`,
with `language: prolog` prepended by the caller (`0_move.rs:316-325`).

`lang/ts.rs` already emits `Specifier` rows for ES static imports and export-FROM
re-exports via oxc (`ts.rs:1143-1273`, `module_specifiers`), but nothing
consumes them and they exclude `require(...)` and `import x = require(...)`
(`ts.rs:1149-1152`). For TS the move reads these rows and their byte spans
directly; the prolog arm keeps its self-built `move_candidate` rel from its own
ast-grep scan (arc C, `0_move.rs:358-395`).

Missing for TS: a `.ts/.tsx/.mts/.cts` corpus walk, TS path resolution
(extensionless, `index.ts`, `.js` written for `.ts`, package.json `exports`/
`main`, tsconfig `paths`/`baseUrl`), dynamic `import()`/`require()` as specifier
rows, and a batch form `--list <tsv>` that stages every move and every importer
rewrite in one soopy transaction. First real corpus: hafley-rxjs grapht layout
(38 files into 4 folders), done by hand in another session. This plan builds on
the merged arcs A (#472), B (#473), C (#475).

Nothing is reinvented: syn parses rust, oxc parses ts, and tree-sitter fills
gaps (prolog, dl6, markdown); the move reads module specifiers from the oxc
parse it already runs and only ast-grep matches where oxc has no node.

## The four decisions

The issue lists four decision areas. Each is an arc below: corpus walk (arc 1),
the TS specifier sources (arc 2), path resolution (arc 3), batch `--list`
(arc 4). Arcs 1-3 are the single-move TS arm; arc 4 generalizes the plan to
many moves.

## Arc 1: the TS corpus walk

### Decision

`prolog_files` (`0_move.rs:461-487`) is the walker. It BFS-walks the root,
skips `[.git, target, node_modules, .boop-worktrees]` (`0_move.rs:463`), and
collects `.pl/.plt`. The TS arm generalizes the same walker to the four TS
extensions plus the JS ones the ESM-style corpus writes.

Chosen: one corpus walker parameterized by extension set, collecting
`.ts .tsx .mts .cts .js .jsx .mjs .cjs`. Rejected: two separate walkers (one
prolog, one ts); the two never overlap and one walker with a predicate is less
code than two.

The JS extensions are in scope because the grapht corpus writes ESM-style
`.js` specifiers and a `.js` file can itself carry `import './x'` or
`import('./x')` that names a moved file. The corpus walk names a file a
"specifier carrier" if its grammar is TS-family.

Monorepo roots: hafley-rxjs is a workspace with a root `tsconfig.json` whose
`references` list eight `packages/*` (`~/projects/hafley-rxjs/tsconfig.json:1-9`).
The move's `--root` is the git root containing `old` (default, `0_move.rs:113-125`),
which for a monorepo is the repo root; the walker skips `node_modules` and
`.boop-worktrees` so the corpus stays the source tree.

### Type signatures

```rust
// v6/sprefa-extract/src/0_move.rs (generalize prolog_files)
/// A corpus file plus which front end reads its specifiers. Prolog files go
/// through the ast-grep rule; TS-family files go through the oxc Specifier rows
/// (arc 2). CorpusLang only classifies the walker; it is not an ast-grep grammar.
enum CorpusLang { Prolog, Ts, Tsx }

/// Files that can carry a module specifier, in path order, each with its front end.
fn specifier_corpus(root: &Path) -> Vec<(PathBuf, CorpusLang)>;

/// The per-grammar extension membership test.
fn is_ts_family(path: &str) -> bool; // .ts .tsx .mts .cts .js .jsx .mjs .cjs
```

Body (pseudo-code):

```rust
fn specifier_corpus(root: &Path) -> Vec<(PathBuf, CorpusLang)> {
    // BFS from root, skip [.git target node_modules .boop-worktrees] (mirror
    // 0_move.rs:463); collect files by extension:
    //   .pl/.plt -> Prolog
    //   .ts/.cts/.mts/.js/.mjs/.cjs -> Ts      (oxc SourceType, ts.rs:89-99)
    //   .tsx/.jsx -> Tsx                       (oxc SourceType, ts.rs:89-99)
    // sort by path (preserve the merge-order guarantee, 0_move.rs:139)
}
```

### Instance lifetimes

`CorpusLang` is a `Copy` token; the per-file value lives for the parse call. A
Prolog file is parsed by ast-grep with `ExtractLang::Prolog`
(`extract_lang.rs` `build_pattern`); a TS-family file is parsed by oxc through
the existing `TsSource` (`lang/ts.rs:19-81`). The `Vec<(PathBuf, CorpusLang)>`
lives for `Plan::build`, parallel-scanned by the rayon pool (`0_move.rs:141-155`).

### Storage layout

None new. Read sequence: root -> walk -> per-file `(path, lang)` -> bytes ->
(oxc parse for TS, ast-grep rule for prolog) -> the file's specifier rows.
Uniqueness: one `(path, lang)` per file, first extension match wins.

### Files touched (Arc 1)

- `v6/sprefa-extract/src/0_move.rs`: replace `prolog_files` with
  `specifier_corpus`; thread the per-file `CorpusLang` into the oxc parse (TS)
  or the ast-grep rule (prolog).
- `v6/sprefa-extract/src/lang/mod.rs`: no change (TS already routes via
  `TsSource`, `mod.rs:59-71`).

### Files forbidden (Arc 1)

- `src/lang/ts.rs` projection logic (the oxc `Specifier` emission is extended in
  arc 2, not rewritten here).
- `v6/prolog/**`, the soopy crate.

### Tests to add (Arc 1)

- `corpus_walks_ts_and_tsx_but_skips_vendor` (tests/1_move.rs): a temp repo
  with `.ts`, `.tsx`, `.mts`, `.cts`, `.js`, `.jsx`, a `node_modules/` and a
  `.boop-worktrees/` dir; assert the walker returns the six TS-family files and
  not the two skip dirs. Breaks if the skip list or extension set is wrong.

### Gate command (Arc 1)

```sh
cargo test --release --features cli --test 1_move
```

### Risk table (Arc 1)

| risk | citation that raises it | mitigation |
|---|---|---|
| `.js`/`.jsx` carriers double-count when a `.js` re-exports a `.ts` | ts.rs:1149-1152 (extractor omits some) vs the move's own scan | the move scans every carrier independently; a `.js` that names a moved `.ts` is a real importer to rewrite |
| monorepo walk cost on a large tree | 0_move.rs:461-487 BFS | skip list already drops `node_modules`/`.boop-worktrees`; corpus scale is the move's existing bound |

## Arc 2: the TS specifier sources

### Decision: read the move's specifier rows off the oxc parse, no TS rule

The oxc parse already runs for every TS file (`lang/ts.rs:19-81`, oxc_parser +
oxc_ast + oxc_ast_visit, `Cargo.toml:40-45`), and `module_specifiers`
(`ts.rs:1143-1273`) already emits a `Specifier` row per static import and
export-FROM re-export, each carrying a byte span (`push_specifier`,
`ts.rs:1265-1273`). The move feeds those spans straight into the arc B drain as
`BoundEdit` (`types.rs:2527`, `drain.rs:17-30`); no tree-sitter-typescript
matching, no `rules/move_specifier.typescript.yml`. ast-grep stays for the
prolog arm only (`0_move.rs:316-325`).

The gap: `module_specifiers` covers static `import` and `export ... from`
(`ts.rs:1143-1254`), but not dynamic `import()` or `require()` (`ts.rs:1149-1152`
names them as NOT covered). Those land as new rows in the SAME oxc visitor, with
byte spans, cited to the oxc node types.

### The new specifier rows

Dynamic `import()` is an `ImportExpression` with a `source: Expression`
(`oxc_ast-0.135.0/src/ast/js.rs:124-125, 2516-2520`). `require()` is a
`CallExpression` with a `callee` naming `require` and an `arguments` vec whose
first element is a `StringLiteral` (`js.rs:613-625`, `StringLiteral` at
`literal.rs:84-100`). Both carry `Span` (`js.rs:617, 2518`). The visitor adds a
row per module path:

```rust
// v6/sprefa-extract/src/lang/ts.rs, in module_specifiers (the same oxc Program walk)
match stmt {
    ts::Statement::ExpressionStatement(expr) => match &expr.expression {
        // import('./m') -> ImportExpression.source is a StringLiteral (js.rs:2516-2520)
        ts::Expression::ImportExpression(imp) => {
            if let ts::Expression::StringLiteral(lit) = &imp.source {
                push_specifier(sink, strings, lit.span, &lit.value, SpecifierKind::Dynamic, &lit.value, None);
            }
        }
        // require('./m') -> CallExpression, callee name "require" (js.rs:613-625)
        ts::Expression::CallExpression(call) => {
            if call.callee_name() == Some("require")
               && let Some(ts::Argument::StringLiteral(lit)) = call.arguments.first() {
                push_specifier(sink, strings, lit.span, &lit.value, SpecifierKind::CommonJS, &lit.value, None);
            }
        }
        _ => {}
    },
    _ => {}
}
```

`SpecifierKind` gains `Dynamic` and `CommonJS` variants; the existing
`Named`/`Default`/`Namespace`/`SideEffect`/`Reexport`/`Include`/`ReexportModule`
set (`types.rs:563-570`, `ts.rs:1167-1254`) is untouched. The `name` column is
the module path and `module` is the same value, matching the path-only forms
(`ts.rs:1162-1171`, `1271-1273`).

### Feeding the drain

`push_specifier` already interns the span into the row (`ts.rs:1271`). The
drain's `SpecifierRewrite` (`0_move.rs:447-470`) is replaced by a TS rewrite
that maps a `Specifier` span to its replacement text: a spec names a moved file
when `resolve` (arc 3) maps its `value` to a moved `old`; the replacement is the
re-aimed path (`spec_text_ts`, arc 3). `BoundEdit` carries the span + producer
(`types.rs:2527`, `drain.rs:17-30`), so the arc B `drain_edits`/`replace_action`
(`drain.rs:63-97`) fold the spans into one soopy `Replace` per file unchanged.

### Type signatures

```rust
// v6/sprefa-extract/src/lang/ts.rs
/// The per-file specifier rows for the move: every static import/export-FROM
/// (existing) plus dynamic import() and require() (new).
fn module_specifiers(program: &Program<'_>, strings: &mut Strings, sink: &mut FamilyBundle<CallF>);

// v6/sprefa-extract/src/0_move.rs
/// The one-per-file map the move rewrites by: span -> replacement text, for
/// the specs that resolve to a moved file. The span is the oxc row's byte span.
fn ts_rewrites(
    rows: &[Specifier],          // from ts.rs module_specifiers output
    moved: &BTreeMap<String, String>, // old_rel -> new_rel (arc 4)
    dir: &Path, root: &Path,
) -> BTreeMap<(usize, usize), String>; // (start, end) -> replacement bytes
```

Body (pseudo-code):

```rust
fn ts_rewrites(rows, moved, dir, root) -> BTreeMap<(usize,usize), String> {
    let mut out = BTreeMap::new();
    for row in rows {
        let Some(target) = resolve(&dir, row.value) else { continue };   // arc 3
        let Some(new_rel) = moved.get(target_rel(root, &target)) else { continue };
        let replacement = spec_text_ts(dir, root.join(new_rel), row.value); // arc 3
        out.insert((row.span.start, row.span.end), replacement);           // to_span, ts.rs:45-51
    }
    out
}
```

### Instance lifetimes

The oxc `Program` lives for the parse call (`ts.rs:19-81`, arena-mastered);
the `Specifier` rows borrow its interned strings. The span -> replacement map
lives for `Plan::build` and is folded into `BoundEdit`s (`types.rs:2527`).
`Specifier` is `Clone` (`types.rs`), so the map can outlive the parse.

### Storage layout

Input: the oxc `Program` (already parsed for the type/call families). Read
sequence: parse -> `module_specifiers` -> `Vec<Specifier>` -> resolve (arc 3)
-> span map -> `BoundEdit` -> soopy `Replace`. Uniqueness: one edit per span,
deduped by `(start, end)` (`drain.rs:84-97`).

### Files touched (Arc 2)

- `v6/sprefa-extract/src/lang/ts.rs`: `module_specifiers` gains the
  dynamic-import/require arms; `SpecifierKind` gains `Dynamic`/`CommonJS`.
- `v6/sprefa-extract/src/types.rs`: `SpecifierKind` enum gains the two variants
  (declared with the other kinds, `types.rs`).
- `v6/sprefa-extract/src/0_move.rs`: `ts_rewrites`, drop the ast-grep TS path.

### Files forbidden (Arc 2)

- No `rules/move_specifier.typescript.yml`; no tree-sitter-typescript matching
  for TS. `rules/move_specifier.yml` stays the prolog arm (`0_move.rs:316-325`).
- `src/lang/1_ast_rule.rs` (untouched; ast-grep stays prolog-only for the move).

### Tests to add (Arc 2)

- `module_specifiers_emits_dynamic_import_and_require_rows` (tests/1_move.rs or a
  ts.rs unit test): a fixture with `import './a'`, `import {x} from './b'`,
  `export {y} from './c'`, `import('./d')`, `require('./e')`; assert a
  `Specifier` row per module path with the right `SpecifierKind` and a nonzero
  byte span. Breaks if the new visitor arms miss a node or produce a wrong kind.
- `ts_move_rewrites_via_oxc_spans_not_ast_grep` (tests/1_move.rs): move a TS
  file; assert the importer's rewrite lands at the oxc span (byte-exact) with no
  tree-sitter rule in the run. Breaks if the move falls back to ast-grep.
- `ts_require_and_dynamic_import_reaim` (tests/1_move.rs): `import('./b')` and
  `require('./b')` in an importer both re-aim after `b` moves. Breaks if the new
  rows are not fed to the drain.
- `ts_plain_string_literal_is_untouched` (tests/1_move.rs): `const s = "hello"`;
  `readFile("./b")` with `b` NOT moved stays byte-identical. Breaks if the move
  rewrites a non-source string.

### Gate command (Arc 2)

```sh
cargo test --release --features cli --test 1_move
```

### Risk table (Arc 2)

| risk | citation that raises it | mitigation |
|---|---|---|
| `callee_name()` on `require` returns the right ident | ast_impl/js.rs:751 (`callee_name`) | gate a `require('./x')` fixture; if the helper differs, match `call.callee` against an identifier named `require` |
| `import` may be `Expression::ImportExpression` or a bare `CallExpression` in some versions | js.rs:124-125 vs 613-625 | match both arms; the ImportExpression arm is the spec'd one |
| `require` with a non-literal first arg (a variable) | js.rs:690-696 Argument enum | only `Argument::StringLiteral` becomes a row; a variable arg is skipped |
| the span is the oxc byte span, soopy needs the same | ts.rs:45-51 (`to_span`) | feed `row.span` directly into `BoundEdit`; no re-encode |

## Arc 3: TS path resolution

### Decision: resolution is bought, `oxc_resolver` is TAKEN for v1

`oxc_resolver` 11.24.2 (vendored under
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/oxc_resolver-11.24.2`)
resolves the module question with the tsconfig-paths and ESM/CJS algorithm the
move needs. It is the buy. The hand-rolled relative probe is REJECTED: it
would re-derive extension, index, and extension-alias logic that the library
already implements and tests (its own test suite is the ESM/CJS + tsconfig
ports, README "Tests"), and v6 rule 1 requires a written excuse for hand-rolling
a common-shaped problem (`v6/README.md`). No such excuse holds here.

| candidate | what it resolves | deps | verdict |
|---|---|---|---|
| `oxc_resolver` 11.24.2 | ESM/CJS, extensionless, `index`, `.js`->`.ts` (extension_alias), tsconfig `paths`/`baseUrl` incl. extends/references, package.json `exports`/`main`, node_modules | oxc project family, serde/serde_json (already in the crate), `FileSystem` trait for in-memory | TAKEN for v1 |
| `swc` resolver | module resolution inside its transform pipeline, no standalone ergonomic crate | `swc_ecma_*` | REJECTED: no clean standalone crate |
| `tsconfig` / `tsconfig-resolver` crates | parse tsconfig only, no resolution | serde_json | REJECTED: partial, resolution absent |
| hand-rolled relative probe | relative extensionless / `index` / `.js`->`.ts` | none | REJECTED: re-derives the library's algorithm; v6 rule 1 |

### The `ResolveOptions` the move sets

`ResolverGeneric::new` takes a `ResolveOptions` (`lib.rs:162-168`, `new` /
`new_with_file_system` at `lib.rs:162,175`). The move configures it for the
ESM-style TS corpus, per the option struct (`src/options.rs`):

| option | value | citation |
|---|---|---|
| `extensions` | `[".ts", ".tsx", ".mts", ".cts", ".d.ts", ".js", ".jsx", ".mjs", ".cjs"]` | `options.rs:93-106` (leading dots required); order follows the repo's `source_type_for` table (`ts.rs:89-99`) |
| `extension_alias` | `[(".js", [".ts", ".tsx", ".d.ts"])]` | `options.rs:83-91` (the `.js` written for `.ts` case, dominant in grapht) |
| `main_files` | `["index"]` | `options.rs:115-117` (index resolution) |
| `condition_names` | `["node", "import"]` | `options.rs:48-56` (ESM condition set; the README "ESM" example) |
| `main_fields` | `["module", "main"]` | `options.rs:110-113` (module-first for ESM-style packages) |
| `exports_fields` | default `[["exports"]]` | `options.rs:70-72` |
| `tsconfig` | `TsconfigDiscovery::Auto` for `resolve_file`; else `Manual(TsconfigOptions)` | `options.rs:527-549`; `Auto` only works through `resolve_file` (`lib.rs:250-252`) |
| `symlinks` | default `true` | `options.rs:171-186` |

The `tsconfig` discovery: `TsconfigDiscovery::Auto` needs `resolve_file`
(`lib.rs:250-252`), which discovers the tsconfig by walking parents from the
file and honors `paths`/`baseUrl`/`references`/`extends` (`tsconfig.rs:853-868`,
README "tsconfig-paths-webpack-plugin"). The move calls `resolve_file` with the
importing file's path, so a `.tsx` and a `.ts` under the same project resolve
against the same discovered tsconfig.

### The resolution call

For an importer at path `P` writing spec `S`:

```rust
let resolver = Resolver::new(options);                 // lib.rs:162, one per run
match resolver.resolve_file(&P, &S) {                  // lib.rs:258-268 (resolve_file)
    Ok(resolution) => resolution.path(),               // resolution.rs:64-68, strips query/fragment
    Err(_) => continue,                                // bare package / alias / missing: not a move target
}
```

A bare specifier (`zod`) that resolves into `node_modules` returns a path
outside the root, so it is a move target only if it lands under the root (an
internal alias). The result path, compared against the moved `old` paths, says
whether to rewrite. `resolve_file` takes a file path (not a directory), so the
`TsconfigDiscovery::Auto` branch works (`lib.rs:258-268`).

### The replacement (re-aim)

For a spec `S` (quote `q`, value `v`) that resolves to moved file `M` now at `M'`:

```
relative = relative_from(dir(P), M')        // 0_move.rs:544-558, existing helper
replacement = requote(q, with_ext_style(relative, v))
```

`with_ext_style` maps the relative path's extension to match the original
spec's extension style, so an ESM-style `.js` import stays `.js`
(`./0_benchProtocol.js` -> `./0_bench/0_protocol.js`), a `.ts` import stays
`.ts`, and an extensionless import stays extensionless. The quote char `q` is
preserved. For a moved file's own internal relative specs, the re-aim runs from
the moved file's new directory (the `is_old` branch, `0_move.rs:180-215`).

### Type signatures

```rust
// v6/sprefa-extract/src/0_move.rs
/// One resolver for the whole run, built once with the TS ResolveOptions.
struct TsResolver { inner: oxc_resolver::Resolver }

/// Resolve a TS specifier written by `file`; None if it resolves outside the
/// root or fails (bare package, alias, missing).
fn resolve_ts(&self, file: &Path, raw: &str) -> Option<PathBuf>;

/// The replacement spec preserving quote and extension style.
fn spec_text_ts(from_dir: &Path, target: &Path, original: &str) -> String;
```

Body (pseudo-code):

```rust
fn resolve_ts(&self, file, raw) -> Option<PathBuf> {
    let (_, value) = unquote_ts(raw);              // strip ' or "
    let res = self.inner.resolve_file(file, value).ok()?;
    let path = res.path();                         // resolution.rs:64-68
    within_root(&root, path).ok()?;                // only internal paths are move targets
    Some(path.to_path_buf())
}

fn spec_text_ts(from_dir, target, original) -> String {
    let (q, value) = unquote_ts(original);
    let rel = relative_from(from_dir, target);     // 0_move.rs:544-558
    let rel = with_ext_style(&rel, value);         // .js stays .js, etc.
    requote_ts(q, &rel)                            // keep ' or "
}
```

### Instance lifetimes

`TsResolver` is built ONCE per run (one `Resolver` holding the `ResolveOptions`,
`lib.rs:162`) and shared by reference across the parallel prescan
(`0_move.rs:141-155`) and the sequential merge (`0_move.rs:139-220`). `Resolver`
is `Send + Sync` (it caches internally), so the rayon pool can share it.
`Resolution` values are transient per specifier; only `path()` is kept.

### Storage layout

Input: the corpus file paths (read for parse) and the moved `old`/`new` paths.
Read sequence: spec -> `resolve_file` (filesystem probe + tsconfig discovery) ->
target; replacement = pure path math. Uniqueness: one rewrite per raw spec text
per file (`rewrites: BTreeMap<rel, BTreeMap<raw, replacement>>`, `0_move.rs:164`).

### Files touched (Arc 3)

- `v6/sprefa-extract/Cargo.toml`: add `oxc_resolver = "11.24"` (vendored
  11.24.2; oxc family already in the crate, `Cargo.toml:40-45`).
- `v6/sprefa-extract/src/0_move.rs`: `TsResolver`, `resolve_ts`, `spec_text_ts`,
  `unquote_ts`, `requote_ts`, `with_ext_style`.

### Files forbidden (Arc 3)

- No hand-rolled extension/index/extension-alias probe (that logic is the
  library's). The `relative_from`/`normalize` path helpers stay (they are
  soopy-relative path math, `0_move.rs:544-571`, not resolution).
- `src/lang/ts.rs` resolution logic (the oxc extractor stays extraction-only).

### Tests to add (Arc 3)

- `ts_fixture_relative_index_extensionless_and_js_for_ts` (tests/1_move.rs, the
  issue's receipt): a fixture repo under `tests/fixtures/ts_move/` with four
  cases: relative with extension (`from './b.ts'`), extensionless
  (`from './b'` -> `./b.ts`), `index` (`from './dir'` -> `./dir/index.ts`),
  and ESM-style (`from './b.js'` -> `./b.ts`). Move `b.ts` into a subdir and
  assert every importer re-aims and every resolve lands. Breaks if the options
  or the re-aim extension mapping is wrong.
- `ts_moved_file_reaims_its_own_relative_imports` (tests/1_move.rs): the moved
  file imports a sibling that also moves; both land correctly from the new dir.
  Breaks if the `is_old` new-dir re-aim is wrong.
- `ts_bare_specifier_is_untouched` (tests/1_move.rs): `import "zod"` stays
  byte-identical (resolves to `node_modules`, outside the root). Breaks if a
  bare spec is treated as a move target.
- `ts_paths_alias_in_tsconfig_resolves` (tests/1_move.rs): a fixture with a
  `tsconfig.json` declaring `compilerOptions.paths` (`@/*` -> `./src/*`); a
  moved file referenced through the alias re-aims. Breaks if `TsconfigDiscovery`
  is not wired or the alias is missed.

### Gate command (Arc 3)

```sh
cargo test --release --features cli --test 1_move
# plus byte-identical `tsc --noEmit` / `node --check` on the fixture before and
# after (the issue's receipt) where the fixture has a tsconfig / is runnable.
```

### Risk table (Arc 3)

| risk | citation that raises it | mitigation |
|---|---|---|
| `.js` written for `.ts` must map back to the `.ts` source | `options.rs:83-91` extension_alias | `extension_alias: [(".js", [".ts",".tsx",".d.ts"])]`; gate the ESM fixture |
| tsconfig `paths`/`baseUrl` need the right discovery | `options.rs:527-549`, `lib.rs:250-252` | use `resolve_file` so `TsconfigDiscovery::Auto` walks parents; gate the alias fixture |
| a bare specifier resolving to `node_modules` | `options.rs:110-117` main/module fields | `within_root` gate drops it; test `import "zod"` untouched |
| one resolver per run vs per file | `lib.rs:162` (Resolver holds options + cache) | build once in `Plan::build`, share by reference; `Send + Sync` |
| an alias into the source tree is a real move target | tsconfig.rs:853-868 (paths resolve) | `within_root` accepts it when the path lands inside the root; gate the alias fixture |

## Arc 4: batch `--list <tsv>`

### Decision

The current CLI moves one file: `extract move <old> <new>` (`0_move.rs:43-62`).
The batch form adds `--list <tsv>`, where each line is `old<TAB>new` and every
row is one move. The plan generalizes `Plan` from one `(old, new)` to a set of
moves `old -> new`, then stages the whole batch.

soopy accepts ONE operation per source file (`0_move.rs:93-94`, cited
`_7d_mutation_plan.rs` `insert_non_replace`), so the batch cannot put a file's
`Replace` and its `Move` in one `StageRequest`. The batch therefore collapses
into the same stage shape as the single move:

- stage 1: one soopy `Replace` per distinct importer, across all moves, in ONE
  `StageRequest` (`drain::replace_action`, `drain.rs:84-97`);
- stage 2: one soopy `Move` per moved file, in ONE `StageRequest`
  (`0_move.rs:250`).

This satisfies "every move and every importer rewrite in ONE soopy StageRequest"
at the stage granularity the one-op-per-file rule forces.

### Expected hashes when two moves touch the same importer

A file `X` importing both moved `A` and moved `B` contributes both edits to one
`Replace` on `X`; `by_raw` is a `BTreeMap<raw, replacement>` (`0_move.rs:164`),
so two raws in one file are two edits in one `Replace`, and the `expected` hash
is computed once against `X`'s pre-move bytes. `bind_action` re-reads each
`expected` at the root that actually stages it (`drain.rs:141-171`), so the hash
matches the on-disk bytes at stage time regardless of ordering (arc B deviation
2, PR #473). Within stage 1, edits never re-read each other (they are one
Replace); between stages, stage 1 commits before stage 2's moves read their
`expected` (the moved file's post-edit bytes), which is exactly the single-move
contract.

### Ordering and validation

- `--list` rows are processed in file order; the corpus merge stays sequential
  over path order (`0_move.rs:139`).
- Validation before staging: every `old` must exist; every `new` must not
  exist; no `new` may equal any `old` (a move cannot overwrite a still-present
  source of another move); no two `new` equal. A violation is a hard error
  before any stage is built.
- A file that is both an importer and a moved file lands in stage 1 (its
  internal re-aims) and stage 2 (its Move), per the one-op-per-file split.
- Dry run prints one plan: the existing `Mirror` (`0_move.rs:723-773`) copies
  every file any stage touches, and every stage commits against it with
  `Durability::DryRun` (`0_move.rs:82-86`), so a batch dry run shows the full
  preview set and leaves the tree untouched.

### Type signatures

```rust
// v6/sprefa-extract/src/0_move.rs
/// One move in a batch: source rel -> destination rel (root-relative).
struct MoveSpec { old_rel: String, new_rel: String }

/// Read a `--list` tsv (old<TAB>new per line) into validated MoveSpecs.
fn read_move_list(path: &Path) -> Result<Vec<MoveSpec>, String>;

// Plan::build gains a move set instead of one (old, new):
//   moves: Vec<(old_rel, new_rel)>
// the `is_old` test becomes `moves.contains_key(rel)`, and the "target == old"
// test becomes `moves.contains_key(target_rel)`.
```

Body (pseudo-code):

```rust
fn read_move_list(path) -> Result<Vec<MoveSpec>> {
    // read lines; split_once('\t'); non-empty old/new; no duplicate old;
    // every old under root; no new exists; no new == any old.
}

// in Plan::build:
//   moved: BTreeMap<String,String>  // old_rel -> new_rel (storage: sorted)
//   for each file, each spec:
//     if file is being moved: re-aim all relative specs from new_dir(file)
//     else if resolve(spec) is a moved old: re-aim to that move's new
//   stage 1 = all Replace actions; stage 2 = all Move actions
```

### Instance lifetimes

`MoveSpec`/`moved` map live for `Plan::build`; `Plan.stages` owns the built
`Vec<Vec<soopy::SourceAction>>` (`0_move.rs:93-101`). The `Mirror` owns the temp
root for a dry run, dropped at end (`0_move.rs:769-773`).

### Storage layout

Read: the tsv (once), the corpus, the moved files. Write: stage 1 `Replace`s
then stage 2 `Move`s through soopy (`drain::stage_edits` / `stage_and_commit`,
`0_move.rs:649-688`). Uniqueness: one `Replace` per file (soopy rule); one
`Move` per moved file; edits deduped by `(start, end)` (`drain.rs:84-97`).

### Files touched (Arc 4)

- `v6/sprefa-extract/src/0_move.rs`: `MoveSpec`, `read_move_list`, `--list` flag
  in `MoveCli` (`0_move.rs:43-62`), generalize `Plan::build`.
- `v6/sprefa-extract/src/bin/extract.rs`: the `move` dispatch (`extract.rs:289-294`)
  already forwards args; `--list` needs no new dispatch branch.

### Files forbidden (Arc 4)

- The soopy crate (one-op-per-file and `StageRequest` are already sufficient).
- `src/lang/**` beyond `0_move.rs` and the `ts.rs`/`types.rs` edits of arc 2.

### Tests to add (Arc 4)

- `batch_moves_two_files_and_rewrites_a_shared_importer_once` (tests/1_move.rs):
  `--list` with `a.ts<TAB>x/a.ts` and `b.ts<TAB>x/b.ts`; an importer `index.ts`
  naming both; assert one `replace` row for `index.ts` with two edits and two
  `move` rows. Breaks if the one-op-per-file grouping or the shared importer
  `expected` hash is wrong.
- `batch_rejects_colliding_destinations` (tests/1_move.rs): a `new` equal to
  another row's `old`, and a duplicate destination; assert a hard error before
  staging. Breaks if validation is missing.
- `batch_dry_run_prints_one_plan_and_writes_nothing` (tests/1_move.rs): the
  full preview set on a batch, `git status` clean. Breaks if a dry run touches
  the tree.
- `batch_move_of_a_moved_importer` (tests/1_move.rs): a moved file that itself
  imports another moved file; its `Replace` and `Move` split across the two
  stages with correct `expected` on each.

### Gate command (Arc 4)

```sh
cargo test --release --features cli --test 1_move
```

### Risk table (Arc 4)

| risk | citation that raises it | mitigation |
|---|---|---|
| two moves edit the same importer; their edits must land in one Replace | soopy one-op-per-file (`0_move.rs:93-94`) | `by_raw` BTreeMap per file holds both; `expected` read once at stage time (`drain.rs:141-171`) |
| a moved file is also an importer | `_7d_mutation_plan.rs` one-op-per-file | Replace (stage 1) and Move (stage 2) are separate stages, the single-move contract |
| a destination collides with a source or another destination | validation need | `read_move_list` rejects before any stage builds |
| dry run must not touch the tree | `Mirror` (`0_move.rs:723-773`) | every stage commits into the temp mirror with `Durability::DryRun` |

## Sequencing

Arc 1 -> Arc 2 -> Arc 3 -> Arc 4. Arc 1 is the walker every later arc consumes;
Arc 2 extends the oxc visitor with the specifier rows (dynamic import/require)
and maps their spans to rewrites; Arc 3 buys `oxc_resolver` to resolve and
re-aims; Arc 4 lifts the single-move plan to a batch. Each arc ends green on
its gate.

## Verification

- Single-move TS: `cargo test --release --features cli --test 1_move`, plus
  `tests/fixtures/ts_move/` byte-identical `tsc --noEmit` / `node --check`
  before and after where the fixture is runnable.
- Batch: `cargo test --release --features cli --test 1_move` with the batch
  cases above.
- Acceptance corpus, dry run only from this plan lane: run the batch `--list`
  of the 38 grapht moves (`.boop-worktrees/feature/generic-graph-rxjs-renderers/
  issues/grapht-source-layout/item.md`, the target map) on
  `~/projects/hafley-rxjs` and confirm the printed plan re-aims
  `./0_benchProtocol.js` -> `./0_bench/0_protocol.js` style imports and the
  barrels, without committing. The config updates in that item (package.json
  `browser` export, adapters) are out of scope for the move verb; they are
  manual.

## Staffing

- Implements in `v6/sprefa-extract` on a worktree off this branch. Base SHA:
  `4e478c60725d3d4cf8d86f2674af4c44b5723a81` (merged ff-only into this branch;
  PRs #472 #473 #475 already merged).
- No soopy changes. One new dependency, `oxc_resolver` (vendored 11.24.2, oxc
  family already in the crate, `Cargo.toml:40-45`). No tree-sitter TS rule; no
  `rules/move_specifier.typescript.yml`.
- Gate per arc as above; CI = build + `cargo test --features cli`. No Rust
  written in this plan lane; signatures live in this doc.

## Build-vs-buy paragraph

The one new library question is path resolution, and it is bought: `oxc_resolver`
11.24.2 (vendored under the registry) is taken for v1 with the ESM-style
`ResolveOptions` (`extension_alias` for `.js`->`.ts`, `main_files` for `index`,
tsconfig `paths`/`baseUrl`/extends/references via `TsconfigDiscovery`, ESM
`condition_names`) built once per run. It re-uses the oxc project family the
crate already links (`Cargo.toml:40-45`). The hand-rolled relative probe is
rejected: it would re-derive the library's tested algorithm and v6 rule 1
(`v6/README.md`) requires a written excuse for hand-rolling a common-shaped
problem, which does not exist. `swc`'s resolver is rejected because it is not a
standalone crate, and the `tsconfig` crates are rejected because they parse
config without resolving. The oxc parser (already in the crate) is the source of
the move's specifier rows and spans, so nothing in the TS arm is reinvented.
