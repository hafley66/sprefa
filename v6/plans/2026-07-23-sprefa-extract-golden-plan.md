# sprefa-extract — the golden plan (distill v5, normalize for v6)

> Canonical TYPE source: `v6/sprefa-seed/src/_3_extract/` (the header-only type
> math; `cargo check` green). This file is the EPIC decomposition + recon + golden
> tests. Types are not re-derived here — they are pointed at.

## Context

v5's static-analysis extraction is a working, multi-language subsystem that
became a maintenance nightmare: ~16.7K lines across `src/graph/typegraph` (syn
for Rust), `src/graph/modgraph`, `src/engine/extract/`, `src/scip_import.rs`,
`src/scip_setup.rs`. It also amplified the resident-heap disease that pushed v5
to 36 GB swap. Three independent recon passes (two over the v5 tree, one over the
representation prior art) landed the facts this plan rests on.

### Recon — observed facts (v5, at HEAD)

- Two extraction traits. `TypeLang: Sync` (`src/graph/typegraph/mod.rs:439`) —
  `name`/`matches`/`extract`/`extract_calls`/`extract_dataflow`/`extract_bundle`
  with an `AnalysisMask { types, calls, dataflow }`. `ModuleResolver: Send + Sync`
  (`src/graph/modgraph/mod.rs:171`) — `exts`/`edges(file, content, &ProjectCx)`.
  Asymmetric: one is per-file (`Sync`, no context), the other needs the file set
  (`&ProjectCx<'a>`). This asymmetry is REAL (the cache-key split) and is kept.
- The four families are declared in RUST, not `.dl`: `src/engine/decls.rs`
  `module/type/call/dataflow_rel_decls()`, with CLOSED enum "brands"
  (`builtin_enum_brands`, decls.rs:73-134) the typechecker enforces. The `.dl`
  files only consume them. Variant sets (the authoritative kind vocabularies):
  - `df_node_kind` (23): param let_bind var_read var_write lit call_res new
    member ret borrow binop unop loop if match block closure try break expr
    (+TS: cond logic concat template).
  - `type_entity_kind` (9), `type_edge_kind` (7), `CallKind` (3),
    `module_binding` kind (5: named default namespace side_effect reexport),
    `const_value_kind` (3: lit template concat).
- THREE different `kind` representations in v5: typed enums (type+call),
  free-form `String` (df), `&'static str` (edges/modules). FOUR different span
  shapes: byte range `Option<(u32,u32)>` (modules), line-only (type/call),
  line+col with MIXED char-vs-byte col (df), kind-salted `WhereBytes` (CST/spine).
- SPLIT node identity: `mint_sym` coordinate strings `file::kind::name`
  (`typegraph/mod.rs:412`, ~26.6% of dict bytes) for type/call; dense `NodeIdx`
  → `_df_node_dict` coordinate surrogate for df; kind-salted `WhereBytes` id for
  CST. (`salt_rev`, the function the older plans cite, does NOT exist in current
  v5 — grep is empty; the live digest logic is `extract_input_digest`,
  engine/extract/mod.rs:861. Flagged so the next agent does not chase a ghost.)
- Per-language roster. TypeLang impls: rust (syn), ts (oxc), kotlin (tree-sitter),
  go+python (tree-sitter for comments/strings, SCIP for call/type). ModuleResolver
  impls: rust/ts/kotlin/go/python. EVERY lang has both. NO ast-grep family
  extractor today (ast-grep is the term-op `sg` only). Parse sharing is opt-in:
  only `RustTypes` overrides `extract_bundle`; the other four parse once per
  family (3× waste the bundle seam was meant to kill).
- SCIP is a FORMAT / resolution overlay, not a family. `scip_import::load`
  (scip_import.rs:96) parses symbol/range/role/display_name/relationships into
  `scip_*` rels; the engine consumes them as resolution overrides
  (`scip_ref` overrides name resolution, std/flow.dl:96-114). v5 already tiered:
  Tier 1 SCIP (compiler-backed, "nearly free to add a lang"), Tier 2 native AST
  (the Kotlin-sized lift), tree-sitter floor. `.agents/memory/reference_scip_multilang_indexers.md`.
- The FIFTH family v5 half-built lives in a stdlib `.dl`, not Rust: `flow_edge`
  (`std/flow.dl:89`) = `df_edge ∪ arg→param positional hop ∪ ret→call_res
  backward hop` over `call_edge`. `examples/flow-interproc.dl` states the thesis:
  df stops at fn boundaries, call_edge walks symbols not values, flow_edge unites
  them. Left in `.dl` because v5 never typed it.
- v5 already got the content-addressing + demand-skip right at the engine seam:
  `FactCache<(repo,path,hash), (rid, Arc<T>)>` (engine/extract/mod.rs:51) caches
  per-file by content; `moved_extract_rels(family, files, with_scip) -> Vec<(rev,
  digest)>` (mod.rs:903) is the per-rev digest-skip gate. These are the seeds of
  the v6 two-phase cache keys.
- v5 stated gaps honestly: per-site callee cloning (k-CFA) out of scope; node-level
  types function-level only (type_sig by-position vs df_node by-name); no CFG /
  dominator / use-def family.

### Recon — prior art (representation only; lazy/IVM stripped)

Stack Graphs (index phase = per-file unresolved; query phase = merged resolved)
IS the two-phase split, verbatim. Kythe (anchor = content-addressed `(file, span)`
vs semantic node = name-resolved; `defines/binding`/`ref`/`childof`/`extends`).
SCIP (`Occurrence{symbol,range,role}` + `SymbolInformation.Kind` + the
`SymbolRole` bitfield Definition/Import/Write/Read/Generated/Test/ForwardDef;
relationships as bools). Joern CPG unifies AST+CFG+DDG+CDG+call with edge
properties (`REACHING_DEF.VARIABLE`, `ARGUMENT_INDEX`) — maps to v6's aux side
projections. CodeQL normalizes at the STORAGE level (reject: per-language
predicate normalization); local-vs-global dataflow is a QUERY concern, not a
storage kind (intra/inter derivable from `enclosing_symbol` equality). SVF/WALA/
Soot converge on: def/use sites + CFG + DDG, with register def-use syntactic
(emit it) and memory def-use points-to-dependent (downstream). Arena AST
(oxc/biome): one `Allocator` per rayon task, parse one file, project to owned
Vecs, drop the arena — the CST never crosses a thread; peak RSS = biggest file.
Parser buy-vs-buy: tree-sitter (40+, the universal backup), oxc (JS/TS, arena,
~3× swc), syn (Rust proc-macro), ast-grep (pattern over tree-sitter), rowan
(lossless CST, you write the parser).

### Plan boundary

- **In:** `sprefa-extract` turns `(source bytes, language, FamilyMask)` +
  `(optional SCIP index)` into `RawNode`/`RawEdge`/`ProjectEdge` + aux, in the
  `_0_shape` vocabulary. SYNC, CPU-bound, rayon-parallel, arena-mastered. No DB,
  no async, no reactor, no store-id type in any public signature.
- **Out (other layers own):** the SQLite store (`sprefa-store`, the spine + node/
  edge tables), the graph algorithms (`sprefa-graph`), the reactive/demand engine
  + the async-eval flip (`sprefa-engine`/`sprefa-server` — "syntax to mark when
  not" is a LATER dilemma, not this crate's), cross-file name RESOLUTION
  execution (extract emits unresolved phase-1 specifiers + phase-2 resolutions;
  the store joins), full points-to / memory-SSA / reaching-definitions.
- **Lowering:** source → arena CST (Parser tier) → masked projection (FileExtract
  / ProjectExtract) → owned `RawNode`/`RawEdge` Vecs → (engine seam) intern into
  store `node`/`edge`. The CST is transient; only the projection escapes the
  arena.

## Decisions (the rulings this crate rests on)

1. **ONE coordinate, ONE typed-kind ordinal per family, node id = (family, span,
   kind).** Deletes `mint_sym`, the four span shapes, the three kind reps, and the
   split node identity — structurally, not by discipline. (seed `_0_shape`.)
2. **Two traits, kept two.** `FileExtract` (key: blob+lang+mask) and
   `ProjectExtract` (key: blob+project_digest+mask). The cache-key difference is
   the one distinction worth preserving; everything else unifies. (seed `_2_traits`.)
3. **Tiered sources, merged.** SCIP = ground truth for call/type/module
   resolution; native AST owns dataflow + spans; tree-sitter is the floor + the
   CST family. (seed `_2_traits::Source`, `_4_scip`.)
4. **SCIP is bought, not built.** Foreign indexers behind a subprocess seam;
   diet-scip keeps only symbol/range/role/relations. No bespoke indexer, no FFI.
5. **Arena-per-file on rayon.** The 36GB-kill: each worker owns a `ParseArena`;
   the CST never crosses a thread; peak RSS = biggest single file.
6. **flow_edge promoted to a typed 5th family** (`FlowEdgeKind`), lifted out of
   the stdlib `.dl` into the type system. k-CFA / dominators / CFG stay frontier.
7. **Concrete types until a second impl arrives** (crate-map practicality ruling).
   The seed traits are the contract; the real crate starts with concrete structs.

## The epics (dependency order)

### Epic 0 — the type math (DONE)
**Goal:** the normalized type vocabulary, in one header. **Status:** landed in the
seed (`_3_extract/`, 7 files, `cargo check` green at `6dea0166`).
**Done:** every v5 kind brand is a typed enum; one `Span`; node id = (family,
span, kind); the parity-surface `Extract` trait + `Tasks` stub + `ExtractPlan`
epic ledger exist (`mod.rs`).
**Golden:** `cargo check` clean; a grep audit that every v5 `decls.rs` brand maps
to a `_0_shape` enum variant (no vocabulary lost).

### Epic 1 — hollow crate + tiered parser registry (buy-vs-buy)
**Goal:** stand up `sprefa-extract` as a real crate with concrete-but-empty impls
behind the seed traits, and LAB the parser choice per language.
**Contract** (seed `_2_traits::Parser`): `fn parse<'a>(&self, content: &[u8], arena:
&'a ParseArena) -> Cst<'a>`. Registry: `Source { parser, ast, scip }`.
**Storage/identity:** no state across files; one arena per parse.
**Tasks:**
1. crate skeleton: `Cargo.toml` (own `[workspace]`, like sprefa-store), `lib.rs`
   re-exporting the seed traits, `Tasks` moved from the seed to the crate.
1.1 the `Parser` impls: `SynParser` (Rust), `OxcParser` (JS/TS), `TsParser`
   (floor; all other langs). One per backing tool, behind the trait.
1.2 `Source` registry table: the v5 roster (rust/ts/kotlin/go/python/c) mapped to
   tiers, lifted from `lang_tables.rs`.
1.3 buy-vs-buy bench: parse throughput (MB/s) + peak RSS per parser over a real
   corpus (the sprefa tree + a TS-heavy repo). oxc-vs-tree-sitter for JS/TS is
   the live decision; syn-vs-tree-sitter for Rust is the second.
**Lowering:** `Parser` → `Cst` (arena-borrowed); no projection yet.
**Done:** the crate compiles standalone; the bench prints the per-parser numbers.
**Golden:** parse sprefa's own `src/` through each Rust parser; assert the node
COUNT (not positions) agrees to within a documented tolerance.

### Epic 2 — arena-per-file RAM mastery (the 36GB-kill)
**Goal:** prove RSS stays flat under N-worker parallel parse, stressed to breaking.
**Contract** (seed `_2_traits::ExtractBudget`, `_2_traits::ParseArena`): the budget
fields `rss_bytes` / `arena_bytes` / `workers`; memcap (setrlimit/getrusage,
mirroring sprefa-store) aborts a worker over `rss_bytes`.
**Instance timeline:** arena created per rayon task → parse one file → project to
owned `Vec<RawNode>`/`Vec<RawEdge>` → arena dropped → output sent back. The CST
never outlives the task.
**Storage/identity:** resident state = the per-task arena + the projected Vecs;
nothing accumulates across files (v5's resident memo/parse-tree pile is gone).
**Tasks:**
2. `ParseArena` over `bumpalo` (or oxc's allocator for the oxc path); cap at
   `arena_bytes`; a file over it is skipped + logged (never beachballs).
2.1 rayon `par_iter` over `Vec<ExtractJob>`; each worker owns its arena.
2.2 memcap guard: a worker exceeding `rss_bytes` aborts + reports (the sprefa-store
   pattern), so the OS never swaps.
2.3 the stress lab: ramp workers 1→cores and file sizes to a multi-GB corpus; plot
   peak RSS + wall-time. Find the cliff (arena cap too high, or a leak).
**Done:** peak RSS ≤ `biggest_single_file_arena × workers + overhead`, flat across
corpus size; the lab report names the breaking point.
**Golden:** parse a synthetic 10×-duplicated corpus (same blob, 10k paths) under
the gun; assert RSS is independent of path count (content-addressed parse runs
once) and independent of corpus bytes beyond one arena.

### Epic 3 — two-phase extraction + SCIP tier merge (the core port)
**Goal:** port v5's four families behind `FileExtract`/`ProjectExtract`, port the
SCIP source, and define the merge. This is where v5's 16.7K lines lands, normalized.
**Contract** (seed `_2_traits`): `FileExtract::extract(path, content, mask) ->
FileBundle`; `ProjectExtract::resolve(path, file, cx, mask) -> ProjectBundle`;
`ScipSource::build/load`; `Extract::merge`.
**Storage/identity:** phase-1 cache key `(BlobHash, lang, FamilyMask)`; phase-2 key
`(BlobHash, ProjectDigest, FamilyMask)`. `NameId` is extract's arena interner; the
engine maps `NameId → store::StrId`, `Span → file_bytes`, `RawNode → node_id` by
`(family, span, kind)`.
**Tasks:**
3. port `typegraph/rust` (syn) → `RustFileExtract` + `RustProjectExtract`,
   swapping sym→span, kind-String→`DfNodeKind`, fn_sym→`scope: Span`.
3.1 port `typegraph/{ts,kotlin,go,python}` (oxc/tree-sitter) the same way.
3.2 port `modgraph/{rust,ts,kotlin,go,python}` → the `ProjectExtract` halves +
   the `Binding` side table; `ProjectCx` indexes (`RustCrates`/`GoIndex`/...) stay
   lazy `OnceLock`.
3.3 `ScipSource` impls: shell-out per `INDEXERS` (scip_setup.rs:51) + the diet
   protobuf parse (scip_import.rs:96 → `ScipIndex`).
3.4 `merge`: SCIP def/ref overrides call/type/module resolution; AST fills df +
   spans. Emit `ProjectEdge { kind: ScipOverride | NameResolve }`.
3.5 parity: byte-identical `RawNode`/`RawEdge` sets vs v5 on the sprefa corpus
   (the oracle is v5 itself, run side-by-side).
**Lowering/compat:** diagnostics (parse errors, unresolved specifiers) surface as
`Resolution::Unresolved` + a side channel; the engine owns what to do with them.
**Done:** all four families + SCIP ported; parity green vs v5 on one repo.
**Golden:** extract the sprefa repo through v6 and v5; diff the flattened
`(family, file, span, kind)` node set and the edge set — empty diff (modulo the
documented `sym→span` + `kind`-enum normalizations).

### Epic 4 — parallel dispatch + contention lab
**Goal:** the top-level `dispatch` (seed `Extract::dispatch`) over rayon, with the
shared-read `ProjectCx`, proven free of lock contention / livelock.
**Contract:** `fn dispatch(jobs, cx, sources, budget) -> Vec<ExtractOutput>`.
**Storage/identity:** `ProjectCx` is `Sync` + borrowed; the lazy indexes are built
once (single-threaded) then read concurrently (`OnceLock` → `Arc`). The phase-1
cache is a content-keyed map; contention is the lab target.
**Tasks:**
4. `dispatch`: `jobs.par_iter()` → per-job `extract_file` → `resolve_project`
   (phase 2 needs the merged file set, so it runs after a barrier or per-blob with
   the prebuilt `cx`).
4.1 the cache: `DashMap<FileCacheKey, Arc<FileBundle>>` (or the engine-owned
   store cache) — LAB the contention on the hot path.
4.2 the contention lab: a many-small-files workload (the worst case for cache
   contention) vs a few-large-files workload; measure lock wait + throughput.
**Done:** dispatch wall-time scales near-linearly with workers to core count; no
contention cliff on the small-files workload.
**Golden:** dispatch 50k small files on N workers; assert throughput/worker is
flat (no contention regression) and RSS stays under the Epic-2 budget.

### Epic 5 — flow_edge promotion + term-op unity
**Goal:** (a) promote v5's stdlib `flow_edge` to typed `FlowEdgeKind` edges in the
type system; (b) unify the term-extract ops under the same arena + cache.
**Contract** (seed `_0_shape::FlowEdgeKind`, `_5_term`): the five flow variants;
the `Extractor`/`Registry` term seam sharing `ParseStrings`.
**Tasks:**
5. `flow_edge` lift: implement the arg→param / ret→call_res / lambda hops as
   `EdgeKind::Flow(_)` projection in `resolve_project`, replacing the `std/flow.dl`
   stringly-joined rels. Parity: closure(flow_edge) byte-identical to v5 on
   `examples/flow-interproc.dl`.
5.1 term ops: port `sg`/`ast`/`json`/`regex` behind `Extractor`, arena-interning
   cells (`TermValue::Name(NameId)`, not `String`).
**Done:** flow_edge computed in extract, not `.dl`; term ops share the arena.
**Golden:** run the v5 `flow-interproc.dl` fixture's expected reachable-set
through v6's typed flow edges; exact match.

## Frontier (deferred decisions + the evidence each needs)

- **k-CFA / per-site callee cloning.** v5 stated it out of scope. Evidence needed:
  a real query that needs per-site precision (a taint query that over-approximates
  under the current call_target pin). Belongs in extract (call-site cloning) or
  the engine (query-time)? Decide on that query.
- **Node-level types.** v5: type_sig by-position, df_node by-name, function-level
  only. Evidence: a query that needs the type of a specific df node. Likely an
  extract-side `TypeRef` enrichment on df nodes.
- **CFG / dominators / use-def.** No family today. Evidence: a query that needs
  control-dependence or dominator trees (CodeQL-style). Decide extract-vs-engine;
  SVF says register def-use is syntactic (extract), memory def-use is points-to
  (engine/downstream).
- **ast-grep as a family extractor.** v5 used ast-grep for the term op only.
  Evidence: does lifting a family (e.g. a new lang) onto ast-grep patterns beat a
  tree-sitter extractor for dev cost? Lab in Epic 1.3.
- **The async-eval boundary syntax.** "Mark when not async" is a `sprefa-lang`
  decision for the engine/server layer, NOT this crate. This crate stays sync.

## Verification (standing rails, every epic)

- `cargo check` green on the seed (the type math) AND on the real crate once it
  exists; `cargo tree -p sprefa-extract` contains no `rusqlite`/`sqlx`/`tokio`/
  `axum`/`sea-orm` (extract is a leaf below store).
- extract's public API names no store-id type and no storage type (greppable;
  crate-map boundary rail).
- the RAM gun (Epic 2 golden): RSS independent of corpus size + path count.
- parity vs v5 (Epic 3 golden): byte-identical normalized node/edge sets.
- each epic ends with its golden test green before the next starts.

## Staffing

One agent per epic, worktree under `.claude/worktrees/`, base the extract crate on
the seed at `6dea0166` (branch `plan/extract-golden-plan`). Epics 1 + 2 can pair
(parser bench feeds the arena budget); 3 is the bulk; 4 + 5 follow. This plan +
the seed header are the spec — an agent should not re-derive types, only impl them.

<!-- todo(decision): oxc-vs-tree-sitter for JS/TS — Epic 1.3 bench decides; oxc wins on speed/arena, tree-sitter on lossless CST + shared FFI story -->
<!-- todo(decision): syn-vs-tree-sitter for Rust — syn gives proc-macro-grade accuracy + the existing v5 extractor; tree-sitter unifies the FFI story. Lean syn (port cost lowest) unless the lossless CST is needed -->
<!-- todo(perf): arena cap policy — skip-and-log vs spill-to-disk for a file over arena_bytes; the lab finds the real max single-file arena on the corpus -->
<!-- todo(decision): phase-2 placement — per-blob with prebuilt cx (current sketch) vs a post-barrier pass; the small-files workload in Epic 4 decides -->
<!-- todo(feature): the content-keyed phase-1 cache lives in extract, the engine, or the store? v5 had it in the engine (FactCache); v6 should pin it once -->
