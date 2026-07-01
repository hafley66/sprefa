# North star: types, modules, parsers, and the LSP/info channel

Date: 2026-06-18. Status: PLAN. Companion to
`chat_log/20260618.1.sprefa-v5-types-modules-parsers-design.md`.

Read that session doc for the full reasoning; this file is the binding spec
and sequencing for the work it identified.

## The commitment

Three things, in priority order:

1. **The data model does not change.** `Type { Text Int Path File Dir Repo Rev }`
   and `Value { Text(String) Int(i64) }` stay. No composites. Wide arity is the
   accepted cost of the SQL-IR commitment (plan 2026-06-01:11-17). The
   arity-mitigation layer is *authoring*, not storage.

2. **`BodyItem` is small by design, not frozen.** Six source ops today
   (`scan`/`match`/`ast`/`sg`/`json`/`cmd`) plus `comment`. New formats
   compose from these in dl; new grammars slot into the `ts_lang`/`sg_lang`
   match arms (engine.rs:3488, sg.rs:5) with one Cargo dep each. New op
   *variants* are on the table when a real program wants a query family the
   current four cannot reach (regex / S-expr / structural / pattern). The
   v3/v4 wall was about preventing the 60k-LOC bespoke runtime drift, not
   about freezing the surface. Candidates on deck: `rs:` symbol scheme
   (desc.rs:69-72 reserved), CSS/XPath selectors, native go-to-def /
   find-references (Phase F). Add them when the program demand is real;
   don't gate them behind a "two programs prove pain" threshold that reads
   as a permanent no.

3. **Types are rels.** The CodeQL class pattern (characteristic predicate +
   member predicates + supertypes) is emulated by one base rel + one derived
   rel per member, joined by entity key. No class syntax is added. Wide rels
   cohere via `use` (file boundary) and `def` (parameterized templates).

The north star: assert cross-format facts about types in dl, surface them via
LSP hover/info and a UI like anim, and let the agent layer route
progressive-analysis questions (missing repos, candidate clone URLs) through
derived rels. None of this requires composite types, fallible imports, or new
ops by default.

## What this enables later

| capability | unlocked by |
|---|---|
| `std/callgraph.dl`, `std/parsers/{openapi,k8s,docker,helm,cargo}.dl` | Phase A1 (`use`) |
| Arity-reuse via qualified call graph as a one-liner | Phase A2 (`def`) |
| Progressive multi-repo without bail on missing clones | Phase B (`allow_missing`) |
| LSP hover on a type showing comments/refs/signature | Phase C (`info` rel + `locate_at`) |
| `missing_repo(slug, url)` for agent-driven clone UX | Phase B |
| Cross-format node editor (anim) over real call refs | Phases A1 + D + E |
| One built-in call graph instead of 6 reinvented per program | Phase D (CALL_RELS) |
| Unified symbol identity across modules/types/calls | Phase D |
| Programmable go-to-def / find-references reaching type_entity + call_def | Phase E |
| Full SCIP signature types layered on diet `type_sig` | Phase F (on demand) |
| Native CSS/XPath selectors + `rs:` symbol literals | Phase G (on demand) |

## Phase ordering

**A1 -> B -> A2 -> C**, with D parallel to A1/B, E after D, F and G on demand.

Rationale: A1 (use), B (allow_missing), and D (CALL_RELS) are independent
and can land in parallel. A2 (def) needs A1. C (LSP info) and E
(programmable go-to-def) both benefit from A1 + D landing first so the new
rels and symbols are query/hover targets. F (full SCIP) and G (CSS/XPath +
rs:) are on-demand, not sequenced.

---

# Phase A1 - `use` only (module inclusion)

Smallest viable module system. File inclusion with canonical-path dedup.
Unlocks `v5/std/*.dl`. Backward compatible: core `.dl` parses identically.

## Surface

```
use "std/parsers/openapi.dl".       # splice that file's items here, dedup by canonical path
use "std/callgraph.dl".
```

No `def`, no `lint`. Those land in A2.

## Type signatures (new `src/frontend.rs`)

```rust
enum SurfaceItem {
    Core(ast::Item),                 // pass-through: rel/rule/query/anchor/brand/gen
    Use(Import),                     // `use "path"`.
}
struct Import { path: String }       // resolved against include roots

// Parse the EXTENDED surface. Falls back to the existing core parser per item.
fn parse_surface(toks: Vec<lex::Tok>) -> Result<Vec<SurfaceItem>>;
//   pseudo:
//   - same loop as parse::parse, but item dispatch also matches "use"
//   - "use" -> Use(Import { next string literal })
//   - else -> Core(existing self.item())

// Expand the surface into the frozen core IR. The ONLY place sugar disappears.
fn expand(items: Vec<SurfaceItem>, loader: &mut ModuleLoader) -> Result<ast::Program>;
//   pseudo:
//   1. resolve every Use: loader.load(path) -> Vec<SurfaceItem>, recurse,
//      dedup by canonical path (diamond imports load once)
//   2. splice Core items in file order
//   3. re-rels with identical name + identical cols dedup; conflicting cols hard-error
//      with both source paths

struct ModuleLoader { roots: Vec<PathBuf>, loaded: HashMap<PathBuf, Vec<SurfaceItem>> }
fn ModuleLoader::load(&mut self, path: &str) -> Result<Vec<SurfaceItem>>;
//   resolve `path` against roots (std-lib dir bundled next to the binary, then
//   the program file's dir); read; lex; parse_surface; cache by canonical path.
```

## Instance lifetimes

- `ModuleLoader`: lives for ONE compile (`parse_surface` -> `expand` ->
  `Program`), then dropped. Pure, no persistence.
- After `expand`, the resulting `Program` flows into the unchanged
  `engine::tick` path. The engine never sees sugar.

## Storage / reads / writes / uniqueness

- runtime: NONE. Frontend is compile-time only.
- reads: module files from include roots (filesystem, once per path per
  compile, cached in `ModuleLoader.loaded`).
- writes: none.
- uniqueness: rel decls dedup by name + cols across the merge. A name
  collision with conflicting cols is a hard error naming both source paths.
  Rules and queries splice verbatim.

## Wiring

`run_file` / `run_check` / `run_lsp` / `run_watch` / `run_changed` swap one
line each: `parse::parse(lex(src))` becomes
`expand(parse_surface(lex(src)), &mut loader)`. Discovery mode
(`<root>/.dl/*.dl`) merges files before the swap, same as today.

## Phase A1 gate

`use "std/parsers/openapi.dl"` from any program, in any repo, with no
copy-paste. The 6 callgraph examples migrate to `use "std/callgraph.dl"` plus
a per-program invocation rule. Existing examples still parse and run green.

---

# Phase B - `allow_missing` config flag + missing_repo rel

Progressive analysis without fallible imports. The missing-codebase pain is
solved at the source-resolution seam, where it actually bites.

## Surface

```toml
# .config/sprefa.toml or ~/.config/sprefa/config.toml
[[repos]]
slug = "dep-a"
root = "/abs/path/dep-a"
url = "https://github.com/org/dep-a"
allow_missing = true        # new field
```

```dl
% A program-level convention (no new built-in). The engine surfaces the data;
% an agent or UI reads this query.
rel missing_repo(slug: text, hinted_url: text).
missing_repo(S, U) <-
  referenced_repo(S),          % derived from scan(...) slugs the program mentions
  !repo(S, _, _),              % the built-in repo rel
  candidate_url(S, U).         % from manifest/lockfile crawlers
```

## Type signatures

```rust
// config.rs
pub struct RepoConfig {
    pub slug: String,
    pub root: PathBuf,
    pub url: Option<String>,
    pub allow_missing: bool,        // NEW; default false (preserves today's bail)
}

// engine.rs resolve_repo, the only behavior change
fn resolve_repo(&self, repo: &str) -> Result<(String, PathBuf)>;
//   pseudo:
//   - prior branches (".", config slug w/ clone, existing path) unchanged
//   - new final branch: if repo matches a config slug whose allow_missing=true
//     AND root does not exist AND no url configured or clone failed:
//       emit "[missing] repo {slug} (allow_missing); scan returns empty"
//       return Ok((slug, root_even_though_missing))   // <-- key change
//   - else: bail as today
```

The `repo(S, _, _)` built-in still does not list missing repos; the
`referenced_repo` derived rel is what the program authors to surface intent.

## Instance lifetimes

- `RepoConfig.allow_missing`: loaded each tick from config (already the case
  for the other fields). No new state on `Engine`.

## Storage / reads / writes / uniqueness

- runtime: NONE. The flag changes control flow in `resolve_repo` only.
- reads: scan over a missing repo returns zero rows; engine proceeds.
- writes: none beyond the existing `_file` / `repo` / `rev` tables, which
  simply don't get rows for the missing slug.
- uniqueness: a missing slug still has a stable identity for the duration of
  the tick. Re-tick after `git clone` populates it normally.

## Phase B gate

A program that `scan("dep-a", ...)` against an `allow_missing = true` repo
whose root does not exist runs to completion: zero rows from that scan, no
bail, a stderr line noting the miss. After cloning the repo, the next tick
sees the data. `missing_repo(S, U)` derived rel gives the agent layer what
it needs to drive the clone prompt.

---

# Phase A2 - `def` templates (arity-reuse layer)

Parameterized rule inlining. The leverage that lets wide rels cohere across
files. Skipped until A1 proves the module story out.

## Surface

```
def qualified_call(caller, callee) {
  fndef(caller, p, s, e),
  callsite(callee, p, l),
  s <= l, l <= e
}

calls(caller, callee) <- qualified_call(caller, callee).
```

A `def` referenced as a body atom is INLINED: clone body, substitute params,
alpha-rename internal vars. Emits one Rule per instantiation site.

## Type signatures

```rust
enum SurfaceItem {
    Core(ast::Item),
    Use(Import),
    Def(RuleTemplate),               // NEW in A2
}
struct RuleTemplate {
    name: String,
    params: Vec<String>,
    body: Vec<ast::BodyItem>,
    head_rel: String,
    head_terms: Vec<ast::Term>,
}

fn expand(items: Vec<SurfaceItem>, loader: &mut ModuleLoader) -> Result<ast::Program>;
//   pseudo (additions to the A1 expand):
//   1. collect Defs into a template table: name -> RuleTemplate
//   2. detect cycles (template calls template): reject for now, inline-only
//   3. a body atom `name(args)` matching a template INLINES:
//      a. clone the template body
//      b. substitute params: Var(p) -> arg Term
//      c. alpha-rename every non-param internal var to `__<tplname><n>_<v>`
//         so two instantiations never capture each other
//      d. emit one Rule per call site with the surrounding rule's head
//   4. assert all rel names referenced exist post-expansion; bail with a
//      surface-level span on unknown rel / arity / unknown template
```

## Instance lifetimes

- Same as A1: template table lives for one compile, dropped after `expand`.

## Storage / reads / writes / uniqueness

- runtime: NONE. Frontend is compile-time only.
- reads: module files; same as A1.
- writes: one Rule per call site in the expanded program.
- uniqueness: alpha-rename namespaces internal vars by
  `__<tplname><instance>_...`. Two `def`s of the same name across imports is
  a hard error naming both source paths. A `def` name shadowing a rel is a
  hard error.

## Phase A2 gate

The qualified-call-graph example (today's `examples/callgraph-typed.dl`)
becomes `use "std/callgraph.dl"` plus a one-line `def` for the
project-specific entry resolution. Arity reuse measured: the same 5-atom
fndef/callsite/range-join pattern stops being copy-pasted.

---

# Phase C - LSP `info` rel + hover handler

Extends the LSP surface beyond `diag`. The third output channel that lets a
type carry comments, faqs, references, and signature facts into the editor.

## Surface

```dl
% program-defined; no new built-in. Same shape freedom as diag.
rel info(path: file, line: int, msg: text).

info(p, l, "FAQ: ${q}") <-
  type_entity(s, name, _, _, p, l),
  comment(p, "WORK", /FAQ: $name$ ($q)/, _, _, q).

info(p, l, "spec op: ${op}") <-
  type_entity(_, name, _, _, p, l),
  spec_op(op), op = name.
```

Hovering the identifier at (p, l) returns every `info` row at that coordinate.

## Type signatures

```rust
// engine.rs: join _where_bytes with a program rel at a byte offset
pub fn locate_at(&self, path: &str, byte: u32) -> Result<Vec<LocatedRow>>;
//   pseudo:
//   - SELECT cols FROM <rel> WHERE <path-col> = ? AND <line-col> matches the
//     line under byte, for every rel the program tagged as "locate-able"
//     (convention: has path + line cols). Tagged = mentioned in a hover rel
//     registry, OR scan all rels with path+line cols and union.
//   - returns LocatedRow { rel, line, cols: Vec<(name, value)> }

// lsp.rs: new handler alongside handle_definition / handle_references
fn handle_hover(eng: &Engine, root: &Path, req: &Request) -> Response;
//   pseudo:
//   - resolve byte offset from Position (same util as resolve_span)
//   - eng.locate_at(rel_path, byte)
//   - render rows as markdown; one section per contributing rel
//   - return Hover { contents: MarkupContent }
```

Capabilities to add at lsp.rs:57-67: `hoverProvider: true`. No other capability
changes; def/refs stay spine-only (consistent with today's spine-located
identity model).

## Instance lifetimes

- `Engine::locate_at` is a stateless query, same lifetime as `diags(None)`.
- No new long-lived state on the engine.

## Storage / reads / writes / uniqueness

- no new durable storage.
- reads: the program rels (existing tables); `_where_bytes` only when the
  hover needs byte-precise span matching.
- writes: none.
- uniqueness: hover output is many-rows-to-one-coordinate; the renderer
  concatenates. A program with two `info` rules at the same (p, l) shows both.

## Phase C gate

Hover an identifier in a TS file. If the program has `info(p, l, msg)` rows
at that coordinate, the hover shows them as markdown. A program with no
`info` rel produces empty hover (def/refs still work). No regression on
existing diag publishing.

---

# Phase D - Diet SCIP completion: CALL_RELS + unified symbol identity

The diet-SCIP layer is already ~2/3 built. `TYPE_RELS` and `MODULE_RELS`
cover the type-graph and module-graph shapes that SCIP carries, in a
language-agnostic schema (`EntityKind`, shared edge-kind vocab, the
`file::kind::name` symbol convention). The gap:

| SCIP concept | v5 today |
|---|---|
| document symbols | `type_entity` (have it) |
| signatures | `type_sig` diet (have it) |
| relationship edges | `type_edge` / `type_link` (have it) |
| module-level occurrences | `module_edge` / `module_import` (have it) |
| **call edges** | **no built-in peer; 6 examples reinvent per program** |
| **symbol identity across graphs** | **fragmented: `file::kind::name` (types) vs bare/qualified (calls) vs interned strings (spine)** |

Phase D closes both gaps.

## Surface

New `CALL_RELS` built-in family, mirroring `TYPE_RELS`:

```rust
// engine.rs peer to TYPE_RELS / MODULE_RELS
const CALL_RELS: [&str; 4] = ["call_def", "call_site", "call_edge", "call_edge_rev"];

fn call_rel_decls() -> Vec<RelDecl> {
    vec![
        // function/method definition: symbol, kind (function/method/free), file, span
        RelDecl { name: "call_def".into(), cols: vec![
            c("sym", Type::Text), c("kind", Type::Text),
            c("file", Type::Path), c("line", Type::Int), c("end", Type::Int)] },
        // call site: caller symbol, callee symbol (resolved or bare), file, line
        RelDecl { name: "call_site".into(), cols: vec![
            c("caller", Type::Text), c("callee", Type::Text),
            c("file", Type::Path), c("line", Type::Int)] },
        // resolved call graph (callee resolved to def sym when unique); the
        // closure edge
        RelDecl { name: "call_edge".into(), cols: vec![
            c("caller", Type::Text), c("callee", Type::Text), c("kind", Type::Text)] },
        // rev-aware source of truth, same split as type_edge_rev
        RelDecl { name: "call_edge_rev".into(), cols: vec![
            c("caller", Type::Text), c("callee", Type::Text),
            c("kind", Type::Text), c("rev", Type::Text)] },
    ]
}
```

The symbol convention is unified: every call graph node is a
`file::kind::name` symbol (the same shape `type_entity` already uses), so
`closure(call_edge)` reaches types via shared symbols and a query like
"every fn that calls a method on a type in this module" is a plain join.

The 6 example tiers (`examples/callgraph-*.dl`) collapse into one
`v5/std/callgraph.dl` per language: a parameterized `def` (Phase A2) over
the new built-ins, with the language-specific extractor (regex/ast/sg) the
only thing that varies.

## Type signatures

```rust
// typegraph.rs: extract call facts alongside type facts. One parse already
// feeds both today (typegraph.rs:140-148 for Rust); add call extraction to
// the same pass.
pub struct CallFacts {
    pub defs: Vec<CallDef>,
    pub sites: Vec<CallSite>,
}
pub struct CallDef {
    pub sym: String,         // file::function::name (free) or file::method::Parent.name
    pub kind: CallKind,      // Free | Method | Closure
    pub file: String,
    pub line: u32,
    pub end: u32,            // body span end (1-based line), for callsite containment
}
pub struct CallSite {
    pub caller_sym: Option<String>,   // resolved by span containment in a second pass
    pub callee: String,                // bare or qualified; resolved to def sym when unique
    pub file: String,
    pub line: u32,
}

// add to TypeLang trait (typegraph.rs:120)
fn extract_calls(&self, file: &str, content: &str) -> CallFacts;

// engine.rs peer to refresh_type_rels
fn refresh_call_rels(&self) -> Result<()>;
//   pseudo:
//   - par_iter over .rs/.kt/.ts/.tsx files in _file (same shape as refresh_type_rels)
//   - extract per-language via the registry; collect defs and sites
//   - second pass: assign each callsite to its enclosing def by span containment
//     (s <= line <= e), the existing fndef/callsite idiom
//   - resolve callee bare names to def syms via the same by_name/SPIP override
//     path used for type_link (engine.rs:2207-2218)
//   - emit call_def, call_site, call_edge_rev, call_edge (deduped)
```

## Instance lifetimes

- Same as `refresh_type_rels`: stateless per tick, parallel extraction via
  rayon, one write per relation. No new long-lived state.
- The `_file` cache and the per-language registries already exist.

## Storage / reads / writes / uniqueness

- runtime: 4 new tables (`rel_call_def`, `rel_call_site`, `rel_call_edge`,
  `rel_call_edge_rev`), same shape as the TYPE_RELS tables.
- reads: `_file` for the file set; SCIP `scip_ref` for callee resolution
  override (engine.rs:2274 already does this for types).
- writes: one `refresh_rel` per table per tick, gated by `call_rels_used`
  like every other lazy indexer.
- uniqueness: symbols dedup by `file::kind::name`; `call_edge` dedups across
  revs from `call_edge_rev` (same rebuild-legacy pattern as type_edge).

## Phase D gate

`closure(call_edge)` answers forward/backward reachability over a Rust or TS
corpus, no per-program extraction rules. The 6 `examples/callgraph-*.dl`
files delete; `use "std/callgraph.dl"` plus a `? call_reaches("main", dst)`
query replaces them. The seeded point query (engine.rs:2651) works on
`call_edge` for free, since `closure` already keys the condensation cache by
edge rel name.

---

# Phase E - Programmable go-to-def / find-references

Today's `handle_definition` (lsp.rs:167) and `handle_references` (lsp.rs:192)
key off the ref spine (`_where_bytes`) only: a hit is a located string, and
the "symbol" is the literal text. Phase E makes these query program-defined
relations, so go-to-def on a type reaches `type_entity` and on a function
reaches `call_def` (Phase D), with the spine as fallback.

## Surface

Two program-defined rels the LSP handlers query, plus the fallback path:

```dl
% program populates these; LSP queries them. No new built-in rel declarations,
% these are author conventions the handlers know about by name.
rel def_target(symbol: text, file: file, line: int, kind: text).
rel ref_site(symbol: text, file: file, line: int, kind: text).

% one shape that unifies the existing built-ins into the def/ref surface
def_target(sym, f, l, "type")  <- type_entity(sym, _, _, _, f, l).
def_target(sym, f, l, "fn")    <- call_def(sym, _, f, l, _).
ref_site(sym, f, l, "call")    <- call_site(_, sym, f, l).
ref_site(_, f, l, "import")    <- module_import(f, _, spec, _, l), string(_, spec, _), ...
```

## Type signatures

```rust
// engine.rs
pub fn symbol_under_cursor(&self, path: &str, byte: u32) -> Result<Option<String>>;
//   pseudo:
//   - resolve the byte to a located spine string (existing span_at path)
//   - OR look up a program-defined `symbol` rel if the program declared one
//   - return the symbol text

pub fn def_targets(&self, symbol: &str) -> Result<Vec<(String, u32, String)>>;
//   pseudo:
//   - if def_target rel declared: SELECT file, line, kind WHERE symbol = ?
//   - else: return spine-based module_edge targets (today's behavior)

pub fn ref_sites(&self, symbol: &str) -> Result<Vec<(String, u32, String)>>;
//   pseudo:
//   - if ref_site rel declared: SELECT file, line, kind WHERE symbol = ?
//   - else: return every located span with the same StringId (today's behavior)

// lsp.rs handle_definition / handle_references
//   pseudo:
//   - resolve symbol under cursor via symbol_under_cursor
//   - if def_target/ref_site declared, query and return locations (with in-target
//     line position from the rel, not Range::default())
//   - else: fall back to today's spine-only behavior
```

Capabilities unchanged at lsp.rs:57-67 (`definition_provider`,
`references_provider` already declared). The handlers gain a program-defined
path before the spine fallback.

## Instance lifetimes

- All three queries are stateless, same shape as `diags(None)`.
- No new long-lived state.

## Storage / reads / writes / uniqueness

- no new durable storage.
- reads: `def_target` / `ref_site` program tables; `_where_bytes` /
  `_strings` for the spine fallback.
- writes: none.
- uniqueness: a symbol may have many def_targets (overloads, multi-module
  same name) and many ref_sites; LSP returns all. The kind col lets the
  editor disambiguate.

## Phase E gate

Right-click a Rust function call, go-to-def lands on its `call_def` row (not
just on any located string matching the name). Find-references on a type
returns every `ref_site` row (call sites, import sites, field accesses),
not just spine occurrences. A program with no `def_target` / `ref_site`
rels falls back to today's spine-only behavior, no regression.

---

# Phase F - Full SCIP descriptor unpack (deferred, on demand)

The data is loaded (`scip_def`/`scip_ref`/`scip_edge`); only the trailing
descriptor identifier is parsed today (`scip_descriptor_name` engine.rs:192).
Phase F parses the SCIP descriptor grammar into `typegraph::TypeExpr` so
`type_sig` and `call_def` can be filled from SCIP when present, supplementing
the diet syntactic extractors.

## Why on-demand

Diet `type_sig` + Phase D's `call_def` cover the common case syntactically.
Phase F matters when a program needs compiler-resolved facts the syntactic
extractor cannot reach (generic monomorphization, trait resolution,
cross-crate type inference). Pick it up when a real program demands it.

## Phase F gate

A program referencing `type_sig` or `call_def` against a Rust corpus with
`index.scip` present shows SCIP-resolved facts layered on the diet ones. No
behavioral change when the index is absent (diet path carries).

---

# Phase G - CSS/XPath selector op + `rs:` symbol scheme (candidates)

Native selectors and the `rs:` symbol-literal scheme are both candidates for
new `BodyItem` variants or scheme arms. Both have working `cmd`-based paths
today (`pup`/`xq`, manual string syms). Add the native forms when the
cmd-based path is measurably load-bearing.

The contract for new ops, per the commitment above: one variant per
query-language family. CSS/XPath is the fifth family (regex / S-expr /
structural / pattern / selector). The `rs:` scheme is one row in
`desc.rs:69-77` plus a resolution arm against the unified symbol identity
from Phase D.

## Phase G gate (to revisit)

`v5/std/parsers/html.dl` exists, uses `cmd` with `pup`, and reports real
pain (subprocess overhead, missing spans, output re-search for spine
location). Then `select(path, rev, :html, "css-selector", out)` compiled
against `html5ever` + `selectors` is justified. Same shape for `rs:` once a
program needs symbol literals in `scan` / `match` positions.

---

# Sequencing summary

| phase | size | blocks | blocked by |
|---|---|---|---|
| 3-ary scan default WORK | 1h | nothing | nothing |
| A1 `use` only | 1-2 days | A2, std/parsers, std/callgraph | nothing |
| B `allow_missing` | 0.5 day | missing_repo UX | nothing |
| A2 `def` templates | 1-2 days | arity reuse at scale | A1 |
| C LSP `info` + hover | 1-2 days | editor richness, anim pipeline | nothing (benefits from A1) |
| D CALL_RELS + unified symbol | 2-3 days | Phase E, std/callgraph.dl | nothing (parallel with A1/B) |
| E programmable go-to-def / find-refs | 1-2 days | editor richness, cross-format linking | D (uses unified symbols) |
| F full SCIP unpack | 2-3 days | compiler-resolved types | real demand |
| G CSS/XPath op + `rs:` scheme | 2-3 days each | native selectors, sym literals | real demand |

3-ary scan is a freebie; do it any time. A1 + B + D are mutually independent
and can run in parallel. A2 after A1. C and E both benefit from A1 (so the
new rels are hover/query targets) and from D (so go-to-def reaches calls).
F and G are on-demand, not sequenced.

## Hard style rules (from repo CLAUDE.md, repeated because they gate review)
- Never a per-row write loop; collect, then one Db::insert_rows.
- Banned identifiers AND prose: provenance, substrate, load-bearing, regime.
  Use the plain word (source/origin, base layer, critical, mode).
- No em dashes in comments/docs.
- Match existing file style (engine.rs patterns, error handling idioms).
- Never assume a library is available; check Cargo.toml first.
