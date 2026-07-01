# Programmable-LSP arc: ops authoring frontend + incremental closures + real LSP

Date: 2026-06-01. Status: PLAN (no code yet). Supersedes the "freeze the engine"
framing.

## The commitment (read this first)

**Datalog is the frozen IR. Ops return as a compile-time authoring frontend that
lowers to it. Nothing replaces the SQL fixpoint.**

- The frozen IR is `ast::Program { items: Vec<Item> }`, `Item = Rel | Rule |
  Query`, `Rule { head: Atom, body: Vec<BodyItem> }`, `BodyItem` a closed enum
  (`ast.rs:74`). The frontend emits ONLY these variants.
- Non-negotiable: an op/template adds **zero new evaluation semantics**. If it
  cannot lower to existing `BodyItem`s (`Scan`/`Match`/`Ast`/`Sg`/`Json`/`Pos`/
  `Neg`/`Cmp`/`Closure`), it does not go in. This is the wall that stops the
  language-design rabbit hole that produced the v5 desperation flight.
- Why this resolves "reassess datalog": datalog was right *as evaluation*; the
  mistake was deleting the *authoring surface* (v3/v4 ops) along with the 60k-LOC
  bespoke runtime. We put back composition, LSP, and incremental closures —
  NOT the runtime.

## The single gate demo (the whole arc passes iff this works)

Open a real TS/Rust repo. Author, in ~5 composable lines:

> "no `.unwrap()` in code reachable from `app/main.tsx` inside an effect, severity warn"

1. `dl rule.dl --check` blocks `git commit` (husky hook). — Phase A
2. The squiggle appears **live as you type**, sub-100ms on a real repo. — Phase B+C
3. Hover the `unwrap` shows "reachable from main (warn)"; a code action offers the fix. — Phase C

Anything that does not move this demo forward is the substrate bias re-asserting; cut it.

## Why the product is already *expressible* (capability is not the gap)

`closure` + `sg`/`ast` + `diag` + `--lsp` + `--check` all exist. Today the rule
takes ~40 copy-pasted lines across files (re-declare 6 rels, re-derive the call
graph, hand-wire range joins). The gap is purely **authoring ergonomics, LSP
quality, predictable perf** — the three things v5 over-cut.

---

# Phase 0 — Lowering contract (1 doc, no code)

Write `v5/docs/lowering-contract.md`. Content: the commitment above, plus the
enumerated lowering table (every surface construct → the exact core `Item`s it
expands to). This is the artifact every later PR is checked against. ~1 hour.
Gate: the doc exists and lists each Phase-A construct's expansion.

---

# Phase A — Ops authoring frontend + lint std-lib (THE KEYSTONE)

Goal: the canonical rule is ~5 composable lines and `--check` blocks the commit.
Ships the product on the CLI using the *already-working* `--check` path (one full
tick is fine for a CLI gate; no perf/LSP work needed here).

## Surface (kept deliberately small — three constructs, no more)

```
use "std/callgraph.dl"                  # import: splice another module's decls
def reachable_unwrap(entry) {           # def: a parameterized rule component
  reaches(entry, fn), unwrap_at(fn, path, l, c, el, ec)
}                                        # ...lowers to ordinary rules
lint no_unwrap from "app/main.tsx" severity warn {   # lint: one built-in macro
  sg(:rust, "$X.unwrap()")
}
```

`use` + `def` are the composition primitives (the React analogy). `lint` is a
single convenience macro, NOT extensible grammar — it desugars to a fixed
hit-rel + reaches-join + diag-rule shape.

## Type signatures (new module `src/frontend.rs`)

```rust
// A parsed surface item: either a core IR item, or one of three sugar forms.
enum SurfaceItem {
    Core(ast::Item),          // rel / rule / query — passes through untouched
    Use(Import),              // `use "path"`
    Def(RuleTemplate),        // `def name(params) { body }`
    Lint(LintDecl),           // `lint name from "entry" severity sev { pattern }`
}
struct Import { path: String }                              // resolved against include roots
struct RuleTemplate { name: String, params: Vec<String>, body: Vec<ast::BodyItem>,
                      head_rel: String, head_terms: Vec<ast::Term> }
struct LintDecl { name: String, entry: Option<String>, severity: String,
                  pattern: ast::BodyItem /* Sg|Ast|Match */, msg: String, hint: Option<String> }

// Parse the EXTENDED surface. Falls back to the existing core parser per item.
fn parse_surface(toks: Vec<lex::Tok>) -> Result<Vec<SurfaceItem>>;
//   pseudocode:
//   - same loop as parse::parse, but item dispatch also matches "use"/"def"/"lint"
//   - "use"  -> Use(Import{ next string literal })
//   - "def"  -> RuleTemplate (parse params (idents), then a brace-delimited body
//               reusing the existing body parser)
//   - "lint" -> LintDecl (parse name, optional `from STR`, `severity IDENT`,
//               brace body holding ONE Sg/Ast/Match)
//   - else   -> Core(existing self.item())

// Expand the surface into the frozen core IR. The ONLY place sugar disappears.
fn expand(items: Vec<SurfaceItem>, loader: &mut ModuleLoader) -> Result<ast::Program>;
//   pseudocode:
//   1. resolve every Use: loader.load(path) -> Vec<SurfaceItem>, recurse, dedup
//      by canonical path (diamond imports load once). Splice resulting core Items.
//   2. collect Defs into a template table (name -> RuleTemplate).
//   3. a Def referenced as a body atom `name(args)` in another rule/def is
//      INLINED: clone the template body, substitute params (Var(param) -> arg
//      Term), alpha-rename every NON-param internal var to `__<tplname><n>_<var>`
//      so two instantiations never capture each other. Emit the resulting Rule.
//   4. each Lint desugars to fixed core Items (see table below).
//   5. assert all rel names referenced exist (after expansion); bail with a
//      surface-level span on unknown rel / arity / unknown template.

// Loader holds the include search roots + the per-compile dedup cache.
struct ModuleLoader { roots: Vec<PathBuf>, loaded: HashMap<PathBuf, Vec<SurfaceItem>> }
fn ModuleLoader::load(&mut self, path: &str) -> Result<Vec<SurfaceItem>>;
//   resolve `path` against roots (std lib dir bundled next to the binary, then
//   the program file's dir); read; lex; parse_surface; cache by canonical path.
```

## Lint desugaring table (the fixed macro — pin this in Phase 0 doc)

`lint no_unwrap from "app/main.tsx" severity warn { sg(:rust,"$X.unwrap()") }` becomes:

```
rel no_unwrap_hit(fn:text, path:file, line:int, col:int, end_line:int, end_col:int).
no_unwrap_hit(fn,path,l,c,el,ec) <-
    fndef(fn,path,s,e),                      # fndef/reaches come from `use "std/callgraph.dl"`
    scan("WORK","**/*",path,rev),
    sg(path,rev,:rust,"$X.unwrap()",l,c,el,ec),
    s<=l, l<=e.
diag(path,l,c,el,ec,"warn","no_unwrap","unwrap() reachable from app/main.tsx", "...") <-
    reaches("app/main.tsx_entry", fn),       # `from` clause -> reaches join; omit clause -> no join
    no_unwrap_hit(fn,path,l,c,el,ec).
```

The "inside an effect" clause is a second containment join the author adds in a
`def` (it is NOT special syntax) — `effect_span(path,es,ee), es<=l, l<=ee`.

## std-lib (the reuse layer datalog lacks today)

Ship `v5/std/*.dl`, resolvable by `use`:
- `std/callgraph.dl` — `fndef`, `callsite`, `calls`, `reaches <- closure(calls)`
  (lifted verbatim from `examples/callgraph-resolved.dl`).
- `std/reachable.dl` — `reachable_from(entry, fn)` def over `reaches`.
- `std/lints.dl` — common `def`s (`no_panic`, `no_dbg`, `no_unwrap`).

## Instance lifetimes (Phase A)

- `ModuleLoader`, template table, `ExpandCtx`: live for ONE compile
  (`parse_surface` -> `expand` -> `Program`), then dropped. No persistence, no
  runtime state. Expansion is pure.
- After `expand`, the resulting `Program` flows into the unchanged
  `engine::tick` / `lower_rule` path. The engine never sees sugar.

## Storage layout / reads / writes / uniqueness (Phase A)

- Storage: NONE at runtime. Frontend is compile-time only.
- Reads: module files from include roots (filesystem, once per path per compile).
- Writes: none.
- Uniqueness: after expansion every `rel` name is globally unique. Template
  instantiation namespaces internal rels/vars by `__<tplname><instance>_…`. A
  `use`d module's rel decls dedup by name (re-declaring identical cols is a
  no-op; conflicting cols is a hard error with both source paths).

## Wiring

`run_file` / `run_check` / `run_lsp` change one line: `parse::parse(lex(src))`
becomes `expand(parse_surface(lex(src)), &mut loader)`. Everything downstream is
untouched. Core `.dl` files (no `use`/`def`/`lint`) parse identically — backward
compatible.

## Phase A gate

Author the unwrap-reachable-from-main rule in ≤5 lines using `use std/callgraph.dl`
+ one `lint`. `dl rule.dl --check` exits non-zero on a violation; a husky
`pre-commit` calling it blocks the commit. Existing examples still run unchanged.

---

# Phase B — Incremental closures (pure engine, zero language risk)

Goal: per-keystroke cost scales with edit size, not corpus size. Precondition for
the LSP feeling real. Rides the existing
[relation-digest-skip plan](2026-05-30-relation-digest-skip-plan.md).

## The problem, grounded

`rebuild_closures` (`engine.rs`) on every affected tick: `load_edges` (full
reload of the edge rel from SQLite) + `scc::build_condensed` (full Tarjan over
the whole graph) + DELETE+reinsert `scc_node_*`/`scc_edge_*`. So editing a
comment in any file feeding `calls` re-condenses the entire call graph.

## Type signatures

```rust
// Cache the condensation in the Engine across ticks (today it is rebuilt every tick).
struct ClosureCache { by_edge: HashMap<String /*edge rel*/, CachedCondensation> }
struct CachedCondensation { edge_digest: u64, cond: scc::Condensed, names: Vec<String> }

// New gate in front of rebuild_closures.
fn Engine::refresh_closures(&mut self, edges: &[&str]) -> Result<()>;
//   pseudocode, per edge:
//   - digest = self.edge_digest(edge)?            // cheap: hash of edge rel rows,
//                                                 // reuse relation-digest-skip machinery
//   - if cache.by_edge[edge].edge_digest == digest { continue }   // SKIP: no topology change
//   - else: load_edges + build_condensed (bounded full recompute ONLY when edges
//           actually changed), write scc_* tables, update cache.

fn Engine::edge_digest(&self, edge: &str) -> Result<u64>;
//   SELECT over the edge rel's two key cols, folded into a stable hash. The
//   relation-digest-skip plan already specifies this; wire it to closures.
```

## Why this is the honest, low-risk win (not full incremental SCC)

Full delta-maintained SCC under edge removal is research-grade. We do NOT attempt
it. The observation: **most keystrokes do not change call-graph EDGES** (editing a
string, comment, whitespace, a non-call expression). For those, the digest is
unchanged and the closure rebuild is skipped entirely — O(0). When edges *do*
change, we pay one bounded full condense. Cost becomes predictable and bimodal:
free when topology is stable, one-condense when it moves. That is the property the
LSP needs.

## Instance lifetimes (Phase B)

- `ClosureCache` lives on `Engine` for the engine's lifetime (the whole `--watch`
  / `--lsp` session). Seeded on cold tick, updated on edge-digest change.

## Storage / reads / writes / uniqueness (Phase B)

- Storage unchanged: `scc_node_<edge>` / `scc_edge_<edge>` tables stay the
  durable form. The in-memory `CachedCondensation` is the skip oracle.
- Reads: edge-digest query each tick (cheap); full edge load only on change.
- Writes: scc_* tables only when the digest moves.
- Uniqueness: digest keyed by `(edge rel name)`; one condensation per graph
  (matches `dedup_edges`).

## Phase B gate

Instrument tick: editing a comment in a file feeding `calls` rebuilds 0 closures
(log line shows "closures: skipped"). Editing a file that adds/removes a call
rebuilds exactly that graph. A per-keystroke tick on a real repo is sub-100ms.

---

# Phase C — Real LSP on the op surface (rides A + B)

Goal: live squiggle as you type, hover, code actions. Port v3's surface, pointed
at the structured Phase-A surface and the ref-spine spans.

## The problems, grounded (`lsp.rs`)

- `TextDocumentSyncKind::NONE` (line 37): only `didSave`/`didOpen` re-tick. No
  live-as-you-type.
- The tick runs INLINE on the message loop (line 65): a slow tick freezes the
  server. (Phase B makes ticks fast; this still needs decoupling for safety.)
- No hover, no code action, no completion capabilities declared.

## Type signatures

```rust
// In-memory doc buffers for incremental sync (today there are none).
struct DocStore { open: HashMap<PathBuf, String /*current text*/> }
fn DocStore::apply_change(&mut self, uri: &Uri, changes: &[TextDocumentContentChangeEvent]);
//   INCREMENTAL sync: apply each range edit to the buffer; debounce a tick.

// Decouple tick from the message loop: coalesce dirty paths, tick on a debounce.
struct TickScheduler { dirty: HashSet<PathBuf>, debounce: Duration }
fn TickScheduler::mark(&mut self, p: PathBuf);
fn TickScheduler::drain_after_debounce(&mut self) -> Option<Vec<PathBuf>>;

// Hover: byte offset under cursor -> the diag/relation row located there.
fn Engine::locate_at(&self, path: &str, byte: u32) -> Result<Vec<LocatedRow>>;
//   query _where_bytes (ref-spine) JOIN diag/relations WHERE path=? AND lo<=byte<hi.
//   ref-spine already stores the byte spans (Phase C5 + today's repo threading).

// Code action: a diag carrying a hint/fix -> a workspace edit (ties to refactor.rs splice).
fn diag_to_code_action(d: &DiagRow) -> Option<CodeAction>;
```

Capabilities to declare: `change: INCREMENTAL`, `hoverProvider`,
`codeActionProvider`, (optional) `completionProvider` for `.dl` authoring.

## Instance lifetimes (Phase C)

- `DocStore`, `TickScheduler`: live for the LSP session on the server struct.
- The `Engine` stays the single owner of DB state; the scheduler feeds it
  coalesced dirty paths. Tick may move to a worker thread; diagnostics publish
  on tick completion.

## Storage / reads / writes / uniqueness (Phase C)

- No new durable storage. Hover reads `_where_bytes` + relation tables. Code
  actions read `diag.hint`/`code`.
- Uniqueness: doc buffers keyed by canonical path; one open buffer per file.

## Phase C gate

Type in a TS/Rust file; squiggle updates within the debounce, no save needed.
Hover an `unwrap` shows the lint message + "reachable from main". A code action
applies the fix via the refactor splice.

---

# Auto-refactor track (parallel, OFF the critical path)

Nice-to-have, must stay achievable. Rides the same ref-spine + `rspath`
primitives. Independent of A/B/C; do when wanted.

```rust
// Port from the v1/v2 archive (crates/watch/src/change.rs):
fn classify_changes(events: &[FsEvent], hashes: &FileHashes) -> Vec<SemChange>;
//   correlate delete+create by content_hash -> Move{old,new}; ref-diff -> Rename.
// Port js_path.rs for TS/JS relative-path math; Rust side exists in rspath.rs.
// Wire a detected Move/Rename into the existing --move splice path so --watch
// rewrites imports automatically instead of requiring `--move OLD=NEW`.
```

Gate: rename/move a file under `--watch`; importers' paths rewrite on disk.

---

# Sequencing + the no-6th-rewrite property

Order: **0 → A → B → C**, auto-refactor parallel.

- A ships the product on the CLI (commit gate) with good authoring, using the
  existing `--check`. If authoring still feels bad, we learn it cheaply HERE,
  before building LSP on top.
- B is pure engine, no language risk, and makes C usable.
- C makes it live.

Why this is not a 6th rewrite: B, C, and auto-refactor are **additive** to v5's
existing 6k-LOC core; A is a **frontend that lowers into** that core. The SQL
fixpoint engine stays. We are putting back the three things v5 over-cut
(composition, LSP, incremental closures) WITHOUT the 60k-LOC bespoke runtime.

# Open questions (resolve before coding each phase)

- A: does `def` allow recursion (template calls template)? Start NO (inline only,
  reject cycles); revisit if a real lint needs it.
- A: `from "app/main.tsx"` — is the entry a file or a function symbol? The reaches
  graph keys on function names; need an entry-resolution rule (file -> its
  top-level fns). Likely a `std/entry.dl` helper.
- B: edge-digest granularity — whole-rel digest (simple, skips when ANY edge
  changes) vs per-file edge partition (skips more, more bookkeeping). Start
  whole-rel; measure.
- C: tick on worker thread vs inline-after-debounce. Start inline-after-debounce
  (Phase B makes it fast enough); move to worker only if measured stalls remain.
