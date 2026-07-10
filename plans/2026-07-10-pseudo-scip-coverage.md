# Pseudo-SCIP coverage: best syntactic wins per language

Research 2026-07-10 (Fable subagent, read-only). Question: what are the best wins
addable via tree-sitter or similarly cheap syntactic means to increase
compiler-grade-ish coverage per language, short of writing that language's compiler?
Companion to plans/2026-07-10-turnkey-query-surface.md (orthogonal: this is
extraction coverage, that is the query surface over it).

## Ground truth

| Fact | Evidence |
| --- | --- |
| TypeLang trait = {name, matches, extract, extract_calls, extract_dataflow} | src/typegraph.rs:336-349 |
| Kotlin unit (the tree-sitter template) ~1000 lines: df lift 453-900, edges+calls 1926-2200, entities/docs/fn arrows 3802-3990 | src/typegraph.rs |
| TsTypes matches .ts/.tsx ONLY; plain .js/.jsx has NO TypeLang despite oxc parsing JS natively | src/typegraph.rs:1580 |
| Name resolver: by_name keyed (repo, rev, name), unique-in-repo resolves, len>1 stays bare; SCIP override per (repo, file, name) at WORK only; does NOT consult module_edge | src/engine/extract.rs:695-725 |
| modgraph resolvers = Rust, TS, Kotlin only | src/modgraph.rs:130-137 |
| scip_occurrence (spans+roles) + scip_binding (alias binding text) shipped v0.6.24 (ledger S1/S2 answered at Tier 1) | src/rels/scip.rs:62-73 |
| tree-sitter-python 0.25, -go 0.25, -c already direct deps; the other 13 grammars come only via ast-grep-language which ships NO query files (no tags.scm/locals.scm) | Cargo.toml:83-119, src/sg.rs:11 |
| No stack-graphs / tags.scm / locals.scm usage anywhere in tree | grep, zero hits |

## Directions assessed

**A. Generic def/ref tags tier** (all 23 SG_LANG_TABLE grammars): syn_def/syn_ref
name inventory (or reuse type_entity/call_name shapes) for the ~12 zero-fact
grammars (java, csharp, ruby, php, scala, swift, c, cpp, lua, elixir, haskell,
bash). ast-grep-language crates do NOT bundle tags.scm; the honest build is dl's
own per-grammar def-node-kind table (10-25 lines each) + one generic walker
(SupportLang exposes the raw Language, tree_sitter::Query runs directly). Feeds
by_name unchanged. Does NOT get: type_sig, df_*, parents, doc association.
Effort ~0.4 Kotlin units total.

**B. Go TypeLang**: full rel family. Best determinism-per-line of any language:
method receivers carry their type in the syntax (`func (r *Repo) Name()`), no
overloading, one package per dir, exports = capitalization; GoResolver in modgraph
(~0.2 units) is deterministic (go.mod module line + dir). Does NOT get: implicit
interface-satisfaction edges (method-set computation = heuristic), cross-module
outside the workspace. Effort 1.2 units; grammar already a dep.

**C. Python TypeLang**: type_entity (class/def), call_def/call_site, df_*
(comprehensions/nested defs add walker cases), docstrings (easiest doc locator),
PyResolver (explicit imports, relative dots, __init__.py). SKIP type_link beyond
explicit bases/annotations; attribute-chain call resolution without types is where
every syntactic Python tool dies (emit bare callee, ambiguous stays bare).
scip-python already covers Python at Tier 1 when indexed; the TypeLang's unique
add is index-free operation + df_* (SCIP never provides dataflow). Effort ~1.2+
units; highest demand.

**D. Import-scoped ambiguity narrowing**: precision for all current+future
TypeLangs, zero new extraction. resolve closure (extract.rs:713) answers only
len==1 buckets today; when len>1, intersect the bucket with entities declared in
files reachable from the referencing file (module_edge targets + same file + same
dir/package), resolve only a unique survivor that is actually imported.
module_edge read from DB like scip_ref (extract.rs:506). Rescues the duplicated
utility names (new/Config/parse/Error) that stay bare today. RA-oracle precision
0.86 bounds the risk; gate hard. Effort ~0.15 units. NOTE: extract_input_digest
must fold module edges once resolution depends on them, or a module-graph change
won't re-resolve.

**E. Locals filtering**: for the 3 TypeLangs this is buildable TODAY as a dl rule
(anti-join call_site names against same-fn df_node param/let_bind vars); for
tags-tier langs it rides A's walker. Not a standalone engine item.

**F. stack-graphs**: SKIP. Upstream effectively frozen (GitHub code-nav
deinvestment); per-language .tsg files are multiple Kotlin units each (the TS one
alone is thousands of lines); scope-graph path search does not feed the by_name/sym
architecture (second resolver + reconcile). Beats B+D only on alias-heavy
intra-file resolution, which scip indexers already cover.

**G. Receiver-type lite**: already partially present (Rust impl context, Kotlin/TS
receiver-into-result df edges); Go gets it free. Fold into each TypeLang, not
standalone.

**H. JS/JSX through the existing TsTypes** (found during grounding): matches is
.ts/.tsx only; oxc parses plain JS with SourceType::jsx(). Extending matches to
.js/.jsx/.mjs/.cjs (+ the SourceType pick at the parse sites + the engine's
file-selection globs) buys type_entity/call_*/df_* (incl. the JSX dataflow work) +
doc_comment for the single most common un-indexed language at near-zero cost.
type_link/type_sig stay thin (no annotations), honest and expected. Effort ~0.05
units. Nothing else claims .js in the registry.

## Ranked shortlist (coverage per effort)

| # | Item | Size | Kotlin units | Buys |
| --- | --- | --- | --- | --- |
| 1 | H: JS/JSX via TsTypes matches extension | S | 0.05 | full call/df/entity/doc for plain JS |
| 2 | D: import-scoped ambiguity narrowing | S | 0.15 | precision for all TypeLangs, rescues len>1 buckets |
| 3 | B: Go TypeLang + GoResolver | M | 1.2 | full rel family, near-deterministic |
| 4 | A: generic def/ref tags tier | M | 0.4 | name inventory for ~12 zero-fact grammars |
| 5 | C: Python TypeLang + PyResolver (type_link scoped out) | L | 1.2+ | entities/calls/df/docstrings, index-free |
| 6 | F: stack-graphs | L | 3+/lang | skip |

**Single best next step**: H + D together (one small PR, ~0.2 units combined).
