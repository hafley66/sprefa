# Tree-sitter emitter probe

## Verdicts

| Phase | Verdict | Receipt |
|---|---|---|
| A, complete hand grammar | PASS | `golden-flex.dl6`: 630 lines, zero `ERROR` or `MISSING`; generated corpus: `TS_CORPUS total=266 clean=266 errors=0`; Topiary law and idempotence pass. |
| B, DCG emitter | FAIL | 108 DCG clauses and 75 DCG rule names yield 34 mechanically translatable clauses. The emitted file has 1,407 non-whitespace characters; the required hand overlay has 3,862. The overlay grows with declarations, expressions, patterns, and statement forms. |
| C, LSP candidate analysis | COMPLETE | Candidate and capability tables below. |

Phase B is an honest failure of the one-description test. `parse_dl_dcg.pl` contains enough concrete terminals to seed an emitter, while its parameterized combinators, character predicates, semantic actions, and operator registry do not determine the Tree-sitter grammar. The measured overlay is 2.74 times the emitted probe.

## Phase A gate

Run from this directory:

```text
$ ./run-tests.sh
PASS parse: golden-flex.dl6 lines=630 errors=0
TS_CORPUS total=266 clean=266 errors=0
PASS format: formatting law and idempotence
$ echo $?
0
```

The corpus is generated first with `cd v6 && just text-door`; the observed count was 266.

### Precedence and ambiguity overlays

| Level | Tree-sitter declaration | Construct |
|---:|---|---|
| 1 | `prec.right` | `:=` and `is` binding; right association keeps the complete right expression. |
| 2 | `prec.left` | comparison operators. |
| 3 | `prec.left` | additive `+` and `-`. |
| 4 | `prec.left` | multiplicative `*`, `/`, and `mod`. |
| 5 | `prec` | unary `+` and `-`. |
| 6 | `prec` / `prec.left` | calls and member access. |

The grammar also declares GLR conflict sets for fact versus expression, atom versus path, and named argument versus object pair. Tree-sitter 0.26.9 reports these sets as unnecessary for the current corpus; they document the shared prefixes used by editor-incomplete input.

## Phase B emitter

`emit_grammar.pl` uses a `read_term/3` loop. Reader-affecting `op/3` and `back_quotes` directives are applied as encountered. DCG clauses are collected as `(Head --> Body)` terms. The mechanical lowering supports:

| DCG term shape | Emitted Tree-sitter shape |
|---|---|
| `(A, B)` | `seq(A, B)` |
| `(A ; B)` | `choice(A, B)` |
| code list with fixed integers | JSON-escaped literal token |
| call to another discovered DCG head | `$.rule_name` |
| `[]` | `blank()` |
| `{semantic_action}` and cut | `blank()` plus overlay requirement |

Variable code lists, `code_type/2`, higher-order `call/2`, operator tables, and semantic conditions stop mechanical lowering. Run:

```text
$ swipl -q -f emit_grammar.pl -- ../../prolog/compile/parse_dl_dcg.pl emitted-grammar.js
DCG_EMIT clauses=108 translatable=34 rule_names=75 output=emitted-grammar.js
```

### Emitted versus hand rule classification

No hand rule is emitted identically. The table classifies every rule in `grammar.js`.

| Hand rule | Classification | Reason |
|---|---|---|
| `source_file` | HAND-ONLY | Tree-sitter root and recovery boundary. |
| `statement` | EMITTED-NEEDS-OVERLAY | DCG statement dispatch threads semantic state and findings. |
| `relation_declaration` | EMITTED-NEEDS-OVERLAY | DCG terminals exist; enum-versus-column choice and CST fields require overlay. |
| `shell_declaration` | EMITTED-NEEDS-OVERLAY | Template syntax is extractable; input/output CST shape requires overlay. |
| `bind_declaration` | EMITTED-NEEDS-OVERLAY | Concrete tokens are extractable; declaration CST shape requires overlay. |
| `declaration_parameter` | HAND-ONLY | Corpus permits typed and untyped generated columns. |
| `column` | EMITTED-NEEDS-OVERLAY | Higher-order typed-column DCG erases the CST field split. |
| `type` | EMITTED-NEEDS-OVERLAY | Scalar registry predicate and recursive wrappers need regex and recursion overlays. |
| `enum_variants` | EMITTED-NEEDS-OVERLAY | One base clause emits; semicolon recursion does not emit mechanically. |
| `enum_variant` | EMITTED-NEEDS-OVERLAY | Semantic construction and parameterized argument parsing require overlay. |
| `relation_modifier` | EMITTED-NEEDS-OVERLAY | Empty modifier clause emits; `log`, `keep`, and `key` branches need overlay. |
| `rule` | EMITTED-NEEDS-OVERLAY | Arrow terminals exist; variable-table and source-recording actions require overlay. |
| `fact` | HAND-ONLY | The DCG lowers a fact into a rule with a true body. |
| `query` | EMITTED-NEEDS-OVERLAY | Query DCG threads variables and constructs a semantic query term. |
| `match_statement` | EMITTED-NEEDS-OVERLAY | Arm syntax exists; exhaustiveness and expansion actions require overlay. |
| `match_arm` | EMITTED-NEEDS-OVERLAY | Arrow alternatives need explicit CST fields and precedence. |
| `goal_list` | EMITTED-NEEDS-OVERLAY | Parameterized separator combinator cannot preserve the body-item node. |
| `expression` | EMITTED-NEEDS-OVERLAY | Arithmetic tiers come from runtime registry data. |
| `binding_expression` | HAND-ONLY | Operator category and right precedence are Tree-sitter declarations. |
| `comparison_expression` | HAND-ONLY | Operator spellings and left precedence require overlay. |
| `binary_expression` | HAND-ONLY | Tier order and associativity come from registry facts and Tree-sitter precedence. |
| `unary_expression` | HAND-ONLY | Prefix precedence is absent from the emitted subset. |
| `member_expression` | EMITTED-NEEDS-OVERLAY | DCG dot-chain actions construct nested terms; immediate lexical access prevents statement-dot capture. |
| `member_access` | HAND-ONLY | Immediate regex token has no DCG term equivalent. |
| `parenthesized_expression` | EMITTED-NEEDS-OVERLAY | Fixed delimiters emit; expression precedence requires overlay. |
| `atom` | EMITTED-NEEDS-OVERLAY | Dotted path and arguments emit partially; call precedence and fields require overlay. |
| `named_argument` | EMITTED-NEEDS-OVERLAY | DCG semantic wrapper `named/2` does not specify CST fields. |
| `object_pattern` | EMITTED-NEEDS-OVERLAY | Brace recursion and semantic JSON terms need a Tree-sitter node boundary. |
| `object_pair` | EMITTED-NEEDS-OVERLAY | Typed captures and computed keys require explicit alternatives. |
| `capture_key` | HAND-ONLY | Regex-shaped token replaces variable-sensitive character parsing. |
| `list` | EMITTED-NEEDS-OVERLAY | Parameterized separator DCG does not determine spread-element precedence. |
| `spread_element` | HAND-ONLY | `...` is represented through semantic JSON terms. |
| `path` | EMITTED-NEEDS-OVERLAY | Dot-chain DCG constructs terms; CST retains segments. |
| `literal` | EMITTED-NEEDS-OVERLAY | Ordered DCG alternatives require named CST nodes. |
| `integer` | HAND-ONLY | `code_type/2` digit predicate becomes a regex. |
| `float` | HAND-ONLY | Character predicates, finite-float validation, and exponent shape become a regex. |
| `string` | HAND-ONLY | Variable code recursion and escape decoding become a regex token. |
| `quoted_atom` | HAND-ONLY | Variable code recursion and escape decoding become a regex token. |
| `template` | EMITTED-NEEDS-OVERLAY | Several fixed escape clauses emit; arbitrary character recursion requires a regex. |
| `boolean` | EMITTED-NEEDS-OVERLAY | Registry/atom conversion action hides the fixed lexical alternatives. |
| `variable` | HAND-ONLY | Variable identity bookkeeping becomes a lexical split. |
| `identifier` | HAND-ONLY | `code_type/2` and generated-name disambiguation become a regex. |
| `comment` | HAND-ONLY | DCG whitespace discards comments; Tree-sitter retains a CST node. |

### Overlay measurement

```text
emitted-grammar.js  non-whitespace characters: 1407
grammar.js overlay  non-whitespace characters: 3862
ratio: 2.74
```

The complete hand grammar is counted as overlay because the emitted file supplies zero identical hand rules. The overlay grows per surface construct: declarations add rules, each expression family adds precedence code, JSON patterns add lexical alternatives, and statement forms add CST boundaries.

## Phase C candidate analysis

### Candidate receipts

| Candidate | Generator or server surface | Activity receipt | dl6 integration price |
|---|---|---|---|
| [Langium](https://github.com/eclipse-langium/langium) | Grammar language generates AST types, parser services, validation hooks, scoping/linking services, and an LSP framework. The package was at 4.3.1 in the survey. | Commit [`bbaa4b8`](https://github.com/eclipse-langium/langium/commit/bbaa4b836f6a55e37120384105928639dcf4d1b9), 2026-08-11. | Emit a complete `.langium` grammar, replace the stale slice, map its AST to compiler inputs, and forward compiler findings. Tree-sitter and Topiary remain separate targets. |
| [lsp-tree-sitter](https://github.com/neomutt/lsp-tree-sitter) | Shared Python library used by Termux and Mutt language servers; query/schema-driven completion and hover scaffolding. | Commit [`af5ed28`](https://github.com/neomutt/lsp-tree-sitter/commit/af5ed28ac7d3965d6f6ba9ba3dc3d94afda43c6b), 2026-08-01. | Supply dl6 queries, symbols, completion data, and a compiler subprocess bridge. |
| [SWI `prolog_lsp`](https://github.com/hargettp/prolog_lsp) | JSON-RPC/LSP implementation in SWI with stdio and TCP transports. The [SWI pack page](https://www.swi-prolog.org/pack/list?p=prolog_lsp) documents stdio operation. | Commit [`83d8c39`](https://github.com/hargettp/prolog_lsp/commit/83d8c392e52af1a131991bcdc1b849276674282a), 2026-07-16. | Reuse transport and document lifecycle; dl6 parsing, CST queries, capability handlers, URI/range conversion, and compiler result conversion remain dl6-specific. |
| [treelsp](https://github.com/dhrubomoy/treelsp) | TypeScript grammar-first generator advertising Tree-sitter grammar, typed AST, highlights, and LSP generation. | Commit [`c04e643`](https://github.com/dhrubomoy/treelsp/commit/c04e643bf3f96e3619eedc7dd1e2b3dbecd90a49), 2026-02-22. | Move the source description into treelsp's TypeScript semantic schema or emit that schema from the DCG, then add compiler diagnostics and resolution. |
| Compiler backend over stdio | `swipl` already parses dl6 and emits findings; the oracle and manifest scripts establish batch inputs and deterministic JSON output patterns. | Local compiler files at this worktree revision. | Add JSON-RPC framing, document state/version tracking, cancellation, UTF-16 range conversion, and handlers. Parsing and semantic diagnostics remain in the compiler. |

Tree-sitter itself generates and incrementally updates CSTs; its project describes CST parsing and edit updates, rather than semantic language services ([Tree-sitter documentation](https://tree-sitter.github.io/tree-sitter/using-parsers/1-getting-started.html)).

### Capability fan-out

| LSP capability | Primary source | Langium | lsp-tree-sitter | SWI transport + compiler | treelsp |
|---|---|---|---|---|---|
| Syntax diagnostics | CST/parser | Generated parser errors; custom wording hook | Tree-sitter `ERROR`/`MISSING` traversal | Compiler parse findings | Generated Tree-sitter traversal |
| Semantic diagnostics | Compiler | Compiler adapter publishes diagnostics | Compiler subprocess publishes diagnostics | Direct compiler call | Compiler adapter publishes diagnostics |
| Highlighting | CST queries | Generated semantic tokens or TextMate output plus grammar annotations | Tree-sitter highlight queries | Separate Tree-sitter queries | Generated highlight queries |
| Folding ranges | CST | AST/CST traversal handler | Tree-sitter query/visitor | Requires parser tree or compiler spans | Generated or query-backed handler |
| Document outline | CST plus declaration kinds | AST traversal | Tree-sitter symbol query | Compiler declaration rows and spans | Typed AST traversal |
| Go to definition | Compiler symbol table plus spans | Generated linking if relation references are modeled; compiler bridge otherwise | Hand query/index plus compiler data | Direct declaration/reference index handler | Semantic schema plus compiler index |
| Find references | Compiler index | Langium index after full cross-reference modeling | Hand index/query plus compiler data | Direct compiler index handler | Generated index where schema models references |
| Completion | CST position plus compiler declarations | Generated keyword/parser completion; compiler adds relation/column candidates | Hand completion schema and compiler candidates | Hand handler over parser expectation and declarations | Generated syntax completion plus semantic additions |
| Hover | Compiler types and declarations | Generated documentation hooks plus compiler data | Hand schema/query plus compiler data | Direct compiler handler | Generated AST hooks plus compiler data |
| Rename | Compiler reference index | Generated linking when all references are declared cross-references | Hand edits from compiler index | Direct index and workspace edit handler | Generated where references are modeled |
| Formatting | Topiary + CST | Separate Topiary process or a formatter service | Separate Topiary process | Separate Topiary process | Separate Topiary process |
| Code actions | Compiler findings | Map finding codes to actions | Hand finding-to-edit table | Direct finding-to-edit handler | Map finding codes to actions |

### Recommended all-in-one shot and price

Use Langium as the generated LSP shell, with a Phase-B-style emitter targeting `.langium`, and use the SWI compiler as the semantic backend. This tests the strongest all-in-one candidate because Langium supplies document lifecycle, parser services, AST types, indexing hooks, validation hooks, completion plumbing, and editor packaging from its grammar language. The existing `v6/dl/grammar/dl.langium` demonstrates the integration route but covers an older surface.

Price:

1. A second emitter target with its own lexical and precedence overlay. Phase B measures that Tree-sitter's overlay already exceeds emitted code, so the `.langium` target must receive an independent overlay measurement before adoption.
2. A lossless AST-to-compiler input bridge or a compiler invocation against document text.
3. Finding-to-LSP diagnostic conversion with URI, version, UTF-16 ranges, cancellation, and stale-result suppression.
4. Compiler-backed relation/column indexes for definition, references, rename, hover, and semantic completion.
5. Topiary remains the formatting provider and Tree-sitter remains the editor CST/highlight provider unless Langium replaces those consumers explicitly.

Design forks returned for ruling:

| Fork | Evidence requiring a ruling |
|---|---|
| Emit both Tree-sitter and Langium, or replace Tree-sitter with Langium's parser | Tree-sitter currently feeds Topiary and incremental CST consumers; Langium generates its own parser and AST. |
| Model relation references in Langium cross-reference syntax, or let the compiler own all linking | The current stale grammar intentionally uses plain IDs and resolves names in a bridge. Full generated navigation requires cross-reference declarations. |
| Invoke the compiler per document version, or keep a long-running SWI process | Per-version invocation simplifies cancellation and state; a resident process needs versioned state and reset rules. |
| Publish Tree-sitter syntax errors beside compiler findings, or only compiler findings | The parsers recover differently on incomplete editor text and can report overlapping ranges. |
