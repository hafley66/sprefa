# Ruling: STRING

Lower the new S-expression structural-query sugar to a tree-sitter query string, reuse the existing `ts_query/1` compiler, and run it through the extract host. Do not rebuild tree-sitter's matcher as dl6 joins.

The phase-1 compiler already does exactly this: `v6/prolog/1_host_expand.pl:154-158` dispatches `ts_query(Term)` to `compile_ts_query/2`, and `:414-478` serializes `node/field/capture/quant/alternative/wildcard/predicate` terms into tree-sitter query text, throwing `unmapped_feature` for unsupported shapes (`:418-422`, `:473-474`). `v6/prolog/compile/registry.pl:193` registers `ts_query/1` as a live world value, and `v6/prolog/conformance/fixtures/2_hosts_wiring.pl:200-242` exercises it through a `tree_sitter(file_digest, query) -> capture` host. The v5 runner is already there: `src/engine/eval.rs:1047-1078` (`run_ts`) parses, runs the query, and returns captures.

`node`/`child` are the wrong foundation for a faithful tree-sitter matcher. `src/engine/decls.rs:882-887` keeps `child(parent, child)` at exactly two columns so `closure(child)` gives ancestry; it carries no field names and no sibling order. `src/cst.rs:7-9` and `v6/sprefa-extract/src/lang/astgrep.rs:171-199` emit only named nodes, reparenting anonymous children. Fielded patterns, anchors (`.`, `^`), and exact sibling-order semantics cannot be expressed faithfully over that schema without extending it. `v6/plans/2026-07-23-v5-surface-audit.md:178-180` already records the decision that v5 file-form extraction ops (`ast`, `match_ast`, `ast_yaml`) survive as extractor capabilities, not as native joins.

The S-expression parser is cheap by the DCG precedent in `v6/prolog/compile/parse_dl.pl:95-120` and `:1464-1483`; the remaining work is surfacing the sugar and wiring the runtime host, not reimplementing matching.

Pushdown and refusals are not lost. File/rev filters can still be pushed into the probe demand rule before the host is called, and `compile_ts_query` throws `unmapped_feature` for unsupported pattern shapes. What crosses the host boundary is the query text, so the compiler cannot push dynamic filters from other relations into the matcher.

# Steelman against the ruling

The concrete failure case is a structural pattern whose selectivity comes from another, dynamic relation.

S-expression pattern:

```
(identifier) @callee
```

Rule body that also binds `Callee` from a small relation `deprecated(Callee)`.

With STRING, the compiler emits one static tree-sitter query, the host returns every `identifier` capture, and the rule filters afterwards. Tree-sitter `#eq?` predicates are static (capture vs literal or capture vs capture), so the compiler cannot push the dynamic `deprecated` set into the query. Work is proportional to all identifiers in the corpus, not just the deprecated ones. If `deprecated` has 10 rows in a corpus with 10^5 identifiers, STRING materializes and joins 10^5 rows where a join-first plan could start from 10 names.

# Falsifying experiment

1. Build a synthetic source file with 100,000 `identifier` nodes, only 10 of whose texts appear in `deprecated(name)`.
2. Program A (STRING): structural query `(identifier) @callee`, probe the `tree_sitter` host, join with `deprecated(Callee)`.
3. Program B (JOINS): `deprecated(Name), node(Id, "identifier", File, Lo, Hi, _), ref(_, Sid, File, Lo, Hi), string(Sid, Name, _)` (add a temporary `field` relation if the pattern needs field constraints).
4. Measure wall-clock and the intermediate capture row count.

Pass criterion for STRING: Program A is within 10x of Program B, and the intermediate capture count is < 1,000x the final result count.

Fail criterion: Program A is >10x slower, or the intermediate count is >1,000x the final count. Either result flips the ruling to JOINS (or to a hybrid that compiles filter-bound patterns to joins).

# Cost

Files touched / new machinery required for STRING:

- **S-expression parser**: add to `v6/prolog/compile/parse_dl.pl` (DCG precedent at `:1464-1483`) or to `v6/dl/grammar/dl.langium` + `v6/dl/src/0_ast_bridge.ts` for the JS door. The term form already exists; the parser only surfaces it as text.
- **Lowering**: reuse `v6/prolog/1_host_expand.pl:compile_ts_query/2` (`:414-478`) and its refusal clauses (`:418-422`, `:473-474`). Extend `ts_pattern_text` only if the sugar exposes anchors (`anchor(before)` -> `.`, `anchor(first)` -> `^`).
- **Registry**: keep/update `v6/prolog/compile/registry.pl:193` (`surface(ts_query/1, ...)`).
- **Runtime host**: wire the `tree_sitter` host from `v6/prolog/conformance/fixtures/2_hosts_wiring.pl:202-205` to a real runner. `SYNTAX.md:330` notes v6 phase-2 host execution is currently `unsupported_host_execution_phase_2(tree_sitter_query)`, so this cost is real: either port v5's `run_ts` (`src/engine/eval.rs:1047-1078`) or add a thin tree-sitter query wrapper to `v6/sprefa-extract` (the crate currently exposes ast-grep via `v6/sprefa-extract/src/lang/astgrep.rs:54-129`, not raw tree-sitter queries).
- **No change** to `src/engine/decls.rs:node_rel_decls()` or `src/cst.rs`; the `node`/`child` schema stays as-is.

# Semantics inventory

| feature | STRING handling |
|---|---|
| fields | emitted as `name: pattern` by `ts_pattern_text(field(Name, P), ...)` (`1_host_expand.pl:438-441`) |
| anchors (`.`, `^`) | not in current `ts_query/1` vocabulary; add `anchor/1` terms and serialize, else refuse via `slot_ts_pattern_form` (`:473-474`) |
| quantifiers `?` `*` `+` | emitted by `ts_quantified` (`:461-478`) |
| alternations `[...]` | emitted by `ts_pattern_text(alternative(Ps), ...)` (`:467-470`) |
| wildcards `_` / `(_)` | emitted as `_` and `(_)` (`:471-472`) |
| error nodes | supported as `node('ERROR', [])` because `node(Type, ...)` accepts any atom (`:430-437`) |
| predicates `#eq?` `#match?` | emitted (`:453-460`); other predicates refused |
| captures `@name` | emitted (`:442-448`); host returns one row per capture |
| ast-grep metavariables `$X` | refused: `sg_pattern/3` -> `unmapped_feature(slot_sg_metavariable_semantics)` (`:419-420`, `registry.pl:194`) |
| anonymous literals | emitted as quoted strings by `ts_pattern_text(anonymous(Value), ...)` and `ts_pattern_text(string(Value), ...)` (`:449-452`) |
