# sprefa-extract: tree-sitter, ast-grep and CST query paths, as read on 2026-09-15

Read-only survey by a sonnet lane. Every path is relative to the cited repository root; `git show <HEAD>:<path>` reproduces the file.

## Citations base

| repo | HEAD | branch | describe | dirty files |
|---|---|---|---|---|
| /Users/chrishafley/projects/hafley-rs | f0f3136b2b663729abd2c51782d94e4f2cec774e | main | mac-bins-2026-09-14-79-gf0f3136b | 1 |

All paths below are under `crates/sprefa-extract/`.

## 1. Entry points (`--family`, `src/bin/extract.rs`)

| `--family` value | context | dispatches to | path:line |
|---|---|---|---|
| `cst` | per-file mask | `mask.cst = true` -> `dispatch()` -> `Source::extract` | src/bin/extract.rs:963; src/dispatch.rs:48 |
| `type`/`types` | per-file mask | `mask.types = true` | src/bin/extract.rs:964 |
| `call` | per-file mask | `mask.call = true` | src/bin/extract.rs:965 |
| `df` | per-file mask | `mask.df = true` | src/bin/extract.rs:966 |
| `data` | per-file mask | `mask.data = true` | src/bin/extract.rs:967 |
| `cfg` | per-file mask (derived) | turns `mask.cst` on, `cfg` flag set separately | src/bin/extract.rs:969, 778 |
| `scip` | whole-project mode | `stream_scip_family` -> `scip_family_jsonl`/`scip_family_from_index_jsonl` | src/bin/extract.rs:374,411,439-440; src/project.rs:1239,1245 |
| `diet_scip` | whole-project mode | `stream_resolve` -> `resolve_project_with_raw`/`resolve_project_jsonl` | src/bin/extract.rs:375,905,927,932; src/project.rs:272,1050 |
| `call`/`type`,`types`/`flow` (under `--resolve`) | resolve arms | `parse_arms` sets `ResolveArms{call,types,flow}` | src/bin/extract.rs:940-961 |

Runtime-pattern path outside `--family`: `--ast-pattern ID=PATTERN` (+`--ast-selector`, `--ast-capture`) conflicts with `family` and calls `stream_ast_queries` -> `query_patterns`. src/bin/extract.rs:233-258, 766-769, 890-899.

Per-file entry beneath every mask family: `dispatch(path, content, mask)` -> `source_for(path)` -> `src.extract(path, content, mask)`. src/dispatch.rs:48-68; src/lang/mod.rs:110-128.

## 2. Parsers (`src/lang/`)

| file | parser lib | Cargo.toml crate:line | parse call path:line |
|---|---|---|---|
| rust.rs | syn | `syn = "2"` Cargo.toml:65 | `syn::parse_file` src/lang/rust.rs:3267, 3387 |
| go.rs | tree-sitter (tree-sitter-go) | `tree-sitter = "0.25"` :100; `tree-sitter-go = "0.23"` :101 | src/lang/go.rs:56-58 |
| kotlin.rs | tree-sitter (tree-sitter-kotlin-sg) | `tree-sitter-kotlin-sg = "0.4"` :111 | src/lang/kotlin.rs:59-61 |
| ts.rs | oxc (arena AST) | `oxc_parser = "0.135"` :47 | `oxc_parser::Parser::new(..).parse()` src/lang/ts.rs:89 |
| markdown/_0_source.rs | tree-sitter (tree-sitter-md, block+inline) | `tree-sitter-md = "0.5"` :135 | src/lang/markdown/_0_source.rs:22-23, 87, 125 |
| data/_0_source.rs | tree-sitter (json/yaml/toml-ng) | `tree-sitter-json = "0.23"` :123; `tree-sitter-yaml = "0.7"` :124; `tree-sitter-toml-ng = "0.7"` :125 | src/lang/data/_0_source.rs:78-88 |
| python/_0_source.rs | tree-sitter-python | `tree-sitter-python = "0.23"` :106 | src/lang/python/_0_source.rs:66-68 |
| astgrep.rs | ast-grep-core + ast-grep-language | `ast-grep-core = "0.38"` :37; `ast-grep-language = "0.38"` :38 | `AstGrep::new(source, lang)` src/lang/astgrep.rs:69, 164 |
| 1_ast_rule.rs | ast-grep-config (`RuleConfig`, `from_yaml_string`) | `ast-grep-config = "0.38"` :39 | `from_yaml_string` src/lang/1_ast_rule.rs:10, 192 (`decode_ast_rule_yaml`) |
| 2_source_query.rs | tree-sitter `Query`/`QueryCursor` | `tree-sitter = "0.25"` :100 | `TreeParser::new`, `Query::new`, `QueryCursor::new` src/lang/2_source_query.rs:132-146, 214 |
| 3_source_facts.rs | facade over `query_patterns`/`query_ast_rule`/`query_tree_sitter_spans` | n/a | src/lang/3_source_facts.rs:12-16, 212 |
| 4_owned_region.rs | none, marker scan | n/a | src/lang/4_owned_region.rs:1-60 |

## 3. ast-grep

| question | answer | path:line |
|---|---|---|
| crates | `ast-grep-core`, `ast-grep-language`, `ast-grep-config`, all `0.38` | Cargo.toml:37-39 |
| smallest pattern | `AstPatternQuery { id, pattern, selector: Option<String>, captures: Vec<String> }`; `Pattern::try_new(&query.pattern, lang)` or `Pattern::contextual(..)` | src/lang/astgrep.rs:33-38, 69-77 |
| runtime pattern, simple | yes: `--ast-pattern ID=PATTERN`, `--ast-selector ID=KIND`, `--ast-capture ID=NAME`, repeatable | src/bin/extract.rs:233-258, 819-865 |
| runtime composed rule (all/any/not/inside/has/follows/precedes) | library only: `decode_ast_rule_yaml`; no CLI flag; no caller in `src/bin/extract.rs` | src/lang/1_ast_rule.rs:18-46, 192 |
| rows, simple pattern | `AstCaptureFact { record, query, capture, text, start, end, match_start, match_end }` | src/lang/astgrep.rs:43-51 |
| rows, composed rule | `AstRuleMatch { record, query, path, content: ContentId, span, captures: Vec<AstRuleCapture>, proposal }`; `AstRuleCapture { name, text, span }` | src/lang/1_ast_rule.rs:76-89 |

## 4. CST, source query, owned region

| name | does | input | output | path:line |
|---|---|---|---|---|
| `CstProjector` | pre-order DFS over the ast-grep tree, one row per named node, `Child` edge to nearest named ancestor | `SgRoot` | `Node<CstF>`, `Edge<CstF>` in `FamilyBundle<CstF>` | src/lang/astgrep.rs:174-217 |
| `query_source` | dispatches one of three engines (`TreeSitter`/`AstPatterns`/`AstRule`) over caller bytes | `SourceQuery` + path + content | `SourceQueryOutput` | src/lang/2_source_query.rs:95-111 |
| `query_tree_sitter_spans` | raw tree-sitter `Query` with `sprefa-match?`/`sprefa-not-match?`/`sprefa-eq?` predicates | `TreeSitterQuery{language,query}` + content | `Vec<TreeSitterSpannedMatch>` | src/lang/2_source_query.rs:134-149, 210-260; structs :25-63 |
| `query_source_facts` | wraps any engine output in `SourceQueryFacts` with git and content identity | path, content, `SourceQuery`, `SourcePlace` | `SourceQueryFacts { protocol, source, content, byte_length, git_blobs, parse, .. }` | src/lang/3_source_facts.rs:134-145; src/lang/mod.rs:81-86 |
| `find_owned_region`/`propose_owned_region` | bounds a generated text region by `sprefa:auto-begin/end <id>` markers | bytes, region id | `OwnedRegion{id,start,end,current}`, `OwnedRegionProposal{region,expected: ContentId,replacement}` | src/lang/4_owned_region.rs:8-21, 42-60 |

Tests: `tests/30_ast_rule.rs:179,138,230`; `tests/3_ast_pattern_cli.rs:6,52`; `tests/31_owned_region.rs:6,114`.

No `cst_edge` name exists; the CstF edge kind is `CstEdgeKind::Child` (src/types.rs:191-197), on the wire `FlatFact::Edge{family: Cst, kind:"child",..}` (src/types.rs:3025-3035).

## 5. Output row structs (`src/types.rs`)

| struct | fields | path:line |
|---|---|---|
| `TypeEntityKind` | Struct, Enum, Class, Interface, Alias, Function, Method, Const, Ext(LangKind) | src/types.rs:209, 225-237 |
| `TypeEdgeCandidate` | owner: Span, to: NameId, kind: TypeEdgeKind | src/types.rs:348-353 |
| `CallSite` | | src/types.rs:559, 632-640 |
| `FlowEdge` | src_blob: ContentId, src_span: Span, dst_blob: ContentId, dst_span: Span, kind: FlowEdgeKind | src/types.rs:1241, 1279-1286 |
| `DocFact` | owner: Span, parent: Option<NameId>, text: NameId, tags: Vec<DocTag> | src/types.rs:357-364 |
| `DocNode` | span, kind: DocNodeKind, name, parent, target, title, body: Option<Span> | src/types.rs:376-386 |
| `Node<F>` | span: Span, kind: F::NodeKind, name: Option<NameId> | src/types.rs:1589-1596 |
| `Edge<F>` | src: NodeRef, dst: NodeRef, kind: F::EdgeKind | src/types.rs:1615-1622 |
| `FamilyBundle<F>` | nodes, edges, aux: F::Aux | src/types.rs:1635-1640 |
| wire `FlatFact::FileRow` | path: String, digest: String, bytes: u32, lines: u32 | src/wire.rs:582-587 |
| wire `FlatFact::Node`/`Edge` | fact: Option<u32>, family: FamilyTag, span/from/to: SpanOut, kind: String, name: Option<String> | src/types.rs:3016-3035 |

## 6. Spans

| item | fields | unit | path:line |
|---|---|---|---|
| `Span` | start: u32, len: u32 | byte | src/types.rs:42-56 |
| `SpanOut` | start: u32, end: u32 | byte, half-open | src/types.rs:2989-2993 |
| `ByteRange` | start, end | byte | src/lang/3_source_facts.rs:89-93 |
| `TreeSitterSpannedMatch` | pattern, start, end, line, end_line (1-based) | byte plus line | src/lang/2_source_query.rs:56-63, 260-268 |
| `AstCaptureFact` | start, end, match_start, match_end | byte | src/lang/astgrep.rs:43-51 |

`Node<F>`/`Edge<F>` carry neither path nor blob id; `FlowEdge` carries `ContentId` (blake3); `FlatFact::FileRow` is the one row with a path string, one per file per stream (src/wire.rs:574-588).

## 7. Gaps for "ast-grep pattern at runtime, rows back"

- Simple pattern with metavariable captures is wired end to end: `--ast-pattern`/`--ast-selector`/`--ast-capture` -> `parse_ast_queries` -> `AstPatternQuery` -> `query_patterns` -> `AstCaptureFact`. src/bin/extract.rs:233-258, 819-865, 890-899; src/lang/astgrep.rs:56-124.
- Composed rules decode from YAML (`decode_ast_rule_yaml`) with no CLI flag. Gap: src/bin/extract.rs, a `--ast-rule`/`--ast-rule-file` flag; library ready at src/lang/1_ast_rule.rs:192, 298.
- No `--family` reaches `query_source`/`query_source_facts`/`query_tree_sitter*`; `grep -n "query_source\|SourceQuery" src/bin/extract.rs` is empty.
- `--ast-pattern` mode takes one file per invocation (`stream_ast_queries`, src/bin/extract.rs:890-899); no batch flag yields a path-tagged row stream over many files.
- No sibling crate calls `decode_ast_rule_yaml`/`AstRuleRequest`/`query_ast_rule`; the composed-rule path is used only by this crate's tests.
