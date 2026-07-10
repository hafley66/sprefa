---
name: sprf-add-language
description: Adding a new language to dl. The per-surface junction map (TypeLang, module resolver, sg/ast grammars, comment/CST, SCIP indexer, --move rewriter), what each tier buys, and the LANG-JUNCTION comment convention that keeps this file honest. Load before wiring any new-language support.
---

# Adding a language to dl

## Why the shotgun surgery exists

There is deliberately NO central language enum. Language support is per-surface,
because the surfaces have different backends and wildly different costs:

- `sg`/`ast_yaml` ride ast-grep's `SupportLang` (a crate enum we don't control).
- `ast`, `comment_node`, and the CST `node`/`child` rels ride raw tree-sitter
  constructors compiled into the binary.
- The diet tier (`type_entity`/`call_*`/`df_*`/`doc_comment`) is a hand-written
  `TypeLang` walk per language: syn for Rust, oxc for TS/JS, tree-sitter for
  Kotlin. This is the expensive one ("Kotlin-sized", ~1000 lines).
- The real-SCIP tier shells out to an external indexer binary.
- Module resolution and `--move` rewriting encode each language's import and
  file-layout semantics, which don't generalize.

A central enum would force every language to answer every surface at once.
Instead languages land tier by tier, and each tier registers at its own
junction. The cost of that choice is that "add language X" is a scavenger hunt.
This skill is the map, and the map is machine-checked (see below).

## Tiers, cheapest first

| tier | junction slugs | buys | size |
| --- | --- | --- | --- |
| grammar rows | sg-grammars, ast-grammars, comment-cst-extensions | `sg`/`ast_yaml`/`ast` queries, `comment_node`, CST `node`/`child` | S (one row per table, grammar crate must exist) |
| real SCIP | scip-indexers | scip_def/ref/edge/occurrence/binding, `dl index`/`dl doctor`/`scip_want` | S (one Indexer row) |
| diet tier | typelang-registry, extract-file-set | type_entity/type_edge/type_sig, call_def/call_site/call_edge, df_* dataflow, doc_comment/doc_tag, all index-free | M-L (~1 Kotlin unit; exemplar = KotlinTypes in src/typegraph.rs) |
| module graph | module-resolvers | module_edge/module_unresolved/module_binding, resolver alias hop, import-scoped ambiguity narrowing | S-M |
| refactor | move-rewriter | `--move OLD=NEW` import rewriting for the language | M |

A language is useful at ANY prefix of this list. Go/Python shipped grammar rows
and the SCIP row long before their TypeLangs.

## The junction map (generated)

Every registration point carries a one-line marker comment in the source:

    // LANG-JUNCTION(<slug>): <what a new language wires here>

`examples/gen-lang-skill.dl` scans those markers with `comment_node` (grammar
backed, so the string "LANG-JUNCTION(" inside a string literal never counts)
and regenerates the list below. `dl examples/gen-lang-skill.dl --check` is the
drift rail: a marker added or removed without a regen fails; line-number drift
alone never fails. Never hand-edit inside the markers.

<!-- BEGIN: lang-junctions -->
- `src/cst.rs:155` comment-cst-extensions: the extension -> grammar-label map feeding `comment_node` and CST node/child extraction; a label here must exist in the ast-grammars table (`ts_lang` resolves it)
- `src/engine/extract.rs:532` extract-file-set: the extension LIKE list gating which files reach TypeLang extraction; a new TypeLang's extensions must be added to this SQL too or its extractor never sees a file
- `src/engine/mod.rs:7664` ast-grammars: one table row = `ast` op support (tree-sitter constructor keyed by label); `comment_node` and the CST node/child rels also dispatch through `ts_lang`, via `cst::lang_label_for_path`
- `src/lib.rs:640` move-rewriter: per-language `--move` path rewriting (rspath = Rust use-paths + mod surgery, ktpath = Kotlin package math); a new language adds its rewriter module and dispatches from this driver
- `src/modgraph.rs:136` module-resolvers: per-language import resolver registration; buys module_edge/module_unresolved/module_binding plus the name resolver's alias hop and import-scoped ambiguity narrowing
- `src/scip_setup.rs:50` scip-indexers: the real-SCIP tier; one Indexer row (marker files, binary, install hint, argv) = `dl index` / `dl doctor` / `scip_want` support for the language
- `src/sg.rs:6` sg-grammars: one table row = `sg` + `ast_yaml` op support for a grammar (canonical name, aliases, ast-grep SupportLang); the skill language matrix test asserts this table
- `src/typegraph.rs:351` typelang-registry: impl `TypeLang { name, matches, extract }` and register it here; buys type_entity/type_edge/type_sig/call_*/df_*/doc_comment for the language (the index-free diet tier, Kotlin-sized)
<!-- END: lang-junctions -->

## Diet-tier checklist (the M-L lift)

1. Read the exemplar end to end: `KotlinTypes` in src/typegraph.rs (tree-sitter
   based, one parse per file feeding entity + edge + call + dataflow + doc
   walks). TS/JS = `TsTypes` (oxc), Rust = `RustTypes` (syn).
2. Lines are 1-based everywhere; tree-sitter rows are 0-based, add 1 (this bit
   Kotlin once, see the ledger).
3. df_param skips the receiver (`self`/`cls`) so positions align with
   type_sig.pos; df_arg slots are 0-based; lambdas lift as their own fn scopes
   with the `::closure::` sym prefix; loops record spans so `nest` works.
4. Resolution is free: emitting entities and bare callee names feeds the
   existing by_name buckets, SCIP override, module_binding alias hop, and
   import narrowing. Ambiguous stays bare. Never invent a resolver inside the
   TypeLang.
5. Tests to mirror: typegraph unit tests plus tests/it/kotlin.rs (e2e fixture)
   and, when an indexer exists, a parity twin beside
   tests/it/oracle_rust.rs (confirmed-positives-only scoring: parity counts a
   call only when the compiler index agrees; unconfirmable is excluded, never
   counted for or against).
6. After wiring, `dl what <name>` on a fixture is the smoke test that every
   junction actually fired (entities, calls, module rows in one answer).
