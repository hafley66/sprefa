# Verdict

| Claim | Verdict | Scope |
|---|---|---|
| tree-sitter parses dl6 | VIABLE-WITH-CAVEATS | The verbatim contiguous slice of `v6/dl/fixtures/golden-flex.dl6` at lines 175-236 parses with zero `ERROR` or `MISSING` nodes. |
| Topiary formats dl6 | VIABLE-WITH-CAVEATS | A focused declaration/fact/rule sample follows the stated law and a second formatting pass is byte-identical. |

The grammar covers the syntax in the selected 62-line slice. It does not yet cover the complete 630-line golden fixture, operator precedence, `match`, queries, `sh`, `bind`, templates, or CST blocks.

# Receipts

Tool discovery:

```text
$ tree-sitter --version
tree-sitter 0.26.9

$ topiary --version
bash: topiary: command not found

$ npx --yes tree-sitter-cli --version
tree-sitter 0.26.9
```

Topiary was obtained with the permitted lab-local installation:

```text
$ cargo install topiary-cli --root ./tools
Installed package `topiary-cli v0.7.3` (executable `topiary`)
```

Gate receipt:

```text
$ cd v6/labs/tree-sitter-door
$ ./run-tests.sh
PASS parse: golden-flex.dl6 lines 175-236 contain zero ERROR/MISSING nodes
PASS format: formatting law and idempotence
$ echo $?
0
```

`run-tests.sh` generates the parser, builds a local dynamic grammar, rejects any parse containing `ERROR` or `MISSING`, compares formatter output with `fixtures/format-expected.dl6`, and compares the first and second formatter passes. When the local Topiary binary is absent, it installs the pinned CLI under `./tools`.

# Existing grammar survey

The required starting-point survey examined:

| Candidate | Kept or dropped | Measured fit |
|---|---|---|
| `langston-barrett/tree-sitter-souffle` v0.5.0 | Dropped | Supplies Datalog declarations, atoms, rules, and expressions. Souffle declarations use dot directives and rules use `:-`; dl6 uses `rel`, `<-`, `<+`, typed named columns, modifiers, JSON patterns, `match`, `sh`, and `bind`. Replacing its top-level and expression layers would leave little reusable grammar. |
| `Rukiza/tree-sitter-prolog` | Dropped | Supplies atoms, variables, lists, and Prolog operators. Its README lists floats as unfinished, and it has no dl6 declaration, host, binding, query, JSON-pattern, or rule-arrow surface. |
| `foxy/tree-sitter-prolog` / Problog entry in Tree-sitter's parser list | Dropped | A Prolog-family concrete syntax with the same top-level mismatch. The selected golden slice needs dl6 relation declarations and JSON object patterns more than ISO Prolog term breadth. |
| No maintained standalone Datalog grammar found | Dropped | Search results led to Souffle and general Prolog grammars; no closer dl6 skeleton appeared. |

The lab therefore uses a minimal grammar derived from `parse_dl_dcg.pl` rather than copying one of these grammars.

# DCG to grammar.js mapping

| `grammar.js` rule | Reference parser | Mapping | Shape note |
|---|---|---|---|
| `source_file`, `statement` | `statements/7`, `statement/6`, lines 255-276 and 448-457 | Structural | The reference accumulates declarations, rules, queries, variable bindings, and source facts through arguments and semantic calls. Tree-sitter emits a concrete sequence and leaves those computations to a later pass. |
| `comment`, `extras` | `skip_ws/2`, `skip_to_eol/2`, lines 279-291 | Structural | Comments are retained as named CST nodes instead of discarded during whitespace skipping. |
| `identifier` | `ident/3`, `ident_rest_codes/3`, lines 314-324 | Close lexical mapping | The lab splits uppercase variables from lowercase identifiers because Tree-sitter tokens cannot perform `get_or_make_var/4` identity bookkeeping. |
| `integer`, `float` | `integer_lit/3`, `float_lit/3`, `float_codes//1`, lines 327-380 | Close lexical mapping | Numeric conversion and finite-float checks are semantic actions in the reference. The grammar recognizes source spelling only. |
| `string`, `quoted_atom` | `quoted_atom_lit/3`, `string_lit/3`, `quoted_chars/4`, lines 383-411 | Close lexical mapping | The regex tokens retain escapes. Decoding escape values remains outside the parser. |
| `relation_declaration` | `decl_a_stmt/3`, lines 459-497 | Direct concrete-syntax mapping | Reference-side declaration expansion into `col_type`, `kind`, paths, and unit declarations has no parser action equivalent. The CST retains declaration structure. |
| `column` | `decl_a_columns/3`, `decl_a_column/3`, lines 526-536 | 1:1 | Both accept comma-separated name/type column specifications. |
| `type` | `typed_column_type/3`, `typed_column_type_base/3`, lines 538-580 | Structural | One recursive node covers scalar, named, option, and list constructors. The reference uses ordered clauses and constructs Prolog terms. |
| `enum_variants`, `enum_variant` | `enum_decl_variants/3`, `enum_decl_variant/3`, lines 582-604 | 1:1 | The CST retains semicolon-separated variants without generating companion relations. |
| `relation_modifier` | `decl_a_modifiers/4`, `keep_clause/3`, `key_clause/3`, lines 512-524 and 641-660 | Direct concrete-syntax mapping | Modifier validation and declaration records remain later-pass work. |
| `rule`, `fact` | `rule_stmt/6`, lines 1028-1037 | Structural | The reference represents a fact as a rule with `true` body and resolves variables during parsing. The CST gives facts a distinct node and accepts GLR recovery while editing. |
| `atom`, `argument` | `head_atom/6`, `head_args/6`, `atom_arg/6`, lines 1040-1072 | Structural | Named-argument resolution depends on known declaration column order. The CST preserves named and positional arguments without resolving them. |
| `goal_list` | `body/6`, lines 1142-1154 | 1:1 for the selected slice | The lab slice uses comma-separated call goals. Parenthesized bodies and the complete body-item registry remain outside this grammar. |
| `path` | `dotted_path/3`, lines 1580-1585 | 1:1 | Module-name joining and collision handling are semantic passes in the reference. |
| `object_pattern`, `object_pair`, `capture_key` | `braces_term/6`, `brace_pairs/6`, `brace_pair/6`, `brace_key/6`, lines 1605-1638 | Structural | The reference creates Prolog brace terms, typed pairs, and variable holes. Recursive CST nodes represent the same source nesting without actions. |

Tree-sitter's GLR parser can retain multiple viable parses and recover during incomplete edits. The reference parser uses ordered alternatives, cuts, shared variable tables, declaration lookup, diagnostics, and semantic construction while consuming input. Full parity needs a CST-to-dl lowering pass for those operations.

# Full-arc effort estimate

| Work | Estimate | Receipt needed |
|---|---:|---|
| Complete grammar against all 630 golden lines and the conformance fixture corpus | 8-12 engineer-days | Zero-error corpus gate plus CST snapshots for every registered surface construct |
| Highlight queries | 2-3 engineer-days | Editor fixture snapshots for declarations, variables, calls, literals, patterns, errors, and comments |
| `sprefa-extract` integration | 5-8 engineer-days | Incremental edit tests, byte/point conversion tests, and extraction parity against the reference parser |
| Complete Topiary formatting law | 6-10 engineer-days | Golden formatter corpus, idempotence, comment attachment, malformed-input behavior, and line-width cases |
| Packaging and CI rails | 2-3 engineer-days | Pinned grammar ABI, reproducible builds, editor loading, and repository gate wiring |

Total: 23-36 engineer-days before integration review and corpus-driven corrections.
