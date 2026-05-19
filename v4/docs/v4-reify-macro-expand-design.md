# reify — macro-expand mechanism + extraction crate (design)

Builds on `v4-reify-op-design.md` (type = Pipe + DotTable, no new
ValueKind, ast/ast-grep untouched) and `v4-u1-entity-graph-plan.md`.
This doc covers two coupled things:

- D1: how the generated sprf region is written, located, hashed,
  re-parsed, and EXECUTED (a true macro: expand + run).
- D2: a new Rust crate that does tree-sitter extraction of
  types/fields/modules/imports, mirroring sem-core's plugin shape,
  reusing v1/v2 prior art.

Code citations are `path:line`. Nothing in this doc modifies source.

---

# D1 — macro-expand mechanism

## Chosen mechanism (1 paragraph)

**Rewrite file → re-parse**, NOT a parse-time AST splice. The `reify`
op's lower records `(invoke_site_byte, lang, selector)`; a pre-run pass
extracts entities, emits sprf text, and rewrites a marker-delimited
region in the `.sprf` file immediately after the invoking statement;
then `run` proceeds normally and `host_parse` (`compile/parse.rs:22`)
sees the spliced region as ordinary source. The region is real sprf and
flows through the unmodified `host_parse → walk_program → expand`
pipeline, so "expand + run" needs zero new execution path.

### Why rewrite-then-reparse, not parse-time splice

| dimension            | rewrite file → reparse                   | parse-time AST splice                          |
|----------------------|------------------------------------------|------------------------------------------------|
| pipeline change      | none; `host_parse` re-reads from disk    | new splice step inside `host_parse`/`walk`     |
| spans                | natural; region bytes are real file bytes | synthetic ranges; rebase logic like brace path (`parse.rs:259` `rebase_op_call`) |
| diffability          | generated sprf is on disk, reviewable    | invisible; lives only in memory                |
| idempotency proof    | byte compare of region                   | must re-derive every parse                     |
| LSP/editor coherence | file = truth, editor reload trivial      | editor and AST disagree                        |
| failure blast radius | a bad emit = a bad file = normal diag    | a bad emit corrupts the in-memory program      |
| extra cost           | one fs read of one file already in cache | none                                           |

The only cost of rewrite-then-reparse is re-reading the `.sprf` file
(already happening: `app.rs:1594` `read_to_string`, `app.rs:1598`
`host_parse`). The pre-pass slots in before line 1598. Splice would
duplicate the brace-reparse machinery (`parse.rs:165`
`lower_brace_block`, `parse.rs:259` `rebase_op_call`) with synthetic
spans for no gain. Pick rewrite.

## Region markers

Single-line sprf comments (`#` is the host line-comment;
`rewrite_quote_strings`/`needs_trailing_semi` already treat `#` at host
level as a comment, `parse.rs:462`, and `collect_diags` ignores
comments, `walk.rs` step kind `line_comment` is skipped at
`parse.rs:116`). Markers:

```
#<reify gen site=<site_id> sel=<selector_b64> h=<hash16>>
... generated sprf statements ...
#</reify gen site=<site_id>>
```

- `site_id`  = stable id of the invoke statement (see "site location").
- `selector_b64` = base64 of `lang + dsl selector` (regen key input).
- `hash16` = first 16 hex of xxh3 over the *normalized entity IR*
  (NOT over emitted text — text is derived, IR is truth; mirrors
  sem-core `structural_hash`, `sem-core/src/utils/hash.rs:18`).

A region is a contiguous run of full-line statements between the two
marker comment lines. Everything between markers is owned by the
generator; manual edits there are discarded on next run.

## Site location (relative to invoke)

The reify statement is one top-level pipe (`PipeAst`, `parse.rs:66`
iterates `root.named_children` for `pipe` nodes; statement order =
source order). Site id is derived at lower time:

```
site_id = xxh3_16( normalized_invoke_text )    # lang + selector + op span text
```

Region placement: the line immediately AFTER the `;` that terminates
the reify statement's pipe. Location algorithm on the file text:

1. find the reify statement's source span (`OpCall.span`,
   `ast.rs`/`parse.rs:153`; the op already carries `span: ByteRange`).
2. scan forward to the next `;` at host depth → end of statement.
3. if the next non-blank line is `#<reify gen site=<site_id> …>`,
   that is the existing region; else region is absent.

Absent → insert fresh region after the statement line. Present with
matching `site_id` → candidate for in-place replace.

## Idempotency + manual-edit guard

```
oldh = parse h=<…> from the begin marker (if region present)
newh = region_hash(entities)              # xxh3 over normalized IR
if region_present && oldh == newh:
    NO-OP                                  # generated text untouched
else:
    replace [begin_marker ..= end_marker] with freshly emitted text,
    begin marker carries newh
```

- Regen trigger is `oldh != newh` ONLY. `oldh` lives in the file; `newh`
  comes from the entity IR which sem-style caching keys on file
  content_hash (`archive cache/src/sqlite_store.rs:247` upsert keyed
  `(repo_id, path, content_hash)`; skip-set at `:675`). Unchanged
  sources ⇒ same IR ⇒ same `newh` ⇒ no-op.
- Manual edit inside the region: the editor changed bytes but the begin
  marker still says `h=oldh`. Next run recomputes `newh` from IR; if IR
  unchanged `newh == oldh` and we *still no-op* (we trust the marker
  hash, not a re-hash of region body). To make manual edits self-heal,
  v1 also stores `body16 = xxh3_16(region_body)` in the end marker:
  `#</reify gen site=<id> b=<body16>>`. On run, if
  `xxh3_16(current_body) != body16` the region was hand-edited →
  force regenerate even when `oldh == newh`. The IR is truth; a manual
  edit is overwritten, loudly (emit a `reify/region-clobbered` info
  diag with the region span).

## Entry into the existing compile pipeline

No new pipeline. Insertion point is a pre-pass in `run`
(`app.rs:1586` `async fn run`), between:

- `app.rs:1594` `let src = read_to_string(&req.path)`
- `app.rs:1598` `let (program, parse_diags) = host_parse(&src)`

Pre-pass shape:

```
src   = read_to_string(req.path)                       # app.rs:1594 (unchanged)
sites = prescan_reify_sites(&src)                       # cheap regex/line scan
if !sites.is_empty():
    new_src = expand_reify_regions(src, sites, &cfg)    # crate call + rewrite
    if new_src != src:
        write(req.path, &new_src)                       # file = truth
        src = new_src
(program, parse_diags) = host_parse(&src)               # app.rs:1598 (unchanged)
```

`prescan_reify_sites` does NOT need the full walker: a reify invoke is
syntactically `reify(:lang)\`selector\`` at statement head; a line scan
(or a throwaway `host_parse` to get `OpCall` spans where
`op.name == "reify"`) finds them. Reusing `host_parse` once for
discovery is fine and avoids re-implementing tokenization; the authored
selector is `op.dsl` (`parse.rs:142` `node.child_by_field_name("dsl")`)
and lang is the bracket/paren arg.

The same pre-pass must also run on the LSP `ingest` path
(`app.rs:711` second `host_parse` call site) for editor coherence;
v1 may scope LSP to "regenerate-on-save only" and keep `ingest`
read-only. Open decision O3.

`ReifyDef::lower` itself stays trivial: it records the site (already
specced in `v4-reify-op-design.md` Layer 2) and lowers to a no-op /
pass-through pipe so the reify statement contributes nothing to
execution; all generation is the pre-pass.

## Ordering guarantee (with evidence)

Claim: a generated `rule(:A,…)` declaration is lowered before any
statement that reads `t.A` / queries `A?(…)`.

Evidence chain:

1. The region is written **immediately after** the reify statement
   line (site location step, above). So in file byte/line order the
   generated `rule(:A,…)` statements precede every statement the user
   wrote *after* `reify`.
2. `host_parse` produces `Vec<PipeAst>` in source order:
   `parse.rs:66` `for child in root.named_children(&mut walker)` pushes
   one `PipeAst` per top-level `pipe` in tree order = source order.
3. `walk_program` lowers in `Vec` order: `walk.rs:56`
   `for p in program { walk_pipe(p, …) }` — no reordering, single pass.
4. Rule declarations register their columns as they are walked. A
   later statement that does `A.field` / `A?(…)` resolves via
   `ctx.store.declared_cols(head)` (`walk.rs:716`,
   `walk.rs:395 declared_cols`) which only returns non-empty AFTER the
   `rule(:A,…)` decl walked. Pass order = decl-before-use holds because
   the region (hence the decl) sits before any subsequent reader in the
   `program` Vec.
5. Caveat: a statement *before* the reify call cannot reference `t.A`
   (it precedes the region). That is correct macro hygiene — the type
   is in scope only after the reify that produces it. Document this as
   a hard rule, not a bug.

Conclusion: file statement order is sufficient; no topological pass is
needed. The existing single-pass `walk_program` ordering
(`walk.rs:56`) already gives the guarantee because the splice lands
before any user code that can read the type.

## D1 planning-protocol

### signatures

```rust
struct ReifySite { id: u64, lang: Arc<str>, selector: Arc<str>,
                   stmt_end_byte: usize }

fn prescan_reify_sites(src: &str) -> Vec<ReifySite>;
fn region_hash(ents: &[TyEntity]) -> u64;          // xxh3, IR-normalized
fn emit_sprf(ents: &[TyEntity]) -> String;         // see D2
fn locate_region(src: &str, site: &ReifySite)
        -> Option<std::ops::Range<usize>>;          // byte range incl markers
fn splice_region(src: &str, site: &ReifySite,
        body: &str, h: u64, body16: u64) -> String; // returns new file text
fn expand_reify_regions(src: String, sites: &[ReifySite],
        cfg: &ReifyCfg) -> std::io::Result<String>;
```

### pseudo bodies

```
expand_reify_regions(src, sites, cfg):
  out = src
  for site in sites (ascending byte; recompute spans after each splice):
    ents  = sem_like_extract(site.lang, cfg.targets_for(site))   # D2 crate
    body  = emit_sprf(&ents)
    newh  = region_hash(&ents)
    match locate_region(&out, site):
      Some(r):
        (oldh, body16) = parse_markers(&out[r])
        if oldh == newh && xxh3_16(region_body(&out[r])) == body16:
            continue                                  # idempotent no-op
        out = splice_region(&out, site, &body, newh, xxh3_16(&body))
      None:
        out = splice_region(&out, site, &body, newh, xxh3_16(&body))
  Ok(out)
```

### lifetimes / state

- `ReifySite`: lives one `run`. Recomputed each run from src.
- region markers + hashes: persisted IN the `.sprf` file (durable,
  diffable, the only cross-run state for D1).
- entity IR cache: owned by the D2 crate (SQLite), keyed by file
  content_hash; D1 holds no entity state.

### storage / sequence / uniqueness

- one region per reify site, keyed `site_id`.
- splice ascending-by-byte and recompute later sites' `stmt_end_byte`
  after each edit (a splice shifts following offsets). Simplest: redo
  `prescan` after each splice (sites are few; O(reify_count²) on a
  tiny n is fine for v1).
- uniqueness: `(site_id)` unique per file; same IR ⇒ byte-identical
  body ⇒ no-op (idempotent regen).

---

# D2 — extraction crate

## Name + boundary

Crate name: **`sprefa-extract`** (reclaim the archive name; the archive
crate at `sprefa-archive-20260428/crates/extract` already owns exactly
this concept — trait + RawRef IR + kind constants).

```
 IN crate sprefa-extract
   - LangExtract plugin trait (sem-core SemanticParserPlugin shape)
   - TyEntity / TyField / TyRef IR (the field-level model sem lacks)
   - per-lang plugins: rust, ts (v1 slice = rust only)
   - tree-sitter field re-parse layer (struct -> fields+types)
   - SQLite entity cache keyed by file content_hash (sem-core shape)
   - structural_hash over normalized IR

 STAYS in v4
   - ReifyDef (OperatorDef): records site, no-op pipe        # thin op
   - the sprf emitter: TyEntity -> `rule(:A, x?: t.i64, …);`  # v4 knows sprf
   - the macro pre-pass: prescan/locate/splice/write          # D1
   - type-pipe builtins t.i64/t.u64/t (DotTable)              # v4 value model

 UNTOUCHED
   - ast / ast-grep ops; cst/dsls/ast/mod.rs (AstDsl)
```

Rationale for the boundary: the crate must not know sprf syntax (so it
stays a reusable extractor like sem-core); v4 must not know
tree-sitter grammars per language (that is the crate's job). The
`TyEntity → sprf text` emitter is the only place the two meet and it
lives in v4 because only v4 owns the `rule(...)`/`t.*` surface and the
DotTable type model.

## Public types/traits (Rust signatures)

```rust
// IR — the field/type layer sem-core does NOT have.
pub enum TyRef {                        // a type expression
    Prim(Arc<str>),                     // i64, u64, bool, str…
    Named(Arc<str>),                    // a user struct/enum name
    App(Arc<str>, Vec<TyRef>),          // Vec<T>, Option<T>, Map<K,V>
}
pub struct TyField { pub name: Arc<str>, pub ty: TyRef,
                     pub span: (u32,u32) }
pub enum RefKind { Calls, TypeRef, Imports, Exports, Module }  // sem RefType + 2
pub struct TyEdge { pub to: Arc<str>, pub kind: RefKind }
pub struct TyEntity {
    pub name: Arc<str>,
    pub kind: Arc<str>,                 // "struct"|"enum"|"module"|"import"
    pub file: Arc<str>,
    pub fields: Vec<TyField>,           // empty for non-aggregate kinds
    pub edges: Vec<TyEdge>,
    pub structural_hash: u64,           // xxh3, comment/whitespace-stripped
    pub span: (u32,u32),
}

// Plugin trait — mirrors SemanticParserPlugin (sem-core/src/parser/plugin.rs:3)
pub trait LangExtract: Send + Sync {
    fn id(&self) -> &str;
    fn extensions(&self) -> &[&str];
    fn extract(&self, src: &str, path: &str) -> Vec<TyEntity>;
}

// Registry + cache.
pub struct ExtractRegistry { plugins: Vec<Box<dyn LangExtract>> }
impl ExtractRegistry {
    pub fn with_defaults() -> Self;                 // rust (v1), ts (later)
    pub fn for_path(&self, path: &str) -> Option<&dyn LangExtract>;
}
pub struct EntityCache { /* SqlitePool */ }
impl EntityCache {
    pub fn open(db: &Path) -> rusqlite::Result<Self>;
    pub fn get_or_extract(&self, reg: &ExtractRegistry,
        path: &Path, content: &str) -> Vec<TyEntity>;   // skip on hash hit
}

// Demand entry — only kinds a reify selector asked for.
pub struct Demand { pub lang: Arc<str>, pub kinds: Vec<Arc<str>>,
                    pub roots: Vec<PathBuf> }
pub fn extract_demand(reg: &ExtractRegistry, cache: &EntityCache,
        d: &Demand) -> Vec<TyEntity>;               // filtered by d.kinds
```

`extract_demand` honors "demand-driven": it filters to the kinds the
reify selector requested (e.g. `kind: struct_item` → only `kind=="struct"`).

## Pseudo body (rust plugin, v1 slice)

```
RustExtract::extract(src, path):
  tree = tree_sitter_rust parse src                # crate owns the grammar
  for node in tree where node.kind in {struct_item, enum_item,
                                        mod_item, use_declaration}:
    match kind:
      struct_item:
        name   = child field "name"
        fields = for field_declaration child:
                   (name, parse_ty(type_node), span)
        push TyEntity{kind:"struct", fields, edges: ty_refs(fields)}
      use_declaration: push TyEntity{kind:"import", edges:[Imports]}
      mod_item:        push TyEntity{kind:"module", edges:[Module]}
    structural_hash = xxh3(normalized_token_stream(node))   # sem-core shape
```

`parse_ty` is the small re-parse layer sem lacks: it turns the
`type` node text into `TyRef` (`Vec < T >` → `App("Vec",[Named/Prim])`).

## Reuse table (prior art → verdict)

| path | extracts | reuse verdict |
|------|----------|---------------|
| `sprefa-archive-20260428/crates/extract/src/lib.rs:58` `trait Extractor` + `:31` `RawRef` + `:7 mod kind` (IMPORT_PATH/EXPORT_NAME/RS_USE/RS_MOD/IMPORT_ALIAS/EXPORT_LOCAL_BINDING) | the exact plugin-trait + flat-ref IR + kind taxonomy we want | **REUSE as template.** Adopt the trait shape and `kind` constants verbatim. `RawRef` is flat (no nested fields); WIDEN to `TyEntity` (adds `fields: Vec<TyField>`, `TyRef`). The crate name `sprefa-extract` is reclaimed from here. |
| `sprefa-archive-20260428/crates/rs/src/lib.rs:9` `RsExtractor` (`syn`-based; `extract_items` `:54`, use/mod/type at `:83-101`, `extract_path_attr` `:144`, `rewrite_module_refs` `:270`) | Rust `use`/`mod`/type-decl refs + qualified-path resolution | **REUSE logic, swap engine.** It is `syn`, not tree-sitter; v4 standardizes on tree-sitter via ast-grep (`cst/dsls/ast/mod.rs:18` SupportLang). Port the *node taxonomy and edge rules* (what counts as use/mod/declare, grouped-import flattening tested at `rs/src/lib.rs:692,823`), re-implement extraction on `tree_sitter_rust`. Do not pull `syn` into v4. |
| `sprefa-archive-20260428/crates/js/src/lib.rs:10` `JsExtractor` (oxc `module_record`: import_entries `:53`, local_export_entries `:135`, namespace/default/alias handling) | JS/TS import/export names, aliases, specifiers | **REUSE design, defer impl.** Best reference for the import/export *edge model* (NamespaceObject/Default/Name/alias-when-differs). v1 slice is rust-only; when ts lands, port this taxonomy onto `tree_sitter_typescript` (already a dep family in `ext/sem languages.rs:23`). Do not pull `oxc` in for v1. |
| `sprefa-archive-20260428/crates/index/src/extract.rs:24` parallel `extract` over `(abs_path, content_hash)` with skip-set (`:42`), `:32 Box<dyn Extractor>`, rayon `par_iter` | the cache-skip + parallel-run harness | **REUSE pattern.** Mirror its "pre-computed content_hash + skip_set short-circuit" loop for `EntityCache::get_or_extract`. |
| `sprefa-archive-20260428/crates/cache/src/sqlite_store.rs:247` files upsert `ON CONFLICT(repo_id,path,content_hash)`, skip-set load `:675 SELECT path,content_hash … WHERE scanner_hash=?` | SQLite content-hash cache + skip-set | **REUSE schema shape.** Adopt the `(path, content_hash)` keyed table + skip-set query for `EntityCache`. Simplify: drop repo_id/branch columns (v1 single-corpus). |
| `ext/sem/crates/sem-core/src/parser/plugin.rs:3` `SemanticParserPlugin` + `model/entity.rs:6` `SemanticEntity` + `parser/graph.rs:26` `EntityRef`/`:35 RefType` + `utils/hash.rs:18 structural_hash` | per-lang plugin trait, coarse entity, def/ref/import edges, Unison hash | **REUSE as the public-API model to mirror.** `LangExtract` ≅ `SemanticParserPlugin`; `RefKind` ≅ `RefType` (+Exports,+Module); `structural_hash` algorithm copied. `SemanticEntity` is too coarse (no fields, `entity.rs:6-24`) — that gap is exactly `TyField`/`TyRef`, the crate's reason to exist. |
| `sprefa/v3/crates/sprefa/src/ops/ast_grep.rs:41` `AstGrepFactory`, `:115 parse_lang` (rs/ts), `:125 scan_metavars` | generic ast-grep pattern matching, lang-name→SupportLang | **REUSE the lang map only.** `parse_lang` ("rust"|"rs", "typescript"|"ts" → SupportLang) is directly portable for selector lang parsing. The op itself is generic pattern match, NOT structured type/import extraction — not reusable as an extractor. |
| `sprefa/v3/crates/sprefa/src/readers/parse_cache.rs:64` `ParseCacheReader` (content-addressed by git oid `:70`, disk cache `:82`) | per-run tree-sitter parse cache | **REUSE idea, not code.** Confirms the "content-hash keyed parse cache" pattern; v4 `EntityCache` caches *entities* not trees, simpler. Reference only. |
| `sprefa-archive-20260428/crates/rules/src/extractor.rs:163` `impl Extractor for RuleExtractor` (json/yaml/toml via serde, `:590 parse_data`) | data-file (package.json/Cargo.toml) dep extraction | **REUSE later.** Not v1 (v1 = code structs). When `dep_name`/`dep_version` kinds are demanded, port `parse_data` for manifest files. |
| `sprefa-entity/v4/src/cst/dsls/ast/mod.rs:18` `AstDsl::new(SupportLang)`, `:25 lang` field | v4's existing tree-sitter-via-ast-grep lang handle | **REUSE the SupportLang plumbing**, but the crate links tree-sitter directly (not via the `ast` op) so `ast`/ast-grep stays untouched per the constraint. |

## D2 lifetimes / storage / uniqueness

- `ExtractRegistry`: process-static (`with_defaults()` once).
- `EntityCache`: one SQLite handle per `run` (or process; daemon may
  share). Table `entities(path TEXT, content_hash TEXT, kind, name,
  ir BLOB, structural_hash, PRIMARY KEY(path, content_hash))` — schema
  shape from `cache/src/sqlite_store.rs:247`.
- `TyEntity`: derived, never persisted as truth; the source file +
  content_hash is truth (re-extract on hash drift only).
- uniqueness: `(path, content_hash)` → entity set; identical content ⇒
  cache hit ⇒ no re-parse (the demand-driven skip).

---

# Open decisions

1. **O1 region body hash for self-heal**: store `b=<body16>` in the end
   marker (force-regen on manual edit) vs trust only IR hash (cheaper,
   manual edits silently survive until IR changes). Rec: store body16.
2. **O2 op name**: `reify` (current doc) vs `tsgen`/`astgen`. Rec keep
   `reify`.
3. **O3 LSP path**: run the macro pre-pass on `ingest` (`app.rs:711`)
   too, or regenerate-on-save only and keep `ingest` read-only. Rec:
   save-only for v1.
4. **O4 commit policy**: generated region checked in (diffable, marker)
   vs gitignored region. Rec: checked in (already the reify-op-design
   rec #3).
5. **O5 multi-site offset**: re-`prescan` after each splice (simple,
   O(n²) tiny) vs offset-fixup math. Rec: re-prescan for v1.
6. **O6 type-pipe v0 semantics**: validate-only vs coerce vs
   annotate-only (carried over from reify-op-design open #4;
   unaffected by D1/D2, listed for completeness).

---

# Smallest first slice (RED-testable vertical)

One language (Rust), one kind (`struct_item` → `rule`), region written
+ re-parsed + executed.

```
RED test: tests/reify_struct_to_rule.rs
  fixture A: src/types.rs  ->  pub struct Point { x: i64, y: i64 }
  fixture B: prog.sprf:
      reify(:rs)`kind: struct_item`;
      Point?(x?: X, y?: Y) > out(${X}, ${Y});

  expect after `run`:
   1. prog.sprf on disk now contains, right after line 1:
        #<reify gen site=<id> sel=<…> h=<…>>
        rule(:Point, x?: t.i64, y?: t.i64);
        #</reify gen site=<id> b=<…>>
   2. run executes: Point?(...) resolves declared_cols(Point)   # walk.rs:395
      (decl walked before the query because region precedes it)
   3. out rows = [(Point fields…)]  (entity materialized)
   4. second `run` with unchanged src.rs: region byte-identical,
      no rewrite (idempotent; assert file mtime/content stable)
   5. hand-edit inside region, re-run: region restored from IR,
      `reify/region-clobbered` info diag emitted
```

Crate scope for the slice: `sprefa-extract` with only `RustExtract`
handling `struct_item` (fields + prim `TyRef`), `EntityCache`
in-memory (SQLite optional in slice), `extract_demand` filtering
`kinds==["struct"]`. v4 side: `ReifyDef` recording site +
`emit_sprf` for the struct→rule form + the pre-pass wired between
`app.rs:1594` and `app.rs:1598`.

Order of build: (a) crate `RustExtract` + `TyEntity` + RED unit test
on extraction; (b) `emit_sprf` struct→`rule(...)`; (c) D1 pre-pass +
splice + the integration RED test above; (d) idempotency + clobber
guard.
